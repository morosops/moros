#[starknet::contract]
pub mod BlackjackTable {
    use core::panic_with_felt252;
    use garaga::hashes::poseidon_bn254::poseidon_hash_2;
    use starknet::storage::{
        Map, StorageMapReadAccess, StorageMapWriteAccess, StoragePointerReadAccess,
        StoragePointerWriteAccess,
    };
    use starknet::{ContractAddress, get_block_number, get_block_timestamp, get_caller_address};
    use crate::games::blackjack_logic;
    use crate::interfaces::{
        IBankrollVaultDispatcher, IBankrollVaultDispatcherTrait, IBlackjackTable,
        IDealerPeekGroth16VerifierDispatcher, IDealerPeekGroth16VerifierDispatcherTrait,
        IDeckCommitmentDispatcher, IDeckCommitmentDispatcherTrait, ISessionRegistryDispatcher,
        ISessionRegistryDispatcherTrait, ITableRegistryDispatcher, ITableRegistryDispatcherTrait,
    };
    use crate::types::{
        BlackjackCardRevealProof, BlackjackHand, BlackjackSeat, HandOutcome, HandStatus,
        PlayerAction, SeatStatus, TableStatus,
    };

    const CARD_TREE_DEPTH: u8 = 9_u8;
    const CARD_TREE_SIZE: u64 = 312_u64;
    const BLACKJACK_TIMEOUT_BLOCKS: u64 = 50_u64;
    const MAX_HOUSE_EXPOSURE_DIVISOR: u128 = 100_u128;
    const BLACKJACK_BASE_LOCK_FACTOR: u128 = 8_u128;
    const BLACKJACK_ACE_UPCARD_LOCK_FACTOR: u128 = 9_u128;
    const MAX_BLACKJACK_WAGER: u128 = 100_000_000_000_000_000_000_u128;

    #[event]
    #[derive(Drop, starknet::Event)]
    pub enum Event {
        WagerCapUpdated: WagerCapUpdated,
        HandOpened: HandOpened,
        PlayerActionRecorded: PlayerActionRecorded,
        DealerCardRevealed: DealerCardRevealed,
        HandResolved: HandResolved,
        HandVoided: HandVoided,
    }

    #[derive(Drop, starknet::Event)]
    pub struct WagerCapUpdated {
        pub max_wager: u128,
    }

    #[derive(Drop, starknet::Event)]
    pub struct HandOpened {
        pub hand_id: u64,
        pub table_id: u64,
        pub player: ContractAddress,
        pub wager: u128,
        pub transcript_root: u256,
    }

    #[derive(Drop, starknet::Event)]
    pub struct PlayerActionRecorded {
        pub hand_id: u64,
        pub player: ContractAddress,
        pub seat_index: u8,
        pub action: PlayerAction,
        pub new_total: u8,
    }

    #[derive(Drop, starknet::Event)]
    pub struct DealerCardRevealed {
        pub hand_id: u64,
        pub card_index: u8,
        pub dealer_total: u8,
    }

    #[derive(Drop, starknet::Event)]
    pub struct HandResolved {
        pub hand_id: u64,
        pub player: ContractAddress,
        pub dealer_total: u8,
        pub total_payout: u128,
        pub primary_outcome: HandOutcome,
    }

    #[derive(Drop, starknet::Event)]
    pub struct HandVoided {
        pub hand_id: u64,
        pub player: ContractAddress,
    }

    #[storage]
    struct Storage {
        owner: ContractAddress,
        vault: ContractAddress,
        table_registry: ContractAddress,
        session_registry: ContractAddress,
        deck_commitment: ContractAddress,
        dealer_peek_verifier: ContractAddress,
        next_hand_id: u64,
        hands: Map<u64, BlackjackHand>,
        seats: Map<(u64, u8), BlackjackSeat>,
        split_aces: Map<(u64, u8), bool>,
        insurance_wagers: Map<u64, u128>,
        insurance_decided: Map<u64, bool>,
        player_cards: Map<(u64, u8, u8), u8>,
        dealer_cards: Map<(u64, u8), u8>,
        revealed_deck_indices: Map<(u64, u64), bool>,
        next_deck_indices: Map<u64, u64>,
        used_peek_hand_hashes: Map<u256, bool>,
        wager_cap: u128,
    }

    #[constructor]
    fn constructor(
        ref self: ContractState,
        owner: ContractAddress,
        vault: ContractAddress,
        table_registry: ContractAddress,
        session_registry: ContractAddress,
        deck_commitment: ContractAddress,
        dealer_peek_verifier: ContractAddress,
    ) {
        self.owner.write(owner);
        self.vault.write(vault);
        self.table_registry.write(table_registry);
        self.session_registry.write(session_registry);
        self.deck_commitment.write(deck_commitment);
        self.dealer_peek_verifier.write(dealer_peek_verifier);
        self.next_hand_id.write(1);
        self.wager_cap.write(MAX_BLACKJACK_WAGER);
    }

    #[abi(embed_v0)]
    impl BlackjackTableImpl of IBlackjackTable<ContractState> {
        fn peek_next_hand_id(self: @ContractState) -> u64 {
            self.next_hand_id.read()
        }

        fn set_wager_cap(ref self: ContractState, max_wager: u128) {
            self.assert_owner();
            assert(max_wager > 0_u128, 'GAME_WAGER_CAP_ZERO');
            self.wager_cap.write(max_wager);
            self.emit(WagerCapUpdated { max_wager });
        }

        fn get_wager_cap(self: @ContractState) -> u128 {
            self.wager_cap.read()
        }

        fn open_hand_verified(
            ref self: ContractState,
            table_id: u64,
            player: ContractAddress,
            wager: u128,
            transcript_root: u256,
            dealer_upcard: u8,
            dealer_upcard_proof: BlackjackCardRevealProof,
            player_first_card: u8,
            player_first_card_proof: BlackjackCardRevealProof,
            player_second_card: u8,
            player_second_card_proof: BlackjackCardRevealProof,
            dealer_peek_proof: Span<felt252>,
        ) -> u64 {
            let hand_id = self.next_hand_id.read();
            self
                .assert_ordered_card_reveal(
                    hand_id,
                    table_id,
                    player,
                    transcript_root,
                    player_first_card,
                    player_first_card_proof,
                    0,
                );
            self
                .assert_ordered_card_reveal(
                    hand_id,
                    table_id,
                    player,
                    transcript_root,
                    dealer_upcard,
                    dealer_upcard_proof,
                    1,
                );
            self
                .assert_ordered_card_reveal(
                    hand_id,
                    table_id,
                    player,
                    transcript_root,
                    player_second_card,
                    player_second_card_proof,
                    2,
                );
            let opened_hand_id = self
                .open_hand_internal(
                    table_id,
                    player,
                    wager,
                    transcript_root,
                    dealer_upcard,
                    player_first_card,
                    player_second_card,
                    dealer_peek_proof,
                );
            self.next_deck_indices.write(opened_hand_id, 4);
            opened_hand_id
        }

