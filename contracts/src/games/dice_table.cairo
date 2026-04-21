#[starknet::contract]
pub mod DiceTable {
    use core::hash::HashStateTrait;
    use core::poseidon::PoseidonTrait;
    use starknet::storage::{
        Map, StorageMapReadAccess, StorageMapWriteAccess, StoragePointerReadAccess,
        StoragePointerWriteAccess,
    };
    use starknet::{
        ContractAddress, get_block_number, get_block_timestamp, get_caller_address,
        get_contract_address,
    };
    use crate::interfaces::{
        IBankrollVaultDispatcher, IBankrollVaultDispatcherTrait, IDealerCommitmentDispatcher,
        IDealerCommitmentDispatcherTrait, IDiceTable, ISessionRegistryDispatcher,
        ISessionRegistryDispatcherTrait, ITableRegistryDispatcher, ITableRegistryDispatcherTrait,
    };
    use crate::types::{
        DiceCommitmentStatus, DiceRound, DiceSeedCommitment, GameKind, HandStatus, TableStatus,
    };

    const DENOMINATOR_BPS: u32 = 10000_u32;
    const DEFAULT_HOUSE_EDGE_BPS: u32 = 9900_u32;
    const DEFAULT_MIN_CHANCE_BPS: u32 = 100_u32;
    const DEFAULT_MAX_CHANCE_BPS: u32 = 9800_u32;
    const MULTIPLIER_SCALE_BPS: u128 = 10000_u128;
    const DICE_VAULT_ID_OFFSET: u64 = 1_000_000_000_u64;
    const MAX_REVEAL_DELAY_BLOCKS: u64 = 50_u64;
    const MAX_HOUSE_EXPOSURE_DIVISOR: u128 = 100_u128;
    const MAX_DICE_WAGER: u128 = 25_000_000_000_000_000_000_u128;
    const SERVER_SEED_DOMAIN: felt252 = 'MOROS_SERVER_SEED';
    const DICE_ROLL_DOMAIN: felt252 = 'MOROS_DICE_ROLL';

    #[event]
    #[derive(Drop, starknet::Event)]
    pub enum Event {
        OperatorUpdated: OperatorUpdated,
        DiceWagerCapUpdated: DiceWagerCapUpdated,
        DiceRiskConfigUpdated: DiceRiskConfigUpdated,
        DiceSeedCommitted: DiceSeedCommitted,
        DiceRoundOpened: DiceRoundOpened,
        DiceRoundSettled: DiceRoundSettled,
        DiceRoundVoided: DiceRoundVoided,
    }

    #[derive(Drop, starknet::Event)]
    pub struct OperatorUpdated {
        pub operator: ContractAddress,
        pub active: bool,
    }

    #[derive(Drop, starknet::Event)]
    pub struct DiceWagerCapUpdated {
        pub max_wager: u128,
    }

    #[derive(Drop, starknet::Event)]
    pub struct DiceRiskConfigUpdated {
        pub min_chance_bps: u32,
        pub max_chance_bps: u32,
        pub house_edge_bps: u32,
        pub max_payout: u128,
    }

    #[derive(Drop, starknet::Event)]
    pub struct DiceSeedCommitted {
        pub commitment_id: u64,
        pub server_seed_hash: felt252,
        pub reveal_deadline: u64,
    }

    #[derive(Drop, starknet::Event)]
    pub struct DiceRoundOpened {
        pub round_id: u64,
        pub table_id: u64,
        pub player: ContractAddress,
        pub wager: u128,
        pub commitment_id: u64,
        pub client_seed: felt252,
        pub target_bps: u32,
        pub roll_over: bool,
    }

    #[derive(Drop, starknet::Event)]
    pub struct DiceRoundSettled {
        pub round_id: u64,
        pub table_id: u64,
        pub player: ContractAddress,
        pub roll_bps: u32,
        pub payout: u128,
        pub win: bool,
    }

    #[derive(Drop, starknet::Event)]
    pub struct DiceRoundVoided {
        pub round_id: u64,
        pub commitment_id: u64,
    }

