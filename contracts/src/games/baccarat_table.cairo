#[starknet::contract]
pub mod BaccaratTable {
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
        IBaccaratTable, IBankrollVaultDispatcher, IBankrollVaultDispatcherTrait,
        IDealerCommitmentDispatcher, IDealerCommitmentDispatcherTrait, ISessionRegistryDispatcher,
        ISessionRegistryDispatcherTrait, ITableRegistryDispatcher, ITableRegistryDispatcherTrait,
    };
    use crate::types::{
        BaccaratRound, DiceCommitmentStatus, DiceSeedCommitment, GameKind, HandStatus, TableStatus,
    };

    const BACCARAT_VAULT_ID_OFFSET: u64 = 3_000_000_000_u64;
    const PLAYER: u8 = 0_u8;
    const BANKER: u8 = 1_u8;
    const TIE: u8 = 2_u8;
    const HAND_PLAYER: u8 = 0_u8;
    const HAND_BANKER: u8 = 1_u8;
    const BACCARAT_SHOE_CARDS: u32 = 416_u32;
    const MAX_REVEAL_DELAY_BLOCKS: u64 = 50_u64;
    const SERVER_SEED_DOMAIN: felt252 = 'MOROS_SERVER_SEED';
    const BACCARAT_SHOE_DOMAIN: felt252 = 'MOROS_BACCARAT_SHOE';
    const BACCARAT_CARD_DOMAIN: felt252 = 'MOROS_BAC_CARD';
    const BACCARAT_TRANSCRIPT_DOMAIN: felt252 = 'MOROS_BAC_ROOT';
    const MAX_HOUSE_EXPOSURE_DIVISOR: u128 = 100_u128;
    const MAX_BACCARAT_WAGER: u128 = 100_000_000_000_000_000_000_u128;

    #[event]
    #[derive(Drop, starknet::Event)]
    pub enum Event {
        OperatorUpdated: OperatorUpdated,
        BaccaratWagerCapUpdated: BaccaratWagerCapUpdated,
        BaccaratRiskConfigUpdated: BaccaratRiskConfigUpdated,
        BaccaratSeedCommitted: BaccaratSeedCommitted,
        BaccaratRoundOpened: BaccaratRoundOpened,
        BaccaratRoundSettled: BaccaratRoundSettled,
        BaccaratRoundVoided: BaccaratRoundVoided,
    }

    #[derive(Drop, starknet::Event)]
    pub struct OperatorUpdated {
        pub operator: ContractAddress,
        pub active: bool,
    }

    #[derive(Drop, starknet::Event)]
    pub struct BaccaratWagerCapUpdated {
        pub max_wager: u128,
    }

    #[derive(Drop, starknet::Event)]
    pub struct BaccaratRiskConfigUpdated {
        pub max_payout: u128,
    }

    #[derive(Drop, starknet::Event)]
    pub struct BaccaratSeedCommitted {
        pub commitment_id: u64,
        pub server_seed_hash: felt252,
        pub reveal_deadline: u64,
    }

    #[derive(Drop, starknet::Event)]
    pub struct BaccaratRoundOpened {
        pub round_id: u64,
        pub table_id: u64,
        pub player: ContractAddress,
        pub wager: u128,
        pub bet_side: u8,
        pub commitment_id: u64,
        pub client_seed: felt252,
    }

    #[derive(Drop, starknet::Event)]
    pub struct BaccaratRoundSettled {
        pub round_id: u64,
        pub table_id: u64,
        pub player: ContractAddress,
        pub winner: u8,
        pub payout: u128,
    }

    #[derive(Drop, starknet::Event)]
    pub struct BaccaratRoundVoided {
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
        max_payout: u128,
        wager_cap: u128,
        rounds: Map<u64, BaccaratRound>,
        cards: Map<(u64, u8, u8), u8>,
        card_positions: Map<(u64, u8, u8), u16>,
        card_draw_indices: Map<(u64, u8, u8), u8>,
        card_attempts: Map<(u64, u8, u8), u8>,
        card_commitments: Map<(u64, u8, u8), felt252>,
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
        self.max_payout.write(max_payout);
        self.wager_cap.write(MAX_BACCARAT_WAGER);
    }

    #[abi(embed_v0)]
    impl BaccaratTableImpl of IBaccaratTable<ContractState> {
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
            self.emit(BaccaratWagerCapUpdated { max_wager });
        }

        fn get_wager_cap(self: @ContractState) -> u128 {
            self.wager_cap.read()
        }

        fn set_risk_config(ref self: ContractState, max_payout: u128) {
            self.assert_owner();
            assert(max_payout > 0_u128, 'MAX_PAYOUT_ZERO');
            self.max_payout.write(max_payout);
            self.emit(BaccaratRiskConfigUpdated { max_payout });
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
            self.emit(BaccaratSeedCommitted { commitment_id, server_seed_hash, reveal_deadline });
            commitment_id
        }

        fn open_round(
            ref self: ContractState,
            table_id: u64,
            player: ContractAddress,
            session_key: ContractAddress,
            wager: u128,
            bet_side: u8,
            client_seed: felt252,
            commitment_id: u64,
        ) -> u64 {
            self.assert_player_access(player, session_key, wager);
            assert(bet_side <= TIE, 'BAD_BET_SIDE');
            let table = ITableRegistryDispatcher { contract_address: self.table_registry.read() }
                .get_table(table_id);
            assert(table.status == TableStatus::Active, 'TABLE_NOT_ACTIVE');
            assert(table.game_kind == GameKind::Baccarat, 'TABLE_NOT_BACCARAT');
            assert(table.table_contract == get_contract_address(), 'TABLE_ADDR_MISMATCH');
            assert(wager >= table.min_wager, 'WAGER_TOO_LOW');
            assert(wager <= table.max_wager, 'WAGER_TOO_HIGH');
            assert(wager <= self.wager_cap.read(), 'GAME_WAGER_CAP');

            let mut commitment = self.commitments.read(commitment_id);
            assert(commitment.commitment_id == commitment_id, 'COMMITMENT_NOT_FOUND');
            assert(commitment.status == DiceCommitmentStatus::Available, 'COMMITMENT_UNAVAILABLE');
            assert(commitment.reveal_deadline > get_block_number(), 'COMMITMENT_EXPIRED');

            let max_payout = self.max_possible_payout(wager, bet_side);
            assert(max_payout <= self.max_payout.read(), 'PAYOUT_LIMIT');
            let exposure = max_payout - wager;
            self.assert_dynamic_house_exposure(exposure);
            let round_id = self.next_round_id.read();
            self.next_round_id.write(round_id + 1_u64);
            let vault_round_id = self.vault_round_id(table_id, round_id);
            commitment.status = DiceCommitmentStatus::Locked;
            commitment.round_id = round_id;
            self.commitments.write(commitment_id, commitment);
            self.round_for_commitment.write(commitment_id, round_id);

            IBankrollVaultDispatcher { contract_address: self.vault.read() }
                .reserve_for_hand(player, vault_round_id, wager);
            IBankrollVaultDispatcher { contract_address: self.vault.read() }
                .lock_house_exposure(vault_round_id, exposure);
            IDealerCommitmentDispatcher { contract_address: self.dealer_commitment.read() }
                .post_hand_commitment(
                    vault_round_id,
                    table_id,
                    commitment.server_seed_hash,
                    commitment.reveal_deadline,
                    false,
                    false,
                );

            self
                .rounds
                .write(
                    round_id,
                    BaccaratRound {
                        round_id,
                        table_id,
                        player,
                        wager,
                        status: HandStatus::Active,
                        transcript_root: commitment.server_seed_hash,
                        commitment_id,
                        server_seed_hash: commitment.server_seed_hash,
                        client_seed,
                        bet_side,
                        player_total: 0_u8,
                        banker_total: 0_u8,
                        player_card_count: 0_u8,
                        banker_card_count: 0_u8,
                        winner: 0_u8,
                        payout: 0_u128,
                    },
                );
            self
                .emit(
                    BaccaratRoundOpened {
                        round_id, table_id, player, wager, bet_side, commitment_id, client_seed,
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
            assert(
                self.hash_server_seed(server_seed) == commitment.server_seed_hash,
                'BAD_SERVER_SEED',
            );

            let (player_total, banker_total, player_count, banker_count, transcript_root) = self
                .draw_and_store(round_id, server_seed, round.client_seed, round.player);
            let winner = if player_total > banker_total {
                PLAYER
            } else if banker_total > player_total {
                BANKER
            } else {
                TIE
            };
            let payout = self.payout_for(round.wager, round.bet_side, winner);
            assert(payout <= self.max_payout.read(), 'PAYOUT_LIMIT');
            let vault_round_id = self.vault_round_id(round.table_id, round_id);
            IDealerCommitmentDispatcher { contract_address: self.dealer_commitment.read() }
                .record_reveal(vault_round_id);
            IBankrollVaultDispatcher { contract_address: self.vault.read() }
                .settle_hand(round.player, vault_round_id, payout);
            IDealerCommitmentDispatcher { contract_address: self.dealer_commitment.read() }
                .close_transcript(vault_round_id);

            round.status = HandStatus::Settled;
            round.player_total = player_total;
            round.banker_total = banker_total;
            round.player_card_count = player_count;
            round.banker_card_count = banker_count;
            round.transcript_root = transcript_root;
            round.winner = winner;
            round.payout = payout;
            self.rounds.write(round_id, round);
            commitment.status = DiceCommitmentStatus::Revealed;
            self.commitments.write(round.commitment_id, commitment);
            self
                .emit(
                    BaccaratRoundSettled {
                        round_id, table_id: round.table_id, player: round.player, winner, payout,
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
            IBankrollVaultDispatcher { contract_address: self.vault.read() }
                .void_hand(round.player, vault_round_id);
            round.status = HandStatus::Voided;
            self.rounds.write(round_id, round);
            commitment.status = DiceCommitmentStatus::Voided;
            self.commitments.write(round.commitment_id, commitment);
            self.emit(BaccaratRoundVoided { round_id, commitment_id: round.commitment_id });
        }

        fn get_round(self: @ContractState, round_id: u64) -> BaccaratRound {
            let round = self.rounds.read(round_id);
            assert(round.round_id == round_id, 'ROUND_NOT_FOUND');
            round
        }

        fn get_card(self: @ContractState, round_id: u64, hand_index: u8, card_index: u8) -> u8 {
            self.cards.read((round_id, hand_index, card_index))
        }

        fn get_card_position(
            self: @ContractState, round_id: u64, hand_index: u8, card_index: u8,
        ) -> u16 {
            self.card_positions.read((round_id, hand_index, card_index))
        }

        fn get_card_draw_index(
            self: @ContractState, round_id: u64, hand_index: u8, card_index: u8,
        ) -> u8 {
            self.card_draw_indices.read((round_id, hand_index, card_index))
        }

        fn get_card_attempt(
            self: @ContractState, round_id: u64, hand_index: u8, card_index: u8,
        ) -> u8 {
            self.card_attempts.read((round_id, hand_index, card_index))
        }

        fn get_card_commitment(
            self: @ContractState, round_id: u64, hand_index: u8, card_index: u8,
        ) -> felt252 {
            self.card_commitments.read((round_id, hand_index, card_index))
        }

        fn get_round_for_commitment(self: @ContractState, commitment_id: u64) -> u64 {
            self.round_for_commitment.read(commitment_id)
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

        fn draw_and_store(
            ref self: ContractState,
            round_id: u64,
            server_seed: felt252,
            client_seed: felt252,
            player: ContractAddress,
        ) -> (u8, u8, u8, u8, felt252) {
            let (p0_pos, p0, p0_attempt) = self
                .draw_unique_card(
                    server_seed,
                    client_seed,
                    player,
                    round_id,
                    0_u8,
                    0_u8,
                    0_u16,
                    0_u16,
                    0_u16,
                    0_u16,
                    0_u16,
                );
            let p0_commit = self
                .store_card(round_id, HAND_PLAYER, 0_u8, 0_u8, p0_pos, p0, p0_attempt);
            let (b0_pos, b0, b0_attempt) = self
                .draw_unique_card(
                    server_seed,
                    client_seed,
                    player,
                    round_id,
                    1_u8,
                    1_u8,
                    p0_pos,
                    0_u16,
                    0_u16,
                    0_u16,
                    0_u16,
                );
            let b0_commit = self
                .store_card(round_id, HAND_BANKER, 0_u8, 1_u8, b0_pos, b0, b0_attempt);
            let (p1_pos, p1, p1_attempt) = self
                .draw_unique_card(
                    server_seed,
                    client_seed,
                    player,
                    round_id,
                    2_u8,
                    2_u8,
                    p0_pos,
                    b0_pos,
                    0_u16,
                    0_u16,
                    0_u16,
                );
            let p1_commit = self
                .store_card(round_id, HAND_PLAYER, 1_u8, 2_u8, p1_pos, p1, p1_attempt);
            let (b1_pos, b1, b1_attempt) = self
                .draw_unique_card(
                    server_seed,
                    client_seed,
                    player,
                    round_id,
                    3_u8,
                    3_u8,
                    p0_pos,
                    b0_pos,
                    p1_pos,
                    0_u16,
                    0_u16,
                );
            let b1_commit = self
                .store_card(round_id, HAND_BANKER, 1_u8, 3_u8, b1_pos, b1, b1_attempt);

            let mut player_total = self.baccarat_total(p0, p1, 0_u8);
            let mut banker_total = self.baccarat_total(b0, b1, 0_u8);
            let mut player_count = 2_u8;
            let mut banker_count = 2_u8;
            let mut player_third = 0_u8;
            let mut player_third_position = 0_u16;
            let mut p2_commit: felt252 = 0;
            let mut b2_commit: felt252 = 0;

            let natural = player_total >= 8_u8 || banker_total >= 8_u8;
            if !natural {
                if player_total <= 5_u8 {
                    let (next_position, next_card, next_attempt) = self
                        .draw_unique_card(
                            server_seed,
                            client_seed,
                            player,
                            round_id,
                            4_u8,
                            4_u8,
                            p0_pos,
                            b0_pos,
                            p1_pos,
                            b1_pos,
                            0_u16,
                        );
                    player_third = next_card;
                    player_third_position = next_position;
                    p2_commit = self
                        .store_card(
                            round_id,
                            HAND_PLAYER,
                            2_u8,
                            4_u8,
                            player_third_position,
                            player_third,
                            next_attempt,
                        );
                    player_total = self.baccarat_total(p0, p1, player_third);
                    player_count = 3_u8;
                }

                if self
                    .banker_draws(
                        banker_total, player_count == 3_u8, self.card_value(player_third),
                    ) {
                    let (banker_draw_index, used_count) = if player_count == 3_u8 {
                        (5_u8, 5_u8)
                    } else {
                        (4_u8, 4_u8)
                    };
                    let (banker_third_position, banker_third, banker_third_attempt) = self
                        .draw_unique_card(
                            server_seed,
                            client_seed,
                            player,
                            round_id,
                            banker_draw_index,
                            used_count,
                            p0_pos,
                            b0_pos,
                            p1_pos,
                            b1_pos,
                            player_third_position,
                        );
                    b2_commit = self
                        .store_card(
                            round_id,
                            HAND_BANKER,
                            2_u8,
                            banker_draw_index,
                            banker_third_position,
                            banker_third,
                            banker_third_attempt,
                        );
                    banker_total = self.baccarat_total(b0, b1, banker_third);
                    banker_count = 3_u8;
                }
            }

            let transcript_root = self
                .baccarat_transcript_root(
                    self.hash_server_seed(server_seed),
                    client_seed,
                    player,
                    round_id,
                    p0_commit,
                    b0_commit,
                    p1_commit,
                    b1_commit,
                    p2_commit,
                    b2_commit,
                );
            (player_total, banker_total, player_count, banker_count, transcript_root)
        }

        fn store_card(
            ref self: ContractState,
            round_id: u64,
            hand_index: u8,
            card_index: u8,
            draw_index: u8,
            position: u16,
            card: u8,
            attempt: u8,
        ) -> felt252 {
            let commitment = PoseidonTrait::new()
                .update(BACCARAT_CARD_DOMAIN)
                .update(round_id.into())
                .update(hand_index.into())
                .update(card_index.into())
                .update(draw_index.into())
                .update(attempt.into())
                .update(position.into())
                .update(card.into())
                .finalize();
            self.cards.write((round_id, hand_index, card_index), card);
            self.card_positions.write((round_id, hand_index, card_index), position);
            self.card_draw_indices.write((round_id, hand_index, card_index), draw_index);
            self.card_attempts.write((round_id, hand_index, card_index), attempt);
            self.card_commitments.write((round_id, hand_index, card_index), commitment);
            commitment
        }

        fn baccarat_transcript_root(
            self: @ContractState,
            server_seed_hash: felt252,
            client_seed: felt252,
            player: ContractAddress,
            round_id: u64,
            player_0_commitment: felt252,
            banker_0_commitment: felt252,
            player_1_commitment: felt252,
            banker_1_commitment: felt252,
            player_2_commitment: felt252,
            banker_2_commitment: felt252,
        ) -> felt252 {
            let player_felt: felt252 = player.into();
            PoseidonTrait::new()
                .update(BACCARAT_TRANSCRIPT_DOMAIN)
                .update(server_seed_hash)
                .update(client_seed)
                .update(player_felt)
                .update(round_id.into())
                .update(player_0_commitment)
                .update(banker_0_commitment)
                .update(player_1_commitment)
                .update(banker_1_commitment)
                .update(player_2_commitment)
                .update(banker_2_commitment)
                .finalize()
        }

        fn banker_draws(
            self: @ContractState, total: u8, player_drew: bool, player_third_value: u8,
        ) -> bool {
            if !player_drew {
                return total <= 5_u8;
            }
            if total <= 2_u8 {
                return true;
            }
            if total == 3_u8 {
                return player_third_value != 8_u8;
            }
            if total == 4_u8 {
                return player_third_value >= 2_u8 && player_third_value <= 7_u8;
            }
            if total == 5_u8 {
                return player_third_value >= 4_u8 && player_third_value <= 7_u8;
            }
            if total == 6_u8 {
                return player_third_value == 6_u8 || player_third_value == 7_u8;
            }
            false
        }

        fn payout_for(self: @ContractState, wager: u128, bet_side: u8, winner: u8) -> u128 {
            if bet_side != winner {
                return 0_u128;
            }
            if winner == BANKER {
                return (wager * 195_u128) / 100_u128;
            }
            if winner == TIE {
                return wager * 9_u128;
            }
            wager * 2_u128
        }

        fn max_possible_payout(self: @ContractState, wager: u128, bet_side: u8) -> u128 {
            if bet_side == TIE {
                return wager * 9_u128;
            }
            if bet_side == BANKER {
                return (wager * 195_u128) / 100_u128;
            }
            wager * 2_u128
        }

        fn baccarat_total(self: @ContractState, card_0: u8, card_1: u8, card_2: u8) -> u8 {
            (self.card_value(card_0) + self.card_value(card_1) + self.card_value(card_2)) % 10_u8
        }

        fn card_value(self: @ContractState, card: u8) -> u8 {
            if card == 0_u8 {
                return 0_u8;
            }
            if card >= 10_u8 {
                return 0_u8;
            }
            card
        }

        fn draw_unique_card(
            self: @ContractState,
            server_seed: felt252,
            client_seed: felt252,
            player: ContractAddress,
            round_id: u64,
            draw_index: u8,
            used_count: u8,
            used_0: u16,
            used_1: u16,
            used_2: u16,
            used_3: u16,
            used_4: u16,
        ) -> (u16, u8, u8) {
            let mut attempt = 0_u8;
            loop {
                let position = self
                    .shoe_position_from_entropy(
                        server_seed, client_seed, player, round_id, draw_index, attempt,
                    );
                if !self
                    .position_already_used(
                        position, used_count, used_0, used_1, used_2, used_3, used_4,
                    ) {
                    return (position, self.card_from_shoe_position(position), attempt);
                }
                attempt += 1_u8;
                assert(attempt < 32_u8, 'SHOE_DRAW_COLLISION');
            }
        }

        fn position_already_used(
            self: @ContractState,
            position: u16,
            used_count: u8,
            used_0: u16,
            used_1: u16,
            used_2: u16,
            used_3: u16,
            used_4: u16,
        ) -> bool {
            if used_count > 0_u8 && position == used_0 {
                return true;
            }
            if used_count > 1_u8 && position == used_1 {
                return true;
            }
            if used_count > 2_u8 && position == used_2 {
                return true;
            }
            if used_count > 3_u8 && position == used_3 {
                return true;
            }
            if used_count > 4_u8 && position == used_4 {
                return true;
            }
            false
        }

        fn shoe_position_from_entropy(
            self: @ContractState,
            server_seed: felt252,
            client_seed: felt252,
            player: ContractAddress,
            round_id: u64,
            draw_index: u8,
            attempt: u8,
        ) -> u16 {
            let player_felt: felt252 = player.into();
            let mixed = PoseidonTrait::new()
                .update(BACCARAT_SHOE_DOMAIN)
                .update(server_seed)
                .update(client_seed)
                .update(player_felt)
                .update(round_id.into())
                .update(draw_index.into())
                .update(attempt.into())
                .finalize();
            let mixed_u256: u256 = mixed.into();
            let denom: u256 = BACCARAT_SHOE_CARDS.into();
            let result = mixed_u256 % denom;
            result.low.try_into().unwrap()
        }

        fn card_from_shoe_position(self: @ContractState, position: u16) -> u8 {
            let zero_based = position % 52_u16;
            let rank_zero_based: u8 = (zero_based % 13_u16).try_into().unwrap();
            rank_zero_based + 1_u8
        }

        fn hash_server_seed(self: @ContractState, server_seed: felt252) -> felt252 {
            PoseidonTrait::new().update(SERVER_SEED_DOMAIN).update(server_seed).finalize()
        }

        fn vault_round_id(self: @ContractState, table_id: u64, round_id: u64) -> u64 {
            BACCARAT_VAULT_ID_OFFSET + (table_id * BACCARAT_VAULT_ID_OFFSET) + round_id
        }
    }
}