        fn submit_hit(
            ref self: ContractState,
            player: ContractAddress,
            hand_id: u64,
            seat_index: u8,
            drawn_card: u8,
        ) {
            self.assert_legacy_card_entrypoint_allowed();
            self.submit_hit_internal(player, hand_id, seat_index, drawn_card);
        }

        fn submit_hit_verified(
            ref self: ContractState,
            player: ContractAddress,
            hand_id: u64,
            seat_index: u8,
            drawn_card: u8,
            drawn_card_proof: BlackjackCardRevealProof,
        ) {
            let hand = self.load_hand(hand_id);
            let expected_deck_index = self.next_deck_indices.read(hand_id);
            self
                .assert_ordered_card_reveal(
                    hand_id,
                    hand.table_id,
                    hand.player,
                    hand.transcript_root,
                    drawn_card,
                    drawn_card_proof,
                    expected_deck_index,
                );
            self.next_deck_indices.write(hand_id, expected_deck_index + 1);
            self.submit_hit_internal(player, hand_id, seat_index, drawn_card);
        }

        fn submit_stand(
            ref self: ContractState, player: ContractAddress, hand_id: u64, seat_index: u8,
        ) {
            let (mut hand, mut seat) = self.assert_actionable_seat(player, hand_id, seat_index);
            seat.status = SeatStatus::Standing;
            seat.can_double = false;
            seat.can_split = false;
            hand.action_count += 1_u8;
            self.seats.write((hand_id, seat_index), seat);
            hand = self.advance_hand(hand);
            self.hands.write(hand_id, hand);
            self.record_transition(hand_id);
            self
                .emit(
                    PlayerActionRecorded {
                        hand_id,
                        player,
                        seat_index,
                        action: PlayerAction::Stand,
                        new_total: blackjack_logic::total_from_parts(
                            seat.hard_total, seat.ace_count,
                        ),
                    },
                );
        }

        fn submit_double(
            ref self: ContractState,
            player: ContractAddress,
            hand_id: u64,
            seat_index: u8,
            drawn_card: u8,
        ) {
            self.assert_legacy_card_entrypoint_allowed();
            self.submit_double_internal(player, hand_id, seat_index, drawn_card);
        }

        fn submit_double_verified(
            ref self: ContractState,
            player: ContractAddress,
            hand_id: u64,
            seat_index: u8,
            drawn_card: u8,
            drawn_card_proof: BlackjackCardRevealProof,
        ) {
            let hand = self.load_hand(hand_id);
            let expected_deck_index = self.next_deck_indices.read(hand_id);
            self
                .assert_ordered_card_reveal(
                    hand_id,
                    hand.table_id,
                    hand.player,
                    hand.transcript_root,
                    drawn_card,
                    drawn_card_proof,
                    expected_deck_index,
                );
            self.next_deck_indices.write(hand_id, expected_deck_index + 1);
            self.submit_double_internal(player, hand_id, seat_index, drawn_card);
        }

        fn submit_split(
            ref self: ContractState,
            player: ContractAddress,
            hand_id: u64,
            seat_index: u8,
            left_drawn_card: u8,
            right_drawn_card: u8,
        ) {
            self.assert_legacy_card_entrypoint_allowed();
            self
                .submit_split_internal(
                    player, hand_id, seat_index, left_drawn_card, right_drawn_card,
                );
        }

        fn submit_split_verified(
            ref self: ContractState,
            player: ContractAddress,
            hand_id: u64,
            seat_index: u8,
            left_drawn_card: u8,
            left_drawn_card_proof: BlackjackCardRevealProof,
            right_drawn_card: u8,
            right_drawn_card_proof: BlackjackCardRevealProof,
        ) {
            let hand = self.load_hand(hand_id);
            let expected_deck_index = self.next_deck_indices.read(hand_id);
            self
                .assert_ordered_card_reveal(
                    hand_id,
                    hand.table_id,
                    hand.player,
                    hand.transcript_root,
                    left_drawn_card,
                    left_drawn_card_proof,
                    expected_deck_index,
                );
            self
                .assert_ordered_card_reveal(
                    hand_id,
                    hand.table_id,
                    hand.player,
                    hand.transcript_root,
                    right_drawn_card,
                    right_drawn_card_proof,
                    expected_deck_index + 1,
                );
            self.next_deck_indices.write(hand_id, expected_deck_index + 2);
            self
                .submit_split_internal(
                    player, hand_id, seat_index, left_drawn_card, right_drawn_card,
                );
        }

        fn submit_take_insurance(
            ref self: ContractState, player: ContractAddress, hand_id: u64, dealer_blackjack: bool,
        ) {
            let _ = dealer_blackjack;
            let (mut hand, seat) = self.assert_insurance_window(player, hand_id);
            let actual_dealer_blackjack = self.dealer_blackjack_for_hand(hand_id);
            let insurance_wager = hand.wager / 2_u128;
            assert(insurance_wager > 0_u128, 'INSURANCE_NOT_ALLOWED');
            self.assert_session_total_wager(player, hand_id, insurance_wager);
            IBankrollVaultDispatcher { contract_address: self.vault.read() }
                .reserve_for_hand(player, hand_id, insurance_wager);
            IBankrollVaultDispatcher { contract_address: self.vault.read() }
                .lock_house_exposure(hand_id, insurance_wager * 2_u128);
            self.insurance_wagers.write(hand_id, insurance_wager);
            self.insurance_decided.write(hand_id, true);
            hand.action_count += 1_u8;
            hand = self.finish_insurance_decision(hand, actual_dealer_blackjack);
            self.hands.write(hand_id, hand);
            self.record_transition(hand_id);
            self
                .emit(
                    PlayerActionRecorded {
                        hand_id,
                        player,
                        seat_index: 0_u8,
                        action: PlayerAction::InsuranceTake,
                        new_total: blackjack_logic::total_from_parts(
                            seat.hard_total, seat.ace_count,
                        ),
                    },
                );
        }

        fn submit_decline_insurance(
            ref self: ContractState, player: ContractAddress, hand_id: u64, dealer_blackjack: bool,
        ) {
            let _ = dealer_blackjack;
            let (mut hand, seat) = self.assert_insurance_window(player, hand_id);
            let actual_dealer_blackjack = self.dealer_blackjack_for_hand(hand_id);
            self.insurance_wagers.write(hand_id, 0_u128);
            self.insurance_decided.write(hand_id, true);
            hand.action_count += 1_u8;
            hand = self.finish_insurance_decision(hand, actual_dealer_blackjack);
            self.hands.write(hand_id, hand);
            self.record_transition(hand_id);
            self
                .emit(
                    PlayerActionRecorded {
                        hand_id,
                        player,
                        seat_index: 0_u8,
                        action: PlayerAction::InsuranceDecline,
                        new_total: blackjack_logic::total_from_parts(
                            seat.hard_total, seat.ace_count,
                        ),
                    },
                );
        }