    #[storage]
    struct Storage {
        owner: ContractAddress,
        vault: ContractAddress,
        table_registry: ContractAddress,
        session_registry: ContractAddress,
        dealer_commitment: ContractAddress,
        operators: Map<ContractAddress, bool>,
        next_round_id: u64,
        next_commitment_id: u64,
        min_chance_bps: u32,
        max_chance_bps: u32,
        house_edge_bps: u32,
        max_payout: u128,
        wager_cap: u128,
        rounds: Map<u64, DiceRound>,
        commitments: Map<u64, DiceSeedCommitment>,
        round_for_commitment: Map<u64, u64>,
    }

    #[constructor]
    fn constructor(
        ref self: ContractState,
        owner: ContractAddress,
        vault: ContractAddress,
        table_registry: ContractAddress,
        session_registry: ContractAddress,
        dealer_commitment: ContractAddress,
        max_payout: u128,
    ) {
        self.owner.write(owner);
        self.vault.write(vault);
        self.table_registry.write(table_registry);
        self.session_registry.write(session_registry);
        self.dealer_commitment.write(dealer_commitment);
        self.next_round_id.write(1_u64);
        self.next_commitment_id.write(1_u64);
        self.min_chance_bps.write(DEFAULT_MIN_CHANCE_BPS);
        self.max_chance_bps.write(DEFAULT_MAX_CHANCE_BPS);
        self.house_edge_bps.write(DEFAULT_HOUSE_EDGE_BPS);
        self.max_payout.write(max_payout);
        self.wager_cap.write(MAX_DICE_WAGER);
    }

    #[abi(embed_v0)]
    impl DiceTableImpl of IDiceTable<ContractState> {
        fn peek_next_round_id(self: @ContractState) -> u64 {
            self.next_round_id.read()
        }

        fn peek_next_commitment_id(self: @ContractState) -> u64 {
            self.next_commitment_id.read()
        }

        fn set_operator(ref self: ContractState, operator: ContractAddress, active: bool) {
            self.assert_owner();
            self.operators.write(operator, active);
            self.emit(OperatorUpdated { operator, active });
        }

        fn set_wager_cap(ref self: ContractState, max_wager: u128) {
            self.assert_owner();
            assert(max_wager > 0_u128, 'GAME_WAGER_CAP_ZERO');
            self.wager_cap.write(max_wager);
            self.emit(DiceWagerCapUpdated { max_wager });
        }

        fn get_wager_cap(self: @ContractState) -> u128 {
            self.wager_cap.read()
        }

        fn set_risk_config(
            ref self: ContractState,
            min_chance_bps: u32,
            max_chance_bps: u32,
            house_edge_bps: u32,
            max_payout: u128,
        ) {
            self.assert_owner();
            self.assert_risk_config(min_chance_bps, max_chance_bps, house_edge_bps, max_payout);
            self.min_chance_bps.write(min_chance_bps);
            self.max_chance_bps.write(max_chance_bps);
            self.house_edge_bps.write(house_edge_bps);
            self.max_payout.write(max_payout);
            self
                .emit(
                    DiceRiskConfigUpdated {
                        min_chance_bps, max_chance_bps, house_edge_bps, max_payout,
                    },
                );
        }

        fn commit_server_seed(
            ref self: ContractState, server_seed_hash: felt252, reveal_deadline: u64,
        ) -> u64 {
            self.assert_operator();
            assert(server_seed_hash != 0, 'SEED_HASH_ZERO');
            let block_number = get_block_number();
            assert(reveal_deadline > block_number, 'DEADLINE_IN_PAST');
            assert(reveal_deadline <= block_number + MAX_REVEAL_DELAY_BLOCKS, 'DEADLINE_TOO_LONG');

            let commitment_id = self.next_commitment_id.read();
            self.next_commitment_id.write(commitment_id + 1_u64);
            self
                .commitments
                .write(
                    commitment_id,
                    DiceSeedCommitment {
                        commitment_id,
                        server_seed_hash,
                        reveal_deadline,
                        status: DiceCommitmentStatus::Available,
                        round_id: 0_u64,
                    },
                );
            self.emit(DiceSeedCommitted { commitment_id, server_seed_hash, reveal_deadline });
            commitment_id
        }

