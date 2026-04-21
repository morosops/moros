#[starknet::contract]
pub mod DeckCommitment {
    use starknet::storage::{
        Map, StorageMapReadAccess, StorageMapWriteAccess, StoragePointerReadAccess,
        StoragePointerWriteAccess,
    };
    use starknet::{ContractAddress, get_block_number, get_caller_address};
    use crate::interfaces::IDeckCommitment;
    use crate::types::DeckCommitmentState;

    #[event]
    #[derive(Drop, starknet::Event)]
    pub enum Event {
        OperatorUpdated: OperatorUpdated,
        HandCommitted: HandCommitted,
        RevealRecorded: RevealRecorded,
        TransitionRecorded: TransitionRecorded,
        TranscriptClosed: TranscriptClosed,
    }

    #[derive(Drop, starknet::Event)]
    pub struct OperatorUpdated {
        pub operator: ContractAddress,
        pub active: bool,
    }

    #[derive(Drop, starknet::Event)]
    pub struct HandCommitted {
        pub hand_id: u64,
        pub table_id: u64,
        pub transcript_root: u256,
        pub timeout_block: u64,
    }

    #[derive(Drop, starknet::Event)]
    pub struct RevealRecorded {
        pub hand_id: u64,
        pub reveal_count: u32,
        pub timeout_block: u64,
    }

    #[derive(Drop, starknet::Event)]
    pub struct TransitionRecorded {
        pub hand_id: u64,
        pub timeout_block: u64,
    }

    #[derive(Drop, starknet::Event)]
    pub struct TranscriptClosed {
        pub hand_id: u64,
    }

    #[storage]
    struct Storage {
        owner: ContractAddress,
        operators: Map<ContractAddress, bool>,
        commitments: Map<u64, DeckCommitmentState>,
    }

    #[constructor]
    fn constructor(ref self: ContractState, owner: ContractAddress) {
        self.owner.write(owner);
    }

    #[abi(embed_v0)]
    impl DeckCommitmentImpl of IDeckCommitment<ContractState> {
        fn set_operator(ref self: ContractState, operator: ContractAddress, active: bool) {
            self.assert_owner();
            self.operators.write(operator, active);
            self.emit(OperatorUpdated { operator, active });
        }

        fn post_hand_commitment(
            ref self: ContractState,
            hand_id: u64,
            table_id: u64,
            transcript_root: u256,
            timeout_window_blocks: u64,
            dealer_peek_required: bool,
            dealer_blackjack: bool,
        ) {
            self.assert_operator();
            assert(timeout_window_blocks > 0_u64, 'TIMEOUT_WINDOW_ZERO');
            assert(!self.u256_is_zero(transcript_root), 'ROOT_REQUIRED');
            let existing = self.commitments.read(hand_id);
            assert(self.u256_is_zero(existing.transcript_root), 'COMMITMENT_EXISTS');
            let timeout_block = get_block_number() + timeout_window_blocks;
            let state = DeckCommitmentState {
                table_id,
                transcript_root,
                timeout_window_blocks,
                timeout_block,
                reveal_count: 0,
                dealer_peek_required,
                dealer_blackjack,
                closed: false,
            };
            self.commitments.write(hand_id, state);
            self.emit(HandCommitted { hand_id, table_id, transcript_root, timeout_block });
        }

        fn record_reveal(ref self: ContractState, hand_id: u64) {
            self.assert_operator();
            let mut state = self.commitments.read(hand_id);
            self.assert_committed_state(state);
            assert(!state.closed, 'TRANSCRIPT_CLOSED');
            state.reveal_count += 1;
            state.timeout_block = get_block_number() + state.timeout_window_blocks;
            self.commitments.write(hand_id, state);
            self
                .emit(
                    RevealRecorded {
                        hand_id,
                        reveal_count: state.reveal_count,
                        timeout_block: state.timeout_block,
                    },
                );
        }

        fn record_transition(ref self: ContractState, hand_id: u64) {
            self.assert_operator();
            let mut state = self.commitments.read(hand_id);
            self.assert_committed_state(state);
            assert(!state.closed, 'TRANSCRIPT_CLOSED');
            state.timeout_block = get_block_number() + state.timeout_window_blocks;
            self.commitments.write(hand_id, state);
            self.emit(TransitionRecorded { hand_id, timeout_block: state.timeout_block });
        }

        fn close_transcript(ref self: ContractState, hand_id: u64) {
            self.assert_operator();
            let mut state = self.commitments.read(hand_id);
            self.assert_committed_state(state);
            state.closed = true;
            self.commitments.write(hand_id, state);
            self.emit(TranscriptClosed { hand_id });
        }

        fn get_commitment(self: @ContractState, hand_id: u64) -> DeckCommitmentState {
            self.commitments.read(hand_id)
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

        fn assert_committed_state(self: @ContractState, state: DeckCommitmentState) {
            assert(!self.u256_is_zero(state.transcript_root), 'COMMITMENT_REQUIRED');
        }

        fn u256_is_zero(self: @ContractState, value: u256) -> bool {
            let _ = self;
            value.low == 0 && value.high == 0
        }
    }
}