        fn submit_surrender(
            ref self: ContractState, player: ContractAddress, hand_id: u64, seat_index: u8,
        ) {
            let (mut hand, mut seat) = self.assert_actionable_seat(player, hand_id, seat_index);
            assert(hand.seat_count == 1_u8, 'SURRENDER_SPLIT_BLOCKED');
            assert(hand.split_count == 0_u8, 'SURRENDER_SPLIT_BLOCKED');
            assert(seat.card_count == 2_u8, 'SURRENDER_WINDOW_CLOSED');
            assert(!seat.doubled, 'SURRENDER_WINDOW_CLOSED');

            seat.status = SeatStatus::Surrendered;
            seat.can_double = false;
            seat.can_split = false;
            seat.outcome = HandOutcome::Surrender;
            seat.payout = blackjack_logic::payout_for_outcome(HandOutcome::Surrender, seat.wager);
            hand.status = HandStatus::Settled;
            hand.dealer_final_total = 0_u8;
            hand.action_count += 1_u8;

            self.seats.write((hand_id, seat_index), seat);
            self.hands.write(hand_id, hand);
            self.insurance_wagers.write(hand_id, 0_u128);
            self.insurance_decided.write(hand_id, false);

            IBankrollVaultDispatcher { contract_address: self.vault.read() }
                .settle_hand(player, hand_id, seat.payout);
            IDeckCommitmentDispatcher { contract_address: self.deck_commitment.read() }
                .close_transcript(hand_id);
            self
                .emit(
                    PlayerActionRecorded {
                        hand_id,
                        player,
                        seat_index,
                        action: PlayerAction::Surrender,
                        new_total: blackjack_logic::total_from_parts(
                            seat.hard_total, seat.ace_count,
                        ),
                    },
                );
            self
                .emit(
                    HandResolved {
                        hand_id,
                        player,
                        dealer_total: 0_u8,
                        total_payout: seat.payout,
                        primary_outcome: HandOutcome::Surrender,
                    },
                );
        }

        fn force_expired_insurance_decline(ref self: ContractState, hand_id: u64) {
            let mut hand = self.load_hand(hand_id);
            assert(hand.status == HandStatus::AwaitingInsurance, 'INSURANCE_WINDOW_CLOSED');
            self.assert_hand_deadline_expired(hand_id);

            let seat = self.seats.read((hand_id, 0_u8));
            self.insurance_wagers.write(hand_id, 0_u128);
            self.insurance_decided.write(hand_id, true);
            hand.action_count += 1_u8;
            hand = self.finish_insurance_decision(hand, self.dealer_blackjack_for_hand(hand_id));
            self.hands.write(hand_id, hand);
            self.record_transition(hand_id);
            self
                .emit(
                    PlayerActionRecorded {
                        hand_id,
                        player: hand.player,
                        seat_index: 0_u8,
                        action: PlayerAction::InsuranceDecline,
                        new_total: blackjack_logic::total_from_parts(
                            seat.hard_total, seat.ace_count,
                        ),
                    },
                );
        }

        fn force_expired_stand(ref self: ContractState, hand_id: u64) {
            let mut hand = self.load_hand(hand_id);
            assert(hand.status == HandStatus::Active, 'HAND_NOT_ACTIVE');
            self.assert_hand_deadline_expired(hand_id);

            let seat_index = hand.active_seat;
            let mut seat = self.seats.read((hand_id, seat_index));
            assert(seat.status == SeatStatus::Active, 'SEAT_NOT_PLAYABLE');
            seat.status = SeatStatus::Standing;
            seat.can_double = false;
            seat.can_split = false;
            hand.action_count += 1_u8;
            self.seats.write((hand_id, seat_index), seat);
            hand = self.advance_hand(hand);
            self.hands.write(hand_id, hand);
            self.record_transition(hand_id);
            self
                .emit(
                    PlayerActionRecorded {
                        hand_id,
                        player: hand.player,
                        seat_index,
                        action: PlayerAction::Stand,
                        new_total: blackjack_logic::total_from_parts(
                            seat.hard_total, seat.ace_count,
                        ),
                    },
                );
        }

        fn reveal_dealer_card(ref self: ContractState, hand_id: u64, drawn_card: u8) {
            self.assert_legacy_card_entrypoint_allowed();
            self.reveal_dealer_card_internal(hand_id, drawn_card);
        }

        fn reveal_dealer_card_verified(
            ref self: ContractState,
            hand_id: u64,
            drawn_card: u8,
            drawn_card_proof: BlackjackCardRevealProof,
        ) {
            let hand = self.load_hand(hand_id);
            let expected_deck_index = if hand.dealer_card_count == 1_u8 {
                3_u64
            } else {
                self.next_deck_indices.read(hand_id)
            };
            self
                .assert_ordered_card_reveal(
                    hand_id,
                    hand.table_id,
                    hand.player,
                    hand.transcript_root,
                    drawn_card,
                    drawn_card_proof,
                    expected_deck_index,
                );
            if hand.dealer_card_count != 1_u8 {
                self.next_deck_indices.write(hand_id, expected_deck_index + 1);
            }
            self.reveal_dealer_card_internal(hand_id, drawn_card);
        }

        fn finalize_hand(ref self: ContractState, hand_id: u64) {
            self.finalize_hand_internal(hand_id);
        }

        fn void_expired_hand(ref self: ContractState, hand_id: u64) {
            let mut hand = self.load_hand(hand_id);
            assert(hand.status == HandStatus::AwaitingDealer, 'DEALER_TIMEOUT_ONLY');
            self.assert_hand_deadline_expired(hand_id);

            IBankrollVaultDispatcher { contract_address: self.vault.read() }
                .void_hand(hand.player, hand_id);
            IDeckCommitmentDispatcher { contract_address: self.deck_commitment.read() }
                .close_transcript(hand_id);
            hand.status = HandStatus::Voided;
            hand.dealer_final_total = 0_u8;
            self.hands.write(hand_id, hand);
            self.insurance_wagers.write(hand_id, 0_u128);
            self.insurance_decided.write(hand_id, false);
            self.emit(HandVoided { hand_id, player: hand.player });
        }

        fn get_hand(self: @ContractState, hand_id: u64) -> BlackjackHand {
            self.load_hand(hand_id)
        }

        fn get_insurance_wager(self: @ContractState, hand_id: u64) -> u128 {
            self.insurance_wagers.read(hand_id)
        }

        fn get_seat(self: @ContractState, hand_id: u64, seat_index: u8) -> BlackjackSeat {
            self.seats.read((hand_id, seat_index))
        }