        fn open_round(
            ref self: ContractState,
            table_id: u64,
            player: ContractAddress,
            session_key: ContractAddress,
            wager: u128,
            target_bps: u32,
            roll_over: bool,
            client_seed: felt252,
            commitment_id: u64,
        ) -> u64 {
            self.assert_player_access(player, session_key, wager);
            let table = ITableRegistryDispatcher { contract_address: self.table_registry.read() }
                .get_table(table_id);
            assert(table.status == TableStatus::Active, 'TABLE_NOT_ACTIVE');
            assert(table.game_kind == GameKind::Dice, 'TABLE_NOT_DICE');
            assert(table.table_contract == get_contract_address(), 'TABLE_ADDR_MISMATCH');
            assert(wager >= table.min_wager, 'WAGER_TOO_LOW');
            assert(wager <= table.max_wager, 'WAGER_TOO_HIGH');
            assert(wager <= self.wager_cap.read(), 'GAME_WAGER_CAP');

            let mut commitment = self.commitments.read(commitment_id);
            assert(commitment.commitment_id == commitment_id, 'COMMITMENT_NOT_FOUND');
            assert(commitment.status == DiceCommitmentStatus::Available, 'COMMITMENT_UNAVAILABLE');
            assert(commitment.reveal_deadline > get_block_number(), 'COMMITMENT_EXPIRED');

            let chance_bps = self.chance_for(target_bps, roll_over);
            let multiplier_bps = self.multiplier_for(chance_bps);
            let payout = self.payout_for(wager, chance_bps);
            assert(payout <= self.max_payout.read(), 'PAYOUT_LIMIT');
            let exposure = self.exposure_for(wager, payout);
            self.assert_dynamic_house_exposure(exposure);

            let round_id = self.next_round_id.read();
            self.next_round_id.write(round_id + 1_u64);
            let vault_round_id = self.vault_round_id(table_id, round_id);
            commitment.status = DiceCommitmentStatus::Locked;
            commitment.round_id = round_id;
            self.commitments.write(commitment_id, commitment);
            self.round_for_commitment.write(commitment_id, round_id);

            if wager > 0_u128 {
                IBankrollVaultDispatcher { contract_address: self.vault.read() }
                    .reserve_for_hand(player, vault_round_id, wager);
            }
            if exposure > 0_u128 {
                IBankrollVaultDispatcher { contract_address: self.vault.read() }
                    .lock_house_exposure(vault_round_id, exposure);
            }
            IDealerCommitmentDispatcher { contract_address: self.dealer_commitment.read() }
                .post_hand_commitment(
                    vault_round_id,
                    table_id,
                    commitment.server_seed_hash,
                    commitment.reveal_deadline,
                    false,
                    false,
                );

            let round = DiceRound {
                round_id,
                table_id,
                player,
                wager,
                status: HandStatus::Active,
                transcript_root: commitment.server_seed_hash,
                commitment_id,
                server_seed_hash: commitment.server_seed_hash,
                client_seed,
                target_bps,
                roll_over,
                roll_bps: 0_u32,
                chance_bps,
                multiplier_bps,
                payout: 0_u128,
                win: false,
            };
            self.rounds.write(round_id, round);
            self
                .emit(
                    DiceRoundOpened {
                        round_id,
                        table_id,
                        player,
                        wager,
                        commitment_id,
                        client_seed,
                        target_bps,
                        roll_over,
                    },
                );
            round_id
        }

