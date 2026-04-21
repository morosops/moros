#[starknet::contract]
pub mod RouletteTable {
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
        IDealerCommitmentDispatcherTrait, IRouletteTable, ISessionRegistryDispatcher,
        ISessionRegistryDispatcherTrait, ITableRegistryDispatcher, ITableRegistryDispatcherTrait,
    };
    use crate::types::{
        DiceCommitmentStatus, DiceSeedCommitment, GameKind, HandStatus, RouletteBet, RouletteSpin,
        TableStatus,
    };

    const EUROPEAN_WHEEL_SIZE: u32 = 37_u32;
    const DEFAULT_HOUSE_EDGE_BPS: u32 = 9730_u32;
    const ROULETTE_VAULT_ID_OFFSET: u64 = 2_000_000_000_u64;
    const MAX_BETS: u8 = 8_u8;
    const STRAIGHT: u8 = 0_u8;
    const RED: u8 = 1_u8;
    const BLACK: u8 = 2_u8;
    const ODD: u8 = 3_u8;
    const EVEN: u8 = 4_u8;
    const LOW: u8 = 5_u8;
    const HIGH: u8 = 6_u8;
    const DOZEN: u8 = 7_u8;
    const COLUMN: u8 = 8_u8;
    const STREET: u8 = 9_u8;
    const SPLIT: u8 = 10_u8;
    const CORNER: u8 = 11_u8;
    const SIX_LINE: u8 = 12_u8;
    const TOP_LINE: u8 = 13_u8;
    const MAX_REVEAL_DELAY_BLOCKS: u64 = 50_u64;
    const MAX_HOUSE_EXPOSURE_DIVISOR: u128 = 100_u128;
    const MAX_ROULETTE_STRAIGHT_WAGER: u128 = 25_000_000_000_000_000_000_u128;
    const MAX_ROULETTE_DOZEN_COLUMN_WAGER: u128 = 70_000_000_000_000_000_000_u128;
    const MAX_ROULETTE_EVEN_MONEY_WAGER: u128 = 100_000_000_000_000_000_000_u128;
    const SERVER_SEED_DOMAIN: felt252 = 'MOROS_SERVER_SEED';
    const ROULETTE_SPIN_DOMAIN: felt252 = 'MOROS_ROULETTE_SPIN';

    #[event]
    #[derive(Drop, starknet::Event)]
    pub enum Event {
        OperatorUpdated: OperatorUpdated,
        RouletteBetCapsUpdated: RouletteBetCapsUpdated,
        RouletteRiskConfigUpdated: RouletteRiskConfigUpdated,
        RouletteSeedCommitted: RouletteSeedCommitted,
        RouletteSpinOpened: RouletteSpinOpened,
        RouletteSpinSettled: RouletteSpinSettled,
        RouletteSpinVoided: RouletteSpinVoided,
    }

    #[derive(Drop, starknet::Event)]
    pub struct OperatorUpdated {
        pub operator: ContractAddress,
        pub active: bool,
    }

    #[derive(Drop, starknet::Event)]
    pub struct RouletteBetCapsUpdated {
        pub straight_cap: u128,
        pub dozen_column_cap: u128,
        pub even_money_cap: u128,
    }

    #[derive(Drop, starknet::Event)]
    pub struct RouletteRiskConfigUpdated {
        pub house_edge_bps: u32,
        pub max_payout: u128,
    }

    #[derive(Drop, starknet::Event)]
    pub struct RouletteSeedCommitted {
        pub commitment_id: u64,
        pub server_seed_hash: felt252,
        pub reveal_deadline: u64,
    }

    #[derive(Drop, starknet::Event)]
    pub struct RouletteSpinOpened {
        pub spin_id: u64,
        pub table_id: u64,
        pub player: ContractAddress,
        pub wager: u128,
        pub commitment_id: u64,
        pub client_seed: felt252,
        pub bet_count: u8,
    }

    #[derive(Drop, starknet::Event)]
    pub struct RouletteSpinSettled {
        pub spin_id: u64,
        pub table_id: u64,
        pub player: ContractAddress,
        pub result_number: u8,
        pub payout: u128,
    }

    #[derive(Drop, starknet::Event)]
    pub struct RouletteSpinVoided {
        pub spin_id: u64,
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
        next_spin_id: u64,
        next_commitment_id: u64,
        house_edge_bps: u32,
        max_payout: u128,
        straight_bet_cap: u128,
        dozen_column_bet_cap: u128,
        even_money_bet_cap: u128,
        spins: Map<u64, RouletteSpin>,
        bets: Map<(u64, u8), RouletteBet>,
        commitments: Map<u64, DiceSeedCommitment>,
        spin_for_commitment: Map<u64, u64>,
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
        self.next_spin_id.write(1_u64);
        self.next_commitment_id.write(1_u64);
        self.house_edge_bps.write(DEFAULT_HOUSE_EDGE_BPS);
        self.max_payout.write(max_payout);
        self.straight_bet_cap.write(MAX_ROULETTE_STRAIGHT_WAGER);
        self.dozen_column_bet_cap.write(MAX_ROULETTE_DOZEN_COLUMN_WAGER);
        self.even_money_bet_cap.write(MAX_ROULETTE_EVEN_MONEY_WAGER);
    }

    #[abi(embed_v0)]
    impl RouletteTableImpl of IRouletteTable<ContractState> {
        fn peek_next_spin_id(self: @ContractState) -> u64 {
            self.next_spin_id.read()
        }

        fn peek_next_commitment_id(self: @ContractState) -> u64 {
            self.next_commitment_id.read()
        }

        fn set_operator(ref self: ContractState, operator: ContractAddress, active: bool) {
            self.assert_owner();
            self.operators.write(operator, active);
            self.emit(OperatorUpdated { operator, active });
        }

        fn set_bet_caps(
            ref self: ContractState,
            straight_cap: u128,
            dozen_column_cap: u128,
            even_money_cap: u128,
        ) {
            self.assert_owner();
            assert(straight_cap > 0_u128, 'STRAIGHT_CAP_ZERO');
            assert(dozen_column_cap > 0_u128, 'DOZEN_CAP_ZERO');
            assert(even_money_cap > 0_u128, 'EVEN_CAP_ZERO');
            self.straight_bet_cap.write(straight_cap);
            self.dozen_column_bet_cap.write(dozen_column_cap);
            self.even_money_bet_cap.write(even_money_cap);
            self.emit(RouletteBetCapsUpdated { straight_cap, dozen_column_cap, even_money_cap });
        }

        fn get_bet_caps(self: @ContractState) -> (u128, u128, u128) {
            (
                self.straight_bet_cap.read(),
                self.dozen_column_bet_cap.read(),
                self.even_money_bet_cap.read(),
            )
        }

        fn set_risk_config(ref self: ContractState, house_edge_bps: u32, max_payout: u128) {
            self.assert_owner();
            assert(house_edge_bps > 0_u32, 'HOUSE_EDGE_ZERO');
            assert(house_edge_bps <= DEFAULT_HOUSE_EDGE_BPS, 'HOUSE_EDGE_HIGH');
            assert(max_payout > 0_u128, 'MAX_PAYOUT_ZERO');
            self.house_edge_bps.write(house_edge_bps);
            self.max_payout.write(max_payout);
            self.emit(RouletteRiskConfigUpdated { house_edge_bps, max_payout });
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
            self.emit(RouletteSeedCommitted { commitment_id, server_seed_hash, reveal_deadline });
            commitment_id
        }

        fn open_spin(
            ref self: ContractState,
            table_id: u64,
            player: ContractAddress,
            session_key: ContractAddress,
            total_wager: u128,
            client_seed: felt252,
            commitment_id: u64,
            bet_count: u8,
            kind_0: u8,
            selection_0: u8,
            amount_0: u128,
            kind_1: u8,
            selection_1: u8,
            amount_1: u128,
            kind_2: u8,
            selection_2: u8,
            amount_2: u128,
            kind_3: u8,
            selection_3: u8,
            amount_3: u128,
            kind_4: u8,
            selection_4: u8,
            amount_4: u128,
            kind_5: u8,
            selection_5: u8,
            amount_5: u128,
            kind_6: u8,
            selection_6: u8,
            amount_6: u128,
            kind_7: u8,
            selection_7: u8,
            amount_7: u128,
        ) -> u64 {
            self.assert_player_access(player, session_key, total_wager);
            assert(bet_count > 0_u8, 'NO_BETS');
            assert(bet_count <= MAX_BETS, 'TOO_MANY_BETS');
            let table = ITableRegistryDispatcher { contract_address: self.table_registry.read() }
                .get_table(table_id);
            assert(table.status == TableStatus::Active, 'TABLE_NOT_ACTIVE');
            assert(table.game_kind == GameKind::Roulette, 'TABLE_NOT_ROULETTE');
            assert(table.table_contract == get_contract_address(), 'TABLE_ADDR_MISMATCH');
            assert(total_wager >= table.min_wager, 'WAGER_TOO_LOW');
            assert(total_wager <= table.max_wager, 'WAGER_TOO_HIGH');

            let mut commitment = self.commitments.read(commitment_id);
            assert(commitment.commitment_id == commitment_id, 'COMMITMENT_NOT_FOUND');
            assert(commitment.status == DiceCommitmentStatus::Available, 'COMMITMENT_UNAVAILABLE');
            assert(commitment.reveal_deadline > get_block_number(), 'COMMITMENT_EXPIRED');

            let spin_id = self.next_spin_id.read();
            let mut wager_sum = 0_u128;
            let mut max_payout = 0_u128;
            self
                .write_bet(
                    spin_id,
                    0_u8,
                    bet_count,
                    kind_0,
                    selection_0,
                    amount_0,
                    ref wager_sum,
                    ref max_payout,
                );
            self
                .write_bet(
                    spin_id,
                    1_u8,
                    bet_count,
                    kind_1,
                    selection_1,
                    amount_1,
                    ref wager_sum,
                    ref max_payout,
                );
            self
                .write_bet(
                    spin_id,
                    2_u8,
                    bet_count,
                    kind_2,
                    selection_2,
                    amount_2,
                    ref wager_sum,
                    ref max_payout,
                );
            self
                .write_bet(
                    spin_id,
                    3_u8,
                    bet_count,
                    kind_3,
                    selection_3,
                    amount_3,
                    ref wager_sum,
                    ref max_payout,
                );
            self
                .write_bet(
                    spin_id,
                    4_u8,
                    bet_count,
                    kind_4,
                    selection_4,
                    amount_4,
                    ref wager_sum,
                    ref max_payout,
                );
            self
                .write_bet(
                    spin_id,
                    5_u8,
                    bet_count,
                    kind_5,
                    selection_5,
                    amount_5,
                    ref wager_sum,
                    ref max_payout,
                );
            self
                .write_bet(
                    spin_id,
                    6_u8,
                    bet_count,
                    kind_6,
                    selection_6,
                    amount_6,
                    ref wager_sum,
                    ref max_payout,
                );
            self
                .write_bet(
                    spin_id,
                    7_u8,
                    bet_count,
                    kind_7,
                    selection_7,
                    amount_7,
                    ref wager_sum,
                    ref max_payout,
                );
            assert(wager_sum == total_wager, 'WAGER_SUM_MISMATCH');
            assert(max_payout <= self.max_payout.read(), 'PAYOUT_LIMIT');
            let exposure = max_payout - total_wager;
            self.assert_dynamic_house_exposure(exposure);

            self.next_spin_id.write(spin_id + 1_u64);
            let vault_spin_id = self.vault_spin_id(table_id, spin_id);
            commitment.status = DiceCommitmentStatus::Locked;
            commitment.round_id = spin_id;
            self.commitments.write(commitment_id, commitment);
            self.spin_for_commitment.write(commitment_id, spin_id);

            IBankrollVaultDispatcher { contract_address: self.vault.read() }
                .reserve_for_hand(player, vault_spin_id, total_wager);
            IBankrollVaultDispatcher { contract_address: self.vault.read() }
                .lock_house_exposure(vault_spin_id, exposure);
            IDealerCommitmentDispatcher { contract_address: self.dealer_commitment.read() }
                .post_hand_commitment(
                    vault_spin_id,
                    table_id,
                    commitment.server_seed_hash,
                    commitment.reveal_deadline,
                    false,
                    false,
                );

            self
                .spins
                .write(
                    spin_id,
                    RouletteSpin {
                        spin_id,
                        table_id,
                        player,
                        wager: total_wager,
                        status: HandStatus::Active,
                        transcript_root: commitment.server_seed_hash,
                        commitment_id,
                        server_seed_hash: commitment.server_seed_hash,
                        client_seed,
                        result_number: 0_u8,
                        bet_count,
                        payout: 0_u128,
                    },
                );
            self
                .emit(
                    RouletteSpinOpened {
                        spin_id,
                        table_id,
                        player,
                        wager: total_wager,
                        commitment_id,
                        client_seed,
                        bet_count,
                    },
                );
            spin_id
        }

        fn settle_spin(ref self: ContractState, spin_id: u64, server_seed: felt252) {
            self.assert_operator();
            let mut spin = self.spins.read(spin_id);
            assert(spin.spin_id == spin_id, 'SPIN_NOT_FOUND');
            assert(spin.status == HandStatus::Active, 'SPIN_NOT_ACTIVE');
            let mut commitment = self.commitments.read(spin.commitment_id);
            assert(commitment.status == DiceCommitmentStatus::Locked, 'COMMITMENT_NOT_LOCKED');
            assert(
                self.hash_server_seed(server_seed) == commitment.server_seed_hash,
                'BAD_SERVER_SEED',
            );
            let result_number = self
                .result_from_seed(server_seed, spin.client_seed, spin.player, spin_id);
            let payout = self.settle_bets(spin_id, spin.bet_count, result_number);
            assert(payout <= self.max_payout.read(), 'PAYOUT_LIMIT');
            let vault_spin_id = self.vault_spin_id(spin.table_id, spin_id);
            IDealerCommitmentDispatcher { contract_address: self.dealer_commitment.read() }
                .record_reveal(vault_spin_id);
            IBankrollVaultDispatcher { contract_address: self.vault.read() }
                .settle_hand(spin.player, vault_spin_id, payout);
            IDealerCommitmentDispatcher { contract_address: self.dealer_commitment.read() }
                .close_transcript(vault_spin_id);
            spin.status = HandStatus::Settled;
            spin.result_number = result_number;
            spin.payout = payout;
            self.spins.write(spin_id, spin);
            commitment.status = DiceCommitmentStatus::Revealed;
            self.commitments.write(spin.commitment_id, commitment);
            self
                .emit(
                    RouletteSpinSettled {
                        spin_id,
                        table_id: spin.table_id,
                        player: spin.player,
                        result_number,
                        payout,
                    },
                );
        }

        fn void_expired_spin(ref self: ContractState, spin_id: u64) {
            let mut spin = self.spins.read(spin_id);
            assert(spin.spin_id == spin_id, 'SPIN_NOT_FOUND');
            assert(spin.status == HandStatus::Active, 'SPIN_NOT_ACTIVE');
            let mut commitment = self.commitments.read(spin.commitment_id);
            assert(get_block_number() > commitment.reveal_deadline, 'REVEAL_NOT_EXPIRED');
            let vault_spin_id = self.vault_spin_id(spin.table_id, spin_id);
            IBankrollVaultDispatcher { contract_address: self.vault.read() }
                .void_hand(spin.player, vault_spin_id);
            spin.status = HandStatus::Voided;
            spin.payout = 0_u128;
            self.spins.write(spin_id, spin);
            commitment.status = DiceCommitmentStatus::Voided;
            self.commitments.write(spin.commitment_id, commitment);
            self.emit(RouletteSpinVoided { spin_id, commitment_id: spin.commitment_id });
        }

        fn get_spin(self: @ContractState, spin_id: u64) -> RouletteSpin {
            let spin = self.spins.read(spin_id);
            assert(spin.spin_id == spin_id, 'SPIN_NOT_FOUND');
            spin
        }

        fn get_bet(self: @ContractState, spin_id: u64, bet_index: u8) -> RouletteBet {
            self.bets.read((spin_id, bet_index))
        }

        fn get_spin_for_commitment(self: @ContractState, commitment_id: u64) -> u64 {
            self.spin_for_commitment.read(commitment_id)
        }

        fn get_commitment(self: @ContractState, commitment_id: u64) -> DiceSeedCommitment {
            let commitment = self.commitments.read(commitment_id);
            assert(commitment.commitment_id == commitment_id, 'COMMITMENT_NOT_FOUND');
            commitment
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

        fn assert_dynamic_house_exposure(self: @ContractState, exposure: u128) {
            if exposure > 0_u128 {
                let available = IBankrollVaultDispatcher { contract_address: self.vault.read() }
                    .house_available();
                let cap = available / MAX_HOUSE_EXPOSURE_DIVISOR;
                assert(cap > 0_u128, 'HOUSE_LIQUIDITY_EMPTY');
                assert(exposure <= cap, 'HOUSE_EXPOSURE_CAP');
            }
        }

        fn write_bet(
            ref self: ContractState,
            spin_id: u64,
            bet_index: u8,
            bet_count: u8,
            kind: u8,
            selection: u8,
            amount: u128,
            ref wager_sum: u128,
            ref max_payout: u128,
        ) {
            if bet_index < bet_count {
                assert(amount > 0_u128, 'BET_AMOUNT_ZERO');
                let multiplier = self.multiplier_for(kind, selection);
                let bet_cap = self.bet_cap_for(kind);
                if bet_cap > 0_u128 {
                    assert(amount <= bet_cap, 'BET_TYPE_CAP');
                }
                wager_sum += amount;
                max_payout += amount * multiplier;
                self
                    .bets
                    .write(
                        (spin_id, bet_index),
                        RouletteBet {
                            kind,
                            selection,
                            amount,
                            payout_multiplier: multiplier,
                            payout: 0_u128,
                            win: false,
                        },
                    );
            } else {
                assert(amount == 0_u128, 'UNUSED_BET_AMOUNT');
            }
        }

        fn settle_bets(
            ref self: ContractState, spin_id: u64, bet_count: u8, result_number: u8,
        ) -> u128 {
            let mut index = 0_u8;
            let mut total_payout = 0_u128;
            loop {
                if index >= bet_count {
                    break;
                }
                let mut bet = self.bets.read((spin_id, index));
                let win = self.bet_wins(bet.kind, bet.selection, result_number);
                let payout = if win {
                    (bet.amount * bet.payout_multiplier * self.house_edge_bps.read().into())
                        / DEFAULT_HOUSE_EDGE_BPS.into()
                } else {
                    0_u128
                };
                bet.win = win;
                bet.payout = payout;
                self.bets.write((spin_id, index), bet);
                total_payout += payout;
                index += 1_u8;
            }
            total_payout
        }

        fn multiplier_for(self: @ContractState, kind: u8, selection: u8) -> u128 {
            if kind == STRAIGHT {
                assert(selection <= 36_u8, 'BAD_STRAIGHT');
                return 36_u128;
            }
            if kind == RED
                || kind == BLACK
                || kind == ODD
                || kind == EVEN
                || kind == LOW
                || kind == HIGH {
                return 2_u128;
            }
            if kind == DOZEN {
                assert(selection >= 1_u8 && selection <= 3_u8, 'BAD_DOZEN');
                return 3_u128;
            }
            if kind == COLUMN {
                assert(selection >= 1_u8 && selection <= 3_u8, 'BAD_COLUMN');
                return 3_u128;
            }
            if kind == STREET {
                assert(selection < 12_u8, 'BAD_STREET');
                return 12_u128;
            }
            if kind == SPLIT {
                assert(self.valid_split_selection(selection), 'BAD_SPLIT');
                return 18_u128;
            }
            if kind == CORNER {
                assert(self.valid_corner_selection(selection), 'BAD_CORNER');
                return 9_u128;
            }
            if kind == SIX_LINE {
                assert(selection < 11_u8, 'BAD_SIX_LINE');
                return 6_u128;
            }
            if kind == TOP_LINE {
                assert(selection == 0_u8, 'BAD_TOP_LINE');
                return 9_u128;
            }
            panic!("BAD_BET_KIND");
        }

        fn bet_cap_for(self: @ContractState, kind: u8) -> u128 {
            if kind == STRAIGHT {
                return self.straight_bet_cap.read();
            }
            if kind == DOZEN || kind == COLUMN {
                return self.dozen_column_bet_cap.read();
            }
            if kind == RED
                || kind == BLACK
                || kind == ODD
                || kind == EVEN
                || kind == LOW
                || kind == HIGH {
                return self.even_money_bet_cap.read();
            }
            0_u128
        }

        fn bet_wins(self: @ContractState, kind: u8, selection: u8, result_number: u8) -> bool {
            if kind == STRAIGHT {
                return result_number == selection;
            }
            if result_number == 0_u8 {
                return false;
            }
            if kind == RED {
                return self.is_red(result_number);
            }
            if kind == BLACK {
                return !self.is_red(result_number);
            }
            if kind == ODD {
                return (result_number.into() % 2_u32) == 1_u32;
            }
            if kind == EVEN {
                return (result_number.into() % 2_u32) == 0_u32;
            }
            if kind == LOW {
                return result_number >= 1_u8 && result_number <= 18_u8;
            }
            if kind == HIGH {
                return result_number >= 19_u8 && result_number <= 36_u8;
            }
            if kind == DOZEN {
                return ((result_number - 1_u8) / 12_u8) + 1_u8 == selection;
            }
            if kind == COLUMN {
                return ((result_number - 1_u8) % 3_u8) + 1_u8 == selection;
            }
            if kind == STREET {
                return ((result_number - 1_u8) / 3_u8) == selection;
            }
            if kind == SPLIT {
                return self.split_wins(selection, result_number);
            }
            if kind == CORNER {
                return self.corner_wins(selection, result_number);
            }
            if kind == SIX_LINE {
                let start = (selection * 3_u8) + 1_u8;
                return result_number >= start && result_number < start + 6_u8;
            }
            if kind == TOP_LINE {
                return result_number == 0_u8
                    || result_number == 1_u8
                    || result_number == 2_u8
                    || result_number == 3_u8;
            }
            false
        }

        fn valid_split_selection(self: @ContractState, selection: u8) -> bool {
            if selection >= 100_u8 && selection <= 102_u8 {
                return true;
            }
            if selection >= 40_u8 {
                let start = selection - 40_u8;
                return start >= 1_u8 && start <= 35_u8 && (start % 3_u8) != 0_u8;
            }
            selection >= 1_u8 && selection <= 33_u8
        }

        fn split_wins(self: @ContractState, selection: u8, result_number: u8) -> bool {
            if selection >= 100_u8 && selection <= 102_u8 {
                let paired = (selection - 100_u8) + 1_u8;
                return result_number == 0_u8 || result_number == paired;
            }
            if selection >= 40_u8 {
                let start = selection - 40_u8;
                return result_number == start || result_number == start + 1_u8;
            }
            result_number == selection || result_number == selection + 3_u8
        }

        fn valid_corner_selection(self: @ContractState, selection: u8) -> bool {
            selection >= 1_u8 && selection <= 32_u8 && (selection % 3_u8) != 0_u8
        }

        fn corner_wins(self: @ContractState, selection: u8, result_number: u8) -> bool {
            result_number == selection || result_number == selection
                + 1_u8 || result_number == selection
                + 3_u8 || result_number == selection
                + 4_u8
        }

        fn is_red(self: @ContractState, value: u8) -> bool {
            value == 1_u8
                || value == 3_u8
                || value == 5_u8
                || value == 7_u8
                || value == 9_u8
                || value == 12_u8
                || value == 14_u8
                || value == 16_u8
                || value == 18_u8
                || value == 19_u8
                || value == 21_u8
                || value == 23_u8
                || value == 25_u8
                || value == 27_u8
                || value == 30_u8
                || value == 32_u8
                || value == 34_u8
                || value == 36_u8
        }

        fn hash_server_seed(self: @ContractState, server_seed: felt252) -> felt252 {
            PoseidonTrait::new().update(SERVER_SEED_DOMAIN).update(server_seed).finalize()
        }

        fn result_from_seed(
            self: @ContractState,
            server_seed: felt252,
            client_seed: felt252,
            player: ContractAddress,
            spin_id: u64,
        ) -> u8 {
            let player_felt: felt252 = player.into();
            let mixed = PoseidonTrait::new()
                .update(ROULETTE_SPIN_DOMAIN)
                .update(server_seed)
                .update(client_seed)
                .update(player_felt)
                .update(spin_id.into())
                .finalize();
            let mixed_u256: u256 = mixed.into();
            let denom: u256 = EUROPEAN_WHEEL_SIZE.into();
            let result = mixed_u256 % denom;
            result.low.try_into().unwrap()
        }

        fn vault_spin_id(self: @ContractState, table_id: u64, spin_id: u64) -> u64 {
            ROULETTE_VAULT_ID_OFFSET + (table_id * ROULETTE_VAULT_ID_OFFSET) + spin_id
        }
    }
}