        fn get_player_card(
            self: @ContractState, hand_id: u64, seat_index: u8, card_index: u8,
        ) -> u8 {
            self.player_cards.read((hand_id, seat_index, card_index))
        }

        fn get_dealer_card(self: @ContractState, hand_id: u64, card_index: u8) -> u8 {
            self.dealer_cards.read((hand_id, card_index))
        }
    }

    #[generate_trait]
    impl InternalImpl of InternalTrait {
        fn open_hand_internal(
            ref self: ContractState,
            table_id: u64,
            player: ContractAddress,
            wager: u128,
            transcript_root: u256,
            dealer_upcard: u8,
            player_first_card: u8,
            player_second_card: u8,
            dealer_peek_proof: Span<felt252>,
        ) -> u64 {
            let table = ITableRegistryDispatcher { contract_address: self.table_registry.read() }
                .get_table(table_id);
            assert(table.status == TableStatus::Active, 'TABLE_NOT_ACTIVE');
            assert(wager >= table.min_wager, 'WAGER_TOO_LOW');
            assert(wager <= table.max_wager, 'WAGER_TOO_HIGH');
            assert(wager <= self.wager_cap.read(), 'GAME_WAGER_CAP');
            self.assert_dynamic_house_exposure(wager * self.blackjack_lock_factor(dealer_upcard));
            self.assert_action_access(player, wager);
            let hand_id = self.next_hand_id.read();
            let dealer_peek_required = dealer_upcard == 1_u8 || self.is_ten_value(dealer_upcard);
            let dealer_blackjack = if dealer_peek_required {
                self
                    .assert_dealer_peek_proof(
                        hand_id,
                        table_id,
                        player,
                        wager,
                        transcript_root,
                        dealer_upcard,
                        player_first_card,
                        player_second_card,
                        dealer_peek_proof,
                    )
            } else {
                false
            };
            self
                .assert_precommitted_hand(
                    hand_id, table_id, transcript_root, dealer_peek_required, dealer_blackjack,
                );

            self.next_hand_id.write(hand_id + 1);
            self.next_deck_indices.write(hand_id, 4);

            IBankrollVaultDispatcher { contract_address: self.vault.read() }
                .reserve_for_hand(player, hand_id, wager);

            let (dealer_hard_total, dealer_ace_count, _) = blackjack_logic::add_card(
                0_u8, 0_u8, dealer_upcard,
            );

            let mut seat = self.new_seat(wager);
            seat = self.store_player_card(hand_id, 0_u8, seat, player_first_card);
            self.record_reveal(hand_id);
            seat = self.store_player_card(hand_id, 0_u8, seat, player_second_card);
            self.record_reveal(hand_id);
            self.dealer_cards.write((hand_id, 0_u8), dealer_upcard);
            self.record_reveal(hand_id);

            let is_blackjack = blackjack_logic::is_blackjack(
                seat.card_count, seat.hard_total, seat.ace_count, 0_u8,
            );
            seat
                .can_split =
                    blackjack_logic::can_split_cards(player_first_card, player_second_card);
            if is_blackjack {
                seat.status = SeatStatus::Blackjack;
                seat.can_double = false;
                seat.can_split = false;
            }
            self.seats.write((hand_id, 0_u8), seat);
            self.split_aces.write((hand_id, 0_u8), false);
            IBankrollVaultDispatcher { contract_address: self.vault.read() }
                .lock_house_exposure(
                    hand_id,
                    if is_blackjack {
                        blackjack_logic::blackjack_bonus_liability(wager)
                    } else {
                        wager
                    },
                );

            self
                .hands
                .write(
                    hand_id,
                    BlackjackHand {
                        hand_id,
                        table_id,
                        player,
                        wager,
                        status: if dealer_upcard == 1_u8 {
                            HandStatus::AwaitingInsurance
                        } else if is_blackjack || dealer_blackjack {
                            HandStatus::AwaitingDealer
                        } else {
                            HandStatus::Active
                        },
                        transcript_root,
                        dealer_upcard,
                        dealer_card_count: 1_u8,
                        dealer_hard_total,
                        dealer_ace_count,
                        dealer_final_total: 0_u8,
                        action_count: 0_u8,
                        seat_count: 1_u8,
                        active_seat: 0_u8,
                        split_count: 0_u8,
                    },
                );
            self.emit(HandOpened { hand_id, table_id, player, wager, transcript_root });
            hand_id
        }

        fn submit_hit_internal(
            ref self: ContractState,
            player: ContractAddress,
            hand_id: u64,
            seat_index: u8,
            drawn_card: u8,
        ) {
            let (mut hand, mut seat) = self.assert_actionable_seat(player, hand_id, seat_index);
            seat = self.store_player_card(hand_id, seat_index, seat, drawn_card);
            self.record_reveal(hand_id);
            seat.can_double = false;
            seat.can_split = false;
            let total = blackjack_logic::total_from_parts(seat.hard_total, seat.ace_count);
            if total > 21_u8 {
                seat.status = SeatStatus::Busted;
            } else if total == 21_u8 {
                seat.status = SeatStatus::Standing;
            }
            hand.action_count += 1_u8;
            self.seats.write((hand_id, seat_index), seat);
            hand = self.advance_hand(hand);
            self.hands.write(hand_id, hand);
            self
                .emit(
                    PlayerActionRecorded {
                        hand_id, player, seat_index, action: PlayerAction::Hit, new_total: total,
                    },
                );
        }

        fn submit_double_internal(
            ref self: ContractState,
            player: ContractAddress,
            hand_id: u64,
            seat_index: u8,
            drawn_card: u8,
        ) {
            let (mut hand, mut seat) = self.assert_actionable_seat(player, hand_id, seat_index);
            assert(seat.can_double, 'DOUBLE_NOT_ALLOWED');
            let original_wager = seat.wager;
            self.assert_session_total_wager(player, hand_id, original_wager);
            IBankrollVaultDispatcher { contract_address: self.vault.read() }
                .reserve_for_hand(player, hand_id, original_wager);
            IBankrollVaultDispatcher { contract_address: self.vault.read() }
                .lock_house_exposure(hand_id, original_wager);
            seat.wager = original_wager + original_wager;
            seat.doubled = true;
            seat = self.store_player_card(hand_id, seat_index, seat, drawn_card);
            self.record_reveal(hand_id);
            seat.can_double = false;
            seat.can_split = false;
            let total = blackjack_logic::total_from_parts(seat.hard_total, seat.ace_count);
            if total > 21_u8 {
                seat.status = SeatStatus::Busted;
            } else {
                seat.status = SeatStatus::Standing;
            }
            hand.action_count += 1_u8;
            self.seats.write((hand_id, seat_index), seat);
            hand = self.advance_hand(hand);
            self.hands.write(hand_id, hand);
            self
                .emit(
                    PlayerActionRecorded {
                        hand_id, player, seat_index, action: PlayerAction::Double, new_total: total,
                    },
                );
        }