        fn settle_round(ref self: ContractState, round_id: u64, server_seed: felt252) {
            self.assert_operator();
            let mut round = self.rounds.read(round_id);
            assert(round.round_id == round_id, 'ROUND_NOT_FOUND');
            assert(round.status == HandStatus::Active, 'ROUND_NOT_ACTIVE');
            let mut commitment = self.commitments.read(round.commitment_id);
            assert(commitment.status == DiceCommitmentStatus::Locked, 'COMMITMENT_NOT_LOCKED');
            assert(commitment.round_id == round_id, 'COMMITMENT_ROUND');
            assert(
                self.hash_server_seed(server_seed) == commitment.server_seed_hash,
                'BAD_SERVER_SEED',
            );

            let roll_bps = self
                .roll_from_seed(server_seed, round.client_seed, round.player, round_id);
            let payout_quote = self.payout_for(round.wager, round.chance_bps);
            assert(payout_quote <= self.max_payout.read(), 'PAYOUT_LIMIT');
            let win = self.is_win(round.target_bps, round.roll_over, roll_bps);
            let final_payout = if win {
                payout_quote
            } else {
                0_u128
            };
            let vault_round_id = self.vault_round_id(round.table_id, round_id);

            IDealerCommitmentDispatcher { contract_address: self.dealer_commitment.read() }
                .record_reveal(vault_round_id);
            if round.wager > 0_u128 {
                IBankrollVaultDispatcher { contract_address: self.vault.read() }
                    .settle_hand(round.player, vault_round_id, final_payout);
            }
            IDealerCommitmentDispatcher { contract_address: self.dealer_commitment.read() }
                .close_transcript(vault_round_id);

            round.status = HandStatus::Settled;
            round.roll_bps = roll_bps;
            round.payout = final_payout;
            round.win = win;
            self.rounds.write(round_id, round);
            commitment.status = DiceCommitmentStatus::Revealed;
            self.commitments.write(round.commitment_id, commitment);
            self
                .emit(
                    DiceRoundSettled {
                        round_id,
                        table_id: round.table_id,
                        player: round.player,
                        roll_bps,
                        payout: final_payout,
                        win,
                    },
                );
        }

        fn void_expired_round(ref self: ContractState, round_id: u64) {
            let mut round = self.rounds.read(round_id);
            assert(round.round_id == round_id, 'ROUND_NOT_FOUND');
            assert(round.status == HandStatus::Active, 'ROUND_NOT_ACTIVE');
            let mut commitment = self.commitments.read(round.commitment_id);
            assert(get_block_number() > commitment.reveal_deadline, 'REVEAL_NOT_EXPIRED');
            let vault_round_id = self.vault_round_id(round.table_id, round_id);
            if round.wager > 0_u128 {
                IBankrollVaultDispatcher { contract_address: self.vault.read() }
                    .void_hand(round.player, vault_round_id);
            }
            round.status = HandStatus::Voided;
            round.payout = 0_u128;
            round.win = false;
            self.rounds.write(round_id, round);
            commitment.status = DiceCommitmentStatus::Voided;
            self.commitments.write(round.commitment_id, commitment);
            self.emit(DiceRoundVoided { round_id, commitment_id: round.commitment_id });
        }

        fn get_round(self: @ContractState, round_id: u64) -> DiceRound {
            let round = self.rounds.read(round_id);
            assert(round.round_id == round_id, 'ROUND_NOT_FOUND');
            round
        }

        fn get_round_for_commitment(self: @ContractState, commitment_id: u64) -> u64 {
            self.round_for_commitment.read(commitment_id)
        }

        fn get_commitment(self: @ContractState, commitment_id: u64) -> DiceSeedCommitment {
            let commitment = self.commitments.read(commitment_id);
            assert(commitment.commitment_id == commitment_id, 'COMMITMENT_NOT_FOUND');
            commitment
        }

        fn quote_payout(
            self: @ContractState, wager: u128, target_bps: u32, roll_over: bool,
        ) -> (u32, u32, u128, u128) {
            let chance_bps = self.chance_for(target_bps, roll_over);
            let multiplier_bps = self.multiplier_for(chance_bps);
            let payout = self.payout_for(wager, chance_bps);
            assert(payout <= self.max_payout.read(), 'PAYOUT_LIMIT');
            let exposure = self.exposure_for(wager, payout);
            (chance_bps, multiplier_bps, payout, exposure)
        }
    }

    #[generate_trait]
    impl InternalImpl of InternalTrait {
        fn assert_owner(self: @ContractState) {
            assert(get_caller_address() == self.owner.read(), 'OWNER_ONLY');
        }

        fn assert_operator(self: @ContractState) {
            let caller = get_caller_address();
            assert(caller == self.owner.read() || self.operators.read(caller), 'OPERATOR_ONLY');
        }

