#[starknet::contract]
pub mod DealerCommitment {
    use starknet::storage::{
        Map, StorageMapReadAccess, StorageMapWriteAccess, StoragePointerReadAccess,
        StoragePointerWriteAccess,
    };
    use starknet::{ContractAddress, get_caller_address};
    use crate::interfaces::IDealerCommitment;
    use crate::types::DealerCommitmentState;

    #[event]
    #[derive(Drop, starknet::Event)]
    pub enum Event {
        OperatorUpdated: OperatorUpdated,
        HandCommitted: HandCommitted,
        RevealRecorded: RevealRecorded,
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
        pub transcript_root: felt252,
        pub reveal_deadline: u64,
    }

    #[derive(Drop, starknet::Event)]
    pub struct RevealRecorded {
        pub hand_id: u64,
        pub reveal_count: u32,
    }

    #[derive(Drop, starknet::Event)]
    pub struct TranscriptClosed {
        pub hand_id: u64,
    }

    #[storage]
    struct Storage {
        owner: ContractAddress,
        operators: Map<ContractAddress, bool>,
        commitments: Map<u64, DealerCommitmentState>,
    }

    #[constructor]
    fn constructor(ref self: ContractState, owner: ContractAddress) {
        self.owner.write(owner);
    }

    #[abi(embed_v0)]
    impl DealerCommitmentImpl of IDealerCommitment<ContractState> {
        fn set_operator(ref self: ContractState, operator: ContractAddress, active: bool) {
            self.assert_owner();
            self.operators.write(operator, active);
            self.emit(OperatorUpdated { operator, active });
        }

        fn post_hand_commitment(
            ref self: ContractState,
            hand_id: u64,
            table_id: u64,
            transcript_root: felt252,
            reveal_deadline: u64,
            dealer_peek_required: bool,
            dealer_blackjack: bool,
        ) {
            self.assert_operator();
            let state = DealerCommitmentState {
                table_id,
                transcript_root,
                reveal_deadline,
                reveal_count: 0,
                dealer_peek_required,
                dealer_blackjack,
                closed: false,
            };
            self.commitments.write(hand_id, state);
            self.emit(HandCommitted { hand_id, table_id, transcript_root, reveal_deadline });
        }

        fn record_reveal(ref self: ContractState, hand_id: u64) {
            self.assert_operator();
            let mut state = self.commitments.read(hand_id);
            assert(!state.closed, 'TRANSCRIPT_CLOSED');
            state.reveal_count += 1;
            self.commitments.write(hand_id, state);
            self.emit(RevealRecorded { hand_id, reveal_count: state.reveal_count });
        }

        fn close_transcript(ref self: ContractState, hand_id: u64) {
            self.assert_operator();
            let mut state = self.commitments.read(hand_id);
            state.closed = true;
            self.commitments.write(hand_id, state);
            self.emit(TranscriptClosed { hand_id });
        }

        fn get_commitment(self: @ContractState, hand_id: u64) -> DealerCommitmentState {
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
    }
}