        fn submit_split_internal(
            ref self: ContractState,
            player: ContractAddress,
            hand_id: u64,
            seat_index: u8,
            left_drawn_card: u8,
            right_drawn_card: u8,
        ) {
            let (mut hand, seat) = self.assert_actionable_seat(player, hand_id, seat_index);
            assert(hand.seat_count < blackjack_logic::MAX_SEATS_PER_HAND, 'TABLE_SPLIT_LIMIT');
            assert(hand.split_count < blackjack_logic::MAX_SPLITS_PER_HAND, 'SPLIT_LIMIT');
            assert(seat.can_split, 'SPLIT_NOT_ALLOWED');
            assert(!self.split_aces.read((hand_id, seat_index)), 'ACES_NO_RESPLIT');
            assert(seat.card_count == 2_u8, 'SPLIT_HAND_STATE');

            let first_card = self.player_cards.read((hand_id, seat_index, 0_u8));
            let second_card = self.player_cards.read((hand_id, seat_index, 1_u8));
            assert(blackjack_logic::can_split_cards(first_card, second_card), 'SPLIT_PAIR_ONLY');

            self.assert_session_total_wager(player, hand_id, seat.wager);
            IBankrollVaultDispatcher { contract_address: self.vault.read() }
                .reserve_for_hand(player, hand_id, seat.wager);
            IBankrollVaultDispatcher { contract_address: self.vault.read() }
                .lock_house_exposure(hand_id, seat.wager);

            let split_aces = first_card == 1_u8 && second_card == 1_u8;
            let next_split_count = hand.split_count + 1_u8;
            let right_seat_index = seat_index + 1_u8;
            self.shift_seats_right_for_split(hand_id, right_seat_index, hand.seat_count);

            let mut left_seat = self.new_seat(seat.wager);
            left_seat = self.store_player_card(hand_id, seat_index, left_seat, first_card);
            left_seat = self.store_player_card(hand_id, seat_index, left_seat, left_drawn_card);
            self.record_reveal(hand_id);
            let left_total = blackjack_logic::total_from_parts(
                left_seat.hard_total, left_seat.ace_count,
            );
            if split_aces || left_total == 21_u8 {
                left_seat.status = SeatStatus::Standing;
                left_seat.can_double = false;
                left_seat.can_split = false;
            } else {
                left_seat.can_split = next_split_count < blackjack_logic::MAX_SPLITS_PER_HAND
                    && blackjack_logic::can_split_cards(first_card, left_drawn_card);
            }
            if split_aces {
                left_seat.can_double = false;
            }
            self.seats.write((hand_id, seat_index), left_seat);
            self.split_aces.write((hand_id, seat_index), split_aces);

            let mut right_seat = self.new_seat(seat.wager);
            right_seat = self.store_player_card(hand_id, right_seat_index, right_seat, second_card);
            right_seat = self
                .store_player_card(hand_id, right_seat_index, right_seat, right_drawn_card);
            self.record_reveal(hand_id);
            let right_total = blackjack_logic::total_from_parts(
                right_seat.hard_total, right_seat.ace_count,
            );
            if split_aces || right_total == 21_u8 {
                right_seat.status = SeatStatus::Standing;
                right_seat.can_double = false;
                right_seat.can_split = false;
            } else {
                right_seat.can_split = next_split_count < blackjack_logic::MAX_SPLITS_PER_HAND
                    && blackjack_logic::can_split_cards(second_card, right_drawn_card);
            }
            if split_aces {
                right_seat.can_double = false;
            }
            self.seats.write((hand_id, right_seat_index), right_seat);
            self.split_aces.write((hand_id, right_seat_index), split_aces);

            hand.seat_count += 1_u8;
            hand.split_count = next_split_count;
            if hand.split_count >= blackjack_logic::MAX_SPLITS_PER_HAND
                || hand.seat_count >= blackjack_logic::MAX_SEATS_PER_HAND {
                self.clear_split_permissions(hand_id, hand.seat_count);
            }
            hand.action_count += 1_u8;
            hand = self.advance_hand(hand);
            self.hands.write(hand_id, hand);

            self
                .emit(
                    PlayerActionRecorded {
                        hand_id,
                        player,
                        seat_index,
                        action: PlayerAction::Split,
                        new_total: left_total,
                    },
                );
        }

        fn reveal_dealer_card_internal(ref self: ContractState, hand_id: u64, drawn_card: u8) {
            let mut hand = self.load_hand(hand_id);
            assert(hand.status == HandStatus::AwaitingDealer, 'HAND_NOT_DEALER_PHASE');
            if hand.dealer_card_count >= 2_u8 {
                assert(
                    !blackjack_logic::dealer_should_stand(
                        hand.dealer_hard_total, hand.dealer_ace_count,
                    ),
                    'DEALER_ALREADY_STOOD',
                );
            }
            let card_index = hand.dealer_card_count;
            hand = self.store_dealer_card(hand_id, hand, drawn_card);
            self.record_reveal(hand_id);
            let dealer_total = blackjack_logic::total_from_parts(
                hand.dealer_hard_total, hand.dealer_ace_count,
            );
            self.hands.write(hand_id, hand);
            self.emit(DealerCardRevealed { hand_id, card_index, dealer_total });
        }