        fn assert_player_access(
            self: @ContractState,
            player: ContractAddress,
            session_key: ContractAddress,
            wager: u128,
        ) {
            let caller = get_caller_address();
            if caller != player {
                assert(caller == session_key, 'SESSION_CALLER');
                let allowed = ISessionRegistryDispatcher {
                    contract_address: self.session_registry.read(),
                }
                    .is_action_allowed(player, session_key, wager, get_block_timestamp());
                assert(allowed, 'SESSION_DENIED');
            }
        }

        fn assert_risk_config(
            self: @ContractState,
            min_chance_bps: u32,
            max_chance_bps: u32,
            house_edge_bps: u32,
            max_payout: u128,
        ) {
            assert(min_chance_bps > 0_u32, 'MIN_CHANCE_ZERO');
            assert(max_chance_bps < DENOMINATOR_BPS, 'MAX_CHANCE_HIGH');
            assert(min_chance_bps <= max_chance_bps, 'CHANCE_RANGE');
            assert(house_edge_bps > 0_u32, 'HOUSE_EDGE_ZERO');
            assert(house_edge_bps <= DENOMINATOR_BPS, 'HOUSE_EDGE_HIGH');
            assert(max_payout > 0_u128, 'MAX_PAYOUT_ZERO');
        }

        fn chance_for(self: @ContractState, target_bps: u32, roll_over: bool) -> u32 {
            assert(target_bps < DENOMINATOR_BPS, 'TARGET_OUT_OF_RANGE');
            let chance_bps = if roll_over {
                DENOMINATOR_BPS - target_bps - 1_u32
            } else {
                target_bps
            };
            assert(chance_bps >= self.min_chance_bps.read(), 'CHANCE_TOO_LOW');
            assert(chance_bps <= self.max_chance_bps.read(), 'CHANCE_TOO_HIGH');
            chance_bps
        }

        fn multiplier_for(self: @ContractState, chance_bps: u32) -> u32 {
            let raw = (self.house_edge_bps.read().into() * MULTIPLIER_SCALE_BPS)
                / chance_bps.into();
            raw.try_into().unwrap()
        }

        fn payout_for(self: @ContractState, wager: u128, chance_bps: u32) -> u128 {
            (wager * self.house_edge_bps.read().into()) / chance_bps.into()
        }

        fn exposure_for(self: @ContractState, wager: u128, payout: u128) -> u128 {
            if payout > wager {
                payout - wager
            } else {
                0_u128
            }
        }

        fn assert_dynamic_house_exposure(self: @ContractState, exposure: u128) {
            if exposure > 0_u128 {
                let available = IBankrollVaultDispatcher { contract_address: self.vault.read() }
                    .house_available();
                let cap = available / MAX_HOUSE_EXPOSURE_DIVISOR;
                assert(cap > 0_u128, 'HOUSE_LIQUIDITY_EMPTY');
                assert(exposure <= cap, 'HOUSE_EXPOSURE_CAP');
            }
        }

        fn is_win(self: @ContractState, target_bps: u32, roll_over: bool, roll_bps: u32) -> bool {
            if roll_over {
                roll_bps > target_bps
            } else {
                roll_bps < target_bps
            }
        }

        fn hash_server_seed(self: @ContractState, server_seed: felt252) -> felt252 {
            PoseidonTrait::new().update(SERVER_SEED_DOMAIN).update(server_seed).finalize()
        }

        fn roll_from_seed(
            self: @ContractState,
            server_seed: felt252,
            client_seed: felt252,
            player: ContractAddress,
            round_id: u64,
        ) -> u32 {
            let player_felt: felt252 = player.into();
            let mixed = PoseidonTrait::new()
                .update(DICE_ROLL_DOMAIN)
                .update(server_seed)
                .update(client_seed)
                .update(player_felt)
                .update(round_id.into())
                .finalize();
            let mixed_u256: u256 = mixed.into();
            let denom: u256 = DENOMINATOR_BPS.into();
            let roll_u256 = mixed_u256 % denom;
            roll_u256.low.try_into().unwrap()
        }

        fn vault_round_id(self: @ContractState, table_id: u64, round_id: u64) -> u64 {
            DICE_VAULT_ID_OFFSET + (table_id * DICE_VAULT_ID_OFFSET) + round_id
        }
    }
}