        fn finalize_hand_internal(ref self: ContractState, hand_id: u64) {
            let mut hand = self.load_hand(hand_id);
            assert(hand.status == HandStatus::AwaitingDealer, 'HAND_NOT_FINALIZABLE');
            assert(hand.dealer_card_count >= 2_u8, 'DEALER_HOLE_CARD_MISSING');

            let dealer_total = blackjack_logic::total_from_parts(
                hand.dealer_hard_total, hand.dealer_ace_count,
            );
            let dealer_busted = dealer_total > 21_u8;
            assert(
                dealer_busted
                    || blackjack_logic::dealer_should_stand(
                        hand.dealer_hard_total, hand.dealer_ace_count,
                    ),
                'DEALER_NOT_STOOD',
            );

            let dealer_blackjack = hand.dealer_card_count == 2_u8 && dealer_total == 21_u8;
            let split_depth = if hand.split_count > 0_u8 {
                1_u8
            } else {
                0_u8
            };

            let mut seat0 = self.seats.read((hand_id, 0_u8));
            let (seat0_outcome, seat0_payout) = blackjack_logic::settle_seat(
                seat0, split_depth, dealer_total, dealer_blackjack, dealer_busted,
            );
            seat0.status = SeatStatus::Settled;
            seat0.outcome = seat0_outcome;
            seat0.payout = seat0_payout;
            self.seats.write((hand_id, 0_u8), seat0);

            let mut total_payout = seat0_payout;
            let mut seat_index = 1_u8;
            loop {
                if seat_index >= hand.seat_count {
                    break;
                }
                let mut split_seat = self.seats.read((hand_id, seat_index));
                let (seat_outcome, seat_payout) = blackjack_logic::settle_seat(
                    split_seat, split_depth, dealer_total, dealer_blackjack, dealer_busted,
                );
                split_seat.status = SeatStatus::Settled;
                split_seat.outcome = seat_outcome;
                split_seat.payout = seat_payout;
                self.seats.write((hand_id, seat_index), split_seat);
                total_payout += seat_payout;
                seat_index += 1_u8;
            }

            let insurance_wager = self.insurance_wagers.read(hand_id);
            if dealer_blackjack && insurance_wager > 0_u128 {
                total_payout += insurance_wager * 3_u128;
            }

            hand.status = HandStatus::Settled;
            hand.dealer_final_total = dealer_total;
            self.hands.write(hand_id, hand);
            self.insurance_wagers.write(hand_id, 0_u128);
            self.insurance_decided.write(hand_id, false);

            IBankrollVaultDispatcher { contract_address: self.vault.read() }
                .settle_hand(hand.player, hand_id, total_payout);
            IDeckCommitmentDispatcher { contract_address: self.deck_commitment.read() }
                .close_transcript(hand_id);
            self
                .emit(
                    HandResolved {
                        hand_id,
                        player: hand.player,
                        dealer_total,
                        total_payout,
                        primary_outcome: seat0_outcome,
                    },
                );
        }

        fn assert_owner(self: @ContractState) {
            assert(get_caller_address() == self.owner.read(), 'OWNER_ONLY');
        }

        fn assert_legacy_card_entrypoint_allowed(self: @ContractState) {
            let _ = self;
            assert(false, 'CARD_PROOF_REQUIRED');
        }

        fn assert_ordered_card_reveal(
            ref self: ContractState,
            hand_id: u64,
            table_id: u64,
            player: ContractAddress,
            transcript_root: u256,
            rank: u8,
            proof: BlackjackCardRevealProof,
            expected_deck_index: u64,
        ) {
            blackjack_logic::assert_card_rank(rank);
            assert(proof.deck_index == expected_deck_index, 'CARD_ORDER_INVALID');
            assert(proof.deck_index < CARD_TREE_SIZE, 'CARD_INDEX_RANGE');
            let committed_rank = blackjack_logic::card_rank_from_id(proof.card_id);
            assert(committed_rank == rank, 'CARD_ID_MISMATCH');
            assert(!self.u256_is_zero(proof.salt), 'CARD_SALT_REQUIRED');
            assert(!self.revealed_deck_indices.read((hand_id, proof.deck_index)), 'CARD_REPLAY');
            let root = self.card_reveal_root(table_id, player, rank, proof);
            assert(root == transcript_root, 'CARD_PROOF_INVALID');
            self.revealed_deck_indices.write((hand_id, proof.deck_index), true);
        }

        fn card_reveal_root(
            self: @ContractState,
            table_id: u64,
            player: ContractAddress,
            rank: u8,
            proof: BlackjackCardRevealProof,
        ) -> u256 {
            let mut hash = self.card_leaf_hash(table_id, player, rank, proof);
            let mut index = proof.deck_index;
            let mut level = 0_u8;
            loop {
                if level >= CARD_TREE_DEPTH {
                    break;
                }
                let sibling = self.proof_sibling(proof, level);
                if index % 2_u64 == 0_u64 {
                    hash = self.poseidon_pair(hash, sibling);
                } else {
                    hash = self.poseidon_pair(sibling, hash);
                }
                index = index / 2_u64;
                level += 1_u8;
            }
            hash
        }

        fn card_leaf_hash(
            self: @ContractState,
            table_id: u64,
            player: ContractAddress,
            rank: u8,
            proof: BlackjackCardRevealProof,
        ) -> u256 {
            let _ = self;
            let _ = table_id;
            let _ = player;
            let _ = rank;
            poseidon_hash_2(self.u256_from_u16(proof.card_id), proof.salt)
        }

        fn proof_sibling(self: @ContractState, proof: BlackjackCardRevealProof, level: u8) -> u256 {
            let _ = self;
            match level {
                0_u8 => proof.sibling_0,
                1_u8 => proof.sibling_1,
                2_u8 => proof.sibling_2,
                3_u8 => proof.sibling_3,
                4_u8 => proof.sibling_4,
                5_u8 => proof.sibling_5,
                6_u8 => proof.sibling_6,
                7_u8 => proof.sibling_7,
                8_u8 => proof.sibling_8,
                _ => self.u256_zero(),
            }
        }

        fn load_hand(self: @ContractState, hand_id: u64) -> BlackjackHand {
            let hand = self.hands.read(hand_id);
            assert(hand.hand_id == hand_id, 'HAND_NOT_FOUND');
            hand
        }

        fn assert_player_access(self: @ContractState, player: ContractAddress, wager: u128) {
            let caller = get_caller_address();
            if caller != player {
                let allowed = ISessionRegistryDispatcher {
                    contract_address: self.session_registry.read(),
                }
                    .is_action_allowed(player, caller, wager, get_block_timestamp());
                assert(allowed, 'SESSION_DENIED');
            }
        }

        fn assert_action_access(self: @ContractState, player: ContractAddress, wager: u128) {
            let caller = get_caller_address();
            if caller == self.owner.read() {
                let allowed = ISessionRegistryDispatcher {
                    contract_address: self.session_registry.read(),
                }
                    .is_action_allowed(player, caller, wager, get_block_timestamp());
                assert(allowed, 'SESSION_DENIED');
                return;
            }
            self.assert_player_access(player, wager);
        }

        fn assert_actionable_seat(
            self: @ContractState, player: ContractAddress, hand_id: u64, seat_index: u8,
        ) -> (BlackjackHand, BlackjackSeat) {
            let hand = self.load_hand(hand_id);
            self.assert_existing_hand_access(player, hand_id);
            assert(hand.player == player, 'HAND_OWNER_MISMATCH');
            assert(hand.status == HandStatus::Active, 'HAND_NOT_ACTIVE');
            assert(seat_index == hand.active_seat, 'SEAT_NOT_ACTIVE');

            let seat = self.seats.read((hand_id, seat_index));
            assert(seat.status == SeatStatus::Active, 'SEAT_NOT_PLAYABLE');
            (hand, seat)
        }

        fn assert_insurance_window(
            self: @ContractState, player: ContractAddress, hand_id: u64,
        ) -> (BlackjackHand, BlackjackSeat) {
            let hand = self.load_hand(hand_id);
            self.assert_existing_hand_access(player, hand_id);
            assert(hand.player == player, 'HAND_OWNER_MISMATCH');
            assert(hand.status == HandStatus::AwaitingInsurance, 'INSURANCE_WINDOW_CLOSED');
            assert(hand.dealer_upcard == 1_u8, 'INSURANCE_NOT_OFFERED');
            assert(!self.insurance_decided.read(hand_id), 'INSURANCE_ALREADY_DECIDED');

            let seat = self.seats.read((hand_id, 0_u8));
            assert(
                seat.status == SeatStatus::Active || seat.status == SeatStatus::Blackjack,
                'SEAT_NOT_INSURABLE',
            );
            (hand, seat)
        }

        fn new_seat(self: @ContractState, wager: u128) -> BlackjackSeat {
            BlackjackSeat {
                wager,
                status: SeatStatus::Active,
                card_count: 0_u8,
                hard_total: 0_u8,
                ace_count: 0_u8,
                can_double: true,
                can_split: false,
                doubled: false,
                outcome: HandOutcome::Pending,
                payout: 0_u128,
            }
        }

        fn store_player_card(
            ref self: ContractState,
            hand_id: u64,
            seat_index: u8,
            mut seat: BlackjackSeat,
            card: u8,
        ) -> BlackjackSeat {
            blackjack_logic::assert_card_rank(card);
            assert(seat.card_count < blackjack_logic::MAX_CARD_SLOTS, 'SEAT_CARD_LIMIT');
            let card_index = seat.card_count;
            self.player_cards.write((hand_id, seat_index, card_index), card);
            let (hard_total, ace_count, _) = blackjack_logic::add_card(
                seat.hard_total, seat.ace_count, card,
            );
            seat.card_count = card_index + 1_u8;
            seat.hard_total = hard_total;
            seat.ace_count = ace_count;
            seat
        }

        fn store_dealer_card(
            ref self: ContractState, hand_id: u64, mut hand: BlackjackHand, card: u8,
        ) -> BlackjackHand {
            blackjack_logic::assert_card_rank(card);
            assert(hand.dealer_card_count < blackjack_logic::MAX_CARD_SLOTS, 'DEALER_CARD_LIMIT');
            let card_index = hand.dealer_card_count;
            self.dealer_cards.write((hand_id, card_index), card);
            let (hard_total, ace_count, _) = blackjack_logic::add_card(
                hand.dealer_hard_total, hand.dealer_ace_count, card,
            );
            hand.dealer_card_count = card_index + 1_u8;
            hand.dealer_hard_total = hard_total;
            hand.dealer_ace_count = ace_count;
            hand
        }

        fn record_reveal(self: @ContractState, hand_id: u64) {
            IDeckCommitmentDispatcher { contract_address: self.deck_commitment.read() }
                .record_reveal(hand_id);
        }

        fn record_transition(self: @ContractState, hand_id: u64) {
            IDeckCommitmentDispatcher { contract_address: self.deck_commitment.read() }
                .record_transition(hand_id);
        }

        fn assert_dealer_peek_proof(
            ref self: ContractState,
            hand_id: u64,
            table_id: u64,
            player: ContractAddress,
            wager: u128,
            transcript_root: u256,
            dealer_upcard: u8,
            player_first_card: u8,
            player_second_card: u8,
            dealer_peek_proof: Span<felt252>,
        ) -> bool {
            assert(dealer_peek_proof.len() > 0, 'PEEK_PROOF_REQUIRED');
            let expected_hand_hash = self
                .dealer_peek_hand_hash(
                    hand_id,
                    table_id,
                    wager,
                    transcript_root,
                    dealer_upcard,
                    player_first_card,
                    player_second_card,
                );
            assert(!self.used_peek_hand_hashes.read(expected_hand_hash), 'PEEK_PROOF_REPLAY');
            let verifier = IDealerPeekGroth16VerifierDispatcher {
                contract_address: self.dealer_peek_verifier.read(),
            };
            let verify_result = verifier.verify_groth16_proof_bn254(dealer_peek_proof);
            let public_inputs = match verify_result {
                Result::Ok(values) => values,
                Result::Err(_) => panic_with_felt252('PEEK_PROOF_INVALID'),
            };
            assert(public_inputs.len() == 11, 'PEEK_PI_LEN');
            assert(*public_inputs.at(0) == expected_hand_hash, 'PEEK_HAND_HASH');
            assert(*public_inputs.at(1) == transcript_root, 'PEEK_ROOT');
            assert(*public_inputs.at(2) == self.u256_from_u64(3_u64), 'PEEK_INDEX');
            assert(
                *public_inputs.at(3) == self.u256_from_u8(self.upcard_class(dealer_upcard)),
                'PEEK_UPCARD_CLASS',
            );
            let peek_result = *public_inputs.at(4);
            assert(*public_inputs.at(5) == self.u256_from_u64(hand_id), 'PEEK_CHAIN_HAND_ID');
            assert(*public_inputs.at(6) == self.u256_from_u64(table_id), 'PEEK_TABLE_ID');
            assert(*public_inputs.at(7) == self.u256_from_u128(wager), 'PEEK_WAGER');
            assert(*public_inputs.at(8) == self.u256_from_u8(dealer_upcard), 'PEEK_UPCARD');
            assert(
                *public_inputs.at(9) == self.u256_from_u8(player_first_card), 'PEEK_PLAYER_FIRST',
            );
            assert(
                *public_inputs.at(10) == self.u256_from_u8(player_second_card),
                'PEEK_PLAYER_SECOND',
            );
            assert(self.u256_is_bool(peek_result), 'PEEK_RESULT_BOOL');
            self.used_peek_hand_hashes.write(expected_hand_hash, true);
            !self.u256_is_zero(peek_result)
        }

        fn dealer_peek_hand_hash(
            self: @ContractState,
            hand_id: u64,
            table_id: u64,
            wager: u128,
            transcript_root: u256,
            dealer_upcard: u8,
            player_first_card: u8,
            player_second_card: u8,
        ) -> u256 {
            let _ = self;
            let mut hash = poseidon_hash_2(
                self.u256_from_u64(hand_id), self.u256_from_u64(table_id),
            );
            hash = poseidon_hash_2(hash, self.u256_from_u128(wager));
            hash = poseidon_hash_2(hash, transcript_root);
            hash = poseidon_hash_2(hash, self.u256_from_u8(dealer_upcard));
            hash = poseidon_hash_2(hash, self.u256_from_u8(player_first_card));
            hash = poseidon_hash_2(hash, self.u256_from_u8(player_second_card));
            hash
        }

        fn poseidon_pair(self: @ContractState, left: u256, right: u256) -> u256 {
            let _ = self;
            poseidon_hash_2(left, right)
        }

        fn upcard_class(self: @ContractState, dealer_upcard: u8) -> u8 {
            let _ = self;
            if dealer_upcard == 1_u8 {
                1_u8
            } else {
                assert(blackjack_logic::card_value(dealer_upcard) == 10_u8, 'PEEK_UPCARD_RANGE');
                2_u8
            }
        }

        fn u256_zero(self: @ContractState) -> u256 {
            let _ = self;
            u256 { low: 0, high: 0 }
        }

        fn u256_from_u64(self: @ContractState, value: u64) -> u256 {
            let _ = self;
            u256 { low: value.into(), high: 0 }
        }

        fn u256_from_u8(self: @ContractState, value: u8) -> u256 {
            let _ = self;
            u256 { low: value.into(), high: 0 }
        }

        fn u256_from_u16(self: @ContractState, value: u16) -> u256 {
            let _ = self;
            u256 { low: value.into(), high: 0 }
        }

        fn u256_from_u128(self: @ContractState, value: u128) -> u256 {
            let _ = self;
            u256 { low: value, high: 0 }
        }

        fn u256_is_zero(self: @ContractState, value: u256) -> bool {
            let _ = self;
            value.low == 0 && value.high == 0
        }

        fn u256_is_bool(self: @ContractState, value: u256) -> bool {
            let _ = self;
            value.high == 0 && (value.low == 0 || value.low == 1)
        }

        fn finish_insurance_decision(
            self: @ContractState, mut hand: BlackjackHand, dealer_blackjack: bool,
        ) -> BlackjackHand {
            if dealer_blackjack {
                hand.status = HandStatus::AwaitingDealer;
                hand.active_seat = 0_u8;
                return hand;
            }

            let seat = self.seats.read((hand.hand_id, 0_u8));
            hand.active_seat = 0_u8;
            if seat.status == SeatStatus::Blackjack {
                hand.status = HandStatus::AwaitingDealer;
            } else {
                hand.status = HandStatus::Active;
            }
            hand
        }

        fn advance_hand(self: @ContractState, mut hand: BlackjackHand) -> BlackjackHand {
            let mut seat_index = 0_u8;
            loop {
                if seat_index >= hand.seat_count {
                    break;
                }
                let seat = self.seats.read((hand.hand_id, seat_index));
                if seat.status == SeatStatus::Active {
                    hand.active_seat = seat_index;
                    hand.status = HandStatus::Active;
                    return hand;
                }
                seat_index += 1_u8;
            }

            hand.status = HandStatus::AwaitingDealer;
            hand
        }

        fn clear_split_permissions(ref self: ContractState, hand_id: u64, seat_count: u8) {
            let mut seat_index = 0_u8;
            loop {
                if seat_index >= seat_count {
                    break;
                }
                let mut seat = self.seats.read((hand_id, seat_index));
                if seat.can_split {
                    seat.can_split = false;
                    self.seats.write((hand_id, seat_index), seat);
                }
                seat_index += 1_u8;
            }
        }

        fn shift_seats_right_for_split(
            ref self: ContractState, hand_id: u64, insert_index: u8, seat_count: u8,
        ) {
            let mut destination = seat_count;
            loop {
                if destination <= insert_index {
                    break;
                }
                let source = destination - 1_u8;
                let source_seat = self.seats.read((hand_id, source));
                self.seats.write((hand_id, destination), source_seat);
                self
                    .split_aces
                    .write((hand_id, destination), self.split_aces.read((hand_id, source)));
                self.copy_player_cards(hand_id, source, destination, source_seat.card_count);
                destination -= 1_u8;
            }
        }

        fn copy_player_cards(
            ref self: ContractState,
            hand_id: u64,
            source_seat_index: u8,
            destination_seat_index: u8,
            card_count: u8,
        ) {
            let mut card_index = 0_u8;
            loop {
                if card_index >= card_count {
                    break;
                }
                self
                    .player_cards
                    .write(
                        (hand_id, destination_seat_index, card_index),
                        self.player_cards.read((hand_id, source_seat_index, card_index)),
                    );
                card_index += 1_u8;
            }
        }

        fn is_ten_value(self: @ContractState, card: u8) -> bool {
            card == 10_u8 || card == 11_u8 || card == 12_u8 || card == 13_u8
        }

        fn blackjack_lock_factor(self: @ContractState, dealer_upcard: u8) -> u128 {
            if dealer_upcard == 1_u8 {
                return BLACKJACK_ACE_UPCARD_LOCK_FACTOR;
            }
            BLACKJACK_BASE_LOCK_FACTOR
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

        fn dealer_blackjack_for_hand(self: @ContractState, hand_id: u64) -> bool {
            IDeckCommitmentDispatcher { contract_address: self.deck_commitment.read() }
                .get_commitment(hand_id)
                .dealer_blackjack
        }

        fn assert_precommitted_hand(
            self: @ContractState,
            hand_id: u64,
            table_id: u64,
            transcript_root: u256,
            dealer_peek_required: bool,
            dealer_blackjack: bool,
        ) {
            let commitment = IDeckCommitmentDispatcher {
                contract_address: self.deck_commitment.read(),
            }
                .get_commitment(hand_id);
            assert(!self.u256_is_zero(commitment.transcript_root), 'DECK_COMMIT_REQUIRED');
            assert(!commitment.closed, 'DECK_COMMIT_CLOSED');
            assert(commitment.reveal_count == 0_u32, 'DECK_COMMIT_STARTED');
            assert(commitment.table_id == table_id, 'DECK_TABLE_MISMATCH');
            assert(commitment.transcript_root == transcript_root, 'DECK_ROOT_MISMATCH');
            assert(
                commitment.timeout_window_blocks == BLACKJACK_TIMEOUT_BLOCKS,
                'DECK_TIMEOUT_MISMATCH',
            );
            assert(commitment.dealer_peek_required == dealer_peek_required, 'DECK_PEEK_MISMATCH');
            assert(commitment.dealer_blackjack == dealer_blackjack, 'DECK_PEEK_MISMATCH');
        }

        fn assert_existing_hand_access(
            self: @ContractState, player: ContractAddress, hand_id: u64,
        ) {
            let reserved = IBankrollVaultDispatcher { contract_address: self.vault.read() }
                .reserved_of(player, hand_id);
            self.assert_action_access(player, reserved);
        }

        fn assert_session_total_wager(
            self: @ContractState, player: ContractAddress, hand_id: u64, additional_wager: u128,
        ) {
            let reserved = IBankrollVaultDispatcher { contract_address: self.vault.read() }
                .reserved_of(player, hand_id);
            self.assert_action_access(player, reserved + additional_wager);
        }

        fn assert_hand_deadline_expired(self: @ContractState, hand_id: u64) {
            let commitment = IDeckCommitmentDispatcher {
                contract_address: self.deck_commitment.read(),
            }
                .get_commitment(hand_id);
            assert(!commitment.closed, 'TRANSCRIPT_CLOSED');
            assert(get_block_number() > commitment.timeout_block, 'HAND_NOT_EXPIRED');
        }
    }
}
