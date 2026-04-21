use starknet::ContractAddress;
use crate::types::{
    BaccaratRound, BlackjackCardRevealProof, BlackjackHand, BlackjackSeat, DealerCommitmentState,
    DeckCommitmentState, DiceRound, DiceSeedCommitment, GameKind, HouseWithdrawalRequest,
    RouletteBet, RouletteSpin, SessionGrant, TableConfig, TableStatus,
};

#[starknet::interface]
pub trait IERC20<TContractState> {
    fn name(self: @TContractState) -> felt252;
    fn symbol(self: @TContractState) -> felt252;
    fn decimals(self: @TContractState) -> u8;
    fn total_supply(self: @TContractState) -> u256;
    fn balance_of(self: @TContractState, owner: ContractAddress) -> u256;
    fn allowance(self: @TContractState, owner: ContractAddress, spender: ContractAddress) -> u256;
    fn transfer(ref self: TContractState, recipient: ContractAddress, amount: u256) -> bool;
    fn transfer_from(
        ref self: TContractState, sender: ContractAddress, recipient: ContractAddress, amount: u256,
    ) -> bool;
    fn approve(ref self: TContractState, spender: ContractAddress, amount: u256) -> bool;
}

#[starknet::interface]
pub trait IBankrollVault<TContractState> {
    fn asset(self: @TContractState) -> ContractAddress;
    fn set_operator(ref self: TContractState, operator: ContractAddress, active: bool);
    fn deposit_house_liquidity(ref self: TContractState, amount: u128) -> u128;
    fn fund_rewards_treasury(
        ref self: TContractState, recipient: ContractAddress, amount: u128,
    ) -> u128;
    fn withdraw_house_liquidity(
        ref self: TContractState, recipient: ContractAddress, amount: u128,
    ) -> u128;
    fn cancel_house_withdrawal(ref self: TContractState, request_id: u64) -> u128;
    fn execute_house_withdrawal(ref self: TContractState, request_id: u64) -> u128;
    fn deposit_public(ref self: TContractState, recipient: ContractAddress, amount: u128) -> u128;
    fn deposit_to_vault(ref self: TContractState, recipient: ContractAddress, amount: u128) -> u128;
    fn move_to_vault(ref self: TContractState, amount: u128) -> (u128, u128);
    fn move_to_gambling(ref self: TContractState, amount: u128) -> (u128, u128);
    fn reserve_for_hand(
        ref self: TContractState, player: ContractAddress, hand_id: u64, amount: u128,
    );
    fn lock_house_exposure(ref self: TContractState, hand_id: u64, amount: u128);
    fn settle_hand(
        ref self: TContractState, player: ContractAddress, hand_id: u64, payout: u128,
    ) -> u128;
    fn void_hand(ref self: TContractState, player: ContractAddress, hand_id: u64) -> u128;
    fn withdraw_public(ref self: TContractState, recipient: ContractAddress, amount: u128) -> u128;
    fn withdraw_from_vault(
        ref self: TContractState, recipient: ContractAddress, amount: u128,
    ) -> u128;
    fn operator_withdraw_public(
        ref self: TContractState, player: ContractAddress, recipient: ContractAddress, amount: u128,
    ) -> u128;
    fn balance_of(self: @TContractState, player: ContractAddress) -> u128;
    fn gambling_balance_of(self: @TContractState, player: ContractAddress) -> u128;
    fn vault_balance_of(self: @TContractState, player: ContractAddress) -> u128;
    fn reserved_of(self: @TContractState, player: ContractAddress, hand_id: u64) -> u128;
    fn total_player_liabilities(self: @TContractState) -> u128;
    fn house_available(self: @TContractState) -> u128;
    fn house_locked(self: @TContractState) -> u128;
    fn hand_exposure_of(self: @TContractState, hand_id: u64) -> u128;
    fn house_withdraw_delay_seconds(self: @TContractState) -> u64;
    fn house_withdrawal(self: @TContractState, request_id: u64) -> HouseWithdrawalRequest;
}

#[starknet::interface]
pub trait IRewardsTreasury<TContractState> {
    fn asset(self: @TContractState) -> ContractAddress;
    fn bankroll_vault(self: @TContractState) -> ContractAddress;
    fn set_operator(ref self: TContractState, operator: ContractAddress, active: bool);
    fn set_reward_budget_cap(ref self: TContractState, cap: u128);
    fn set_operator_reward_limit(
        ref self: TContractState, operator: ContractAddress, limit: u128,
    );
    fn fund(ref self: TContractState, amount: u128) -> u128;
    fn credit_to_vault(ref self: TContractState, player: ContractAddress, amount: u128) -> u128;
    fn available_rewards(self: @TContractState) -> u128;
    fn total_funded(self: @TContractState) -> u128;
    fn total_claimed(self: @TContractState) -> u128;
    fn reward_budget_cap(self: @TContractState) -> u128;
    fn operator_reward_limit(self: @TContractState, operator: ContractAddress) -> u128;
    fn operator_claimed(self: @TContractState, operator: ContractAddress) -> u128;
}

#[starknet::interface]
pub trait ITableRegistry<TContractState> {
    fn register_table(
        ref self: TContractState,
        table_id: u64,
        table_contract: ContractAddress,
        game_kind: GameKind,
        min_wager: u128,
        max_wager: u128,
    );
    fn set_table_status(ref self: TContractState, table_id: u64, status: TableStatus);
    fn set_table_limits(ref self: TContractState, table_id: u64, min_wager: u128, max_wager: u128);
    fn get_table(self: @TContractState, table_id: u64) -> TableConfig;
}

#[starknet::interface]
pub trait ISessionRegistry<TContractState> {
    fn register_session_key(
        ref self: TContractState,
        player: ContractAddress,
        session_key: ContractAddress,
        max_wager: u128,
        expires_at: u64,
    );
    fn revoke_session_key(
        ref self: TContractState, player: ContractAddress, session_key: ContractAddress,
    );
    fn get_session(
        self: @TContractState, player: ContractAddress, session_key: ContractAddress,
    ) -> SessionGrant;
    fn is_action_allowed(
        self: @TContractState,
        player: ContractAddress,
        session_key: ContractAddress,
        wager: u128,
        now_ts: u64,
    ) -> bool;
}

#[starknet::interface]
pub trait IDealerCommitment<TContractState> {
    fn set_operator(ref self: TContractState, operator: ContractAddress, active: bool);
    fn post_hand_commitment(
        ref self: TContractState,
        hand_id: u64,
        table_id: u64,
        transcript_root: felt252,
        reveal_deadline: u64,
        dealer_peek_required: bool,
        dealer_blackjack: bool,
    );
    fn record_reveal(ref self: TContractState, hand_id: u64);
    fn close_transcript(ref self: TContractState, hand_id: u64);
    fn get_commitment(self: @TContractState, hand_id: u64) -> DealerCommitmentState;
}

#[starknet::interface]
pub trait IDeckCommitment<TContractState> {
    fn set_operator(ref self: TContractState, operator: ContractAddress, active: bool);
    fn post_hand_commitment(
        ref self: TContractState,
        hand_id: u64,
        table_id: u64,
        transcript_root: u256,
        timeout_window_blocks: u64,
        dealer_peek_required: bool,
        dealer_blackjack: bool,
    );
    fn record_reveal(ref self: TContractState, hand_id: u64);
    fn record_transition(ref self: TContractState, hand_id: u64);
    fn close_transcript(ref self: TContractState, hand_id: u64);
    fn get_commitment(self: @TContractState, hand_id: u64) -> DeckCommitmentState;
}

#[starknet::interface]
pub trait IDealerPeekGroth16Verifier<TContractState> {
    fn verify_groth16_proof_bn254(
        self: @TContractState, full_proof_with_hints: Span<felt252>,
    ) -> Result<Span<u256>, felt252>;
}

#[starknet::interface]
pub trait IMockDealerPeekGroth16Verifier<TContractState> {
    fn set_valid(ref self: TContractState, valid: bool);
    fn set_public_inputs(
        ref self: TContractState,
        public_0: u256,
        public_1: u256,
        public_2: u256,
        public_3: u256,
        public_4: u256,
        public_5: u256,
        public_6: u256,
        public_7: u256,
        public_8: u256,
        public_9: u256,
        public_10: u256,
    );
}

#[starknet::interface]
pub trait IBlackjackTable<TContractState> {
    fn peek_next_hand_id(self: @TContractState) -> u64;
    fn set_wager_cap(ref self: TContractState, max_wager: u128);
    fn get_wager_cap(self: @TContractState) -> u128;
    fn open_hand_verified(
        ref self: TContractState,
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
    ) -> u64;
    fn submit_hit(
        ref self: TContractState,
        player: ContractAddress,
        hand_id: u64,
        seat_index: u8,
        drawn_card: u8,
    );
    fn submit_hit_verified(
        ref self: TContractState,
        player: ContractAddress,
        hand_id: u64,
        seat_index: u8,
        drawn_card: u8,
        drawn_card_proof: BlackjackCardRevealProof,
    );
    fn submit_stand(
        ref self: TContractState, player: ContractAddress, hand_id: u64, seat_index: u8,
    );
    fn submit_double(
        ref self: TContractState,
        player: ContractAddress,
        hand_id: u64,
        seat_index: u8,
        drawn_card: u8,
    );
    fn submit_double_verified(
        ref self: TContractState,
        player: ContractAddress,
        hand_id: u64,
        seat_index: u8,
        drawn_card: u8,
        drawn_card_proof: BlackjackCardRevealProof,
    );
    fn submit_split(
        ref self: TContractState,
        player: ContractAddress,
        hand_id: u64,
        seat_index: u8,
        left_drawn_card: u8,
        right_drawn_card: u8,
    );
    fn submit_split_verified(
        ref self: TContractState,
        player: ContractAddress,
        hand_id: u64,
        seat_index: u8,
        left_drawn_card: u8,
        left_drawn_card_proof: BlackjackCardRevealProof,
        right_drawn_card: u8,
        right_drawn_card_proof: BlackjackCardRevealProof,
    );
    fn submit_take_insurance(
        ref self: TContractState, player: ContractAddress, hand_id: u64, dealer_blackjack: bool,
    );
    fn submit_decline_insurance(
        ref self: TContractState, player: ContractAddress, hand_id: u64, dealer_blackjack: bool,
    );
    fn submit_surrender(
        ref self: TContractState, player: ContractAddress, hand_id: u64, seat_index: u8,
    );
    fn force_expired_insurance_decline(ref self: TContractState, hand_id: u64);
    fn force_expired_stand(ref self: TContractState, hand_id: u64);
    fn reveal_dealer_card(ref self: TContractState, hand_id: u64, drawn_card: u8);
    fn reveal_dealer_card_verified(
        ref self: TContractState,
        hand_id: u64,
        drawn_card: u8,
        drawn_card_proof: BlackjackCardRevealProof,
    );
    fn finalize_hand(ref self: TContractState, hand_id: u64);
    fn void_expired_hand(ref self: TContractState, hand_id: u64);
    fn get_hand(self: @TContractState, hand_id: u64) -> BlackjackHand;
    fn get_insurance_wager(self: @TContractState, hand_id: u64) -> u128;
    fn get_seat(self: @TContractState, hand_id: u64, seat_index: u8) -> BlackjackSeat;
    fn get_player_card(self: @TContractState, hand_id: u64, seat_index: u8, card_index: u8) -> u8;
    fn get_dealer_card(self: @TContractState, hand_id: u64, card_index: u8) -> u8;
}

#[starknet::interface]
pub trait IDiceTable<TContractState> {
    fn peek_next_round_id(self: @TContractState) -> u64;
    fn peek_next_commitment_id(self: @TContractState) -> u64;
    fn set_operator(ref self: TContractState, operator: ContractAddress, active: bool);
    fn set_wager_cap(ref self: TContractState, max_wager: u128);
    fn get_wager_cap(self: @TContractState) -> u128;
    fn set_risk_config(
        ref self: TContractState,
        min_chance_bps: u32,
        max_chance_bps: u32,
        house_edge_bps: u32,
        max_payout: u128,
    );
    fn commit_server_seed(
        ref self: TContractState, server_seed_hash: felt252, reveal_deadline: u64,
    ) -> u64;
    fn open_round(
        ref self: TContractState,
        table_id: u64,
        player: ContractAddress,
        session_key: ContractAddress,
        wager: u128,
        target_bps: u32,
        roll_over: bool,
        client_seed: felt252,
        commitment_id: u64,
    ) -> u64;
    fn settle_round(ref self: TContractState, round_id: u64, server_seed: felt252);
    fn void_expired_round(ref self: TContractState, round_id: u64);
    fn get_round(self: @TContractState, round_id: u64) -> DiceRound;
    fn get_round_for_commitment(self: @TContractState, commitment_id: u64) -> u64;
    fn get_commitment(self: @TContractState, commitment_id: u64) -> DiceSeedCommitment;
    fn quote_payout(
        self: @TContractState, wager: u128, target_bps: u32, roll_over: bool,
    ) -> (u32, u32, u128, u128);
}

#[starknet::interface]
pub trait IRouletteTable<TContractState> {
    fn peek_next_spin_id(self: @TContractState) -> u64;
    fn peek_next_commitment_id(self: @TContractState) -> u64;
    fn set_operator(ref self: TContractState, operator: ContractAddress, active: bool);
    fn set_bet_caps(
        ref self: TContractState, straight_cap: u128, dozen_column_cap: u128, even_money_cap: u128,
    );
    fn get_bet_caps(self: @TContractState) -> (u128, u128, u128);
    fn set_risk_config(ref self: TContractState, house_edge_bps: u32, max_payout: u128);
    fn commit_server_seed(
        ref self: TContractState, server_seed_hash: felt252, reveal_deadline: u64,
    ) -> u64;
    fn open_spin(
        ref self: TContractState,
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
    ) -> u64;
    fn settle_spin(ref self: TContractState, spin_id: u64, server_seed: felt252);
    fn void_expired_spin(ref self: TContractState, spin_id: u64);
    fn get_spin(self: @TContractState, spin_id: u64) -> RouletteSpin;
    fn get_bet(self: @TContractState, spin_id: u64, bet_index: u8) -> RouletteBet;
    fn get_spin_for_commitment(self: @TContractState, commitment_id: u64) -> u64;
    fn get_commitment(self: @TContractState, commitment_id: u64) -> DiceSeedCommitment;
}

#[starknet::interface]
pub trait IBaccaratTable<TContractState> {
    fn peek_next_round_id(self: @TContractState) -> u64;
    fn peek_next_commitment_id(self: @TContractState) -> u64;
    fn set_operator(ref self: TContractState, operator: ContractAddress, active: bool);
    fn set_wager_cap(ref self: TContractState, max_wager: u128);
    fn get_wager_cap(self: @TContractState) -> u128;
    fn set_risk_config(ref self: TContractState, max_payout: u128);
    fn commit_server_seed(
        ref self: TContractState, server_seed_hash: felt252, reveal_deadline: u64,
    ) -> u64;
    fn open_round(
        ref self: TContractState,
        table_id: u64,
        player: ContractAddress,
        session_key: ContractAddress,
        wager: u128,
        bet_side: u8,
        client_seed: felt252,
        commitment_id: u64,
    ) -> u64;
    fn settle_round(ref self: TContractState, round_id: u64, server_seed: felt252);
    fn void_expired_round(ref self: TContractState, round_id: u64);
    fn get_round(self: @TContractState, round_id: u64) -> BaccaratRound;
    fn get_card(self: @TContractState, round_id: u64, hand_index: u8, card_index: u8) -> u8;
    fn get_card_position(
        self: @TContractState, round_id: u64, hand_index: u8, card_index: u8,
    ) -> u16;
    fn get_card_draw_index(
        self: @TContractState, round_id: u64, hand_index: u8, card_index: u8,
    ) -> u8;
    fn get_card_attempt(self: @TContractState, round_id: u64, hand_index: u8, card_index: u8) -> u8;
    fn get_card_commitment(
        self: @TContractState, round_id: u64, hand_index: u8, card_index: u8,
    ) -> felt252;
    fn get_round_for_commitment(self: @TContractState, commitment_id: u64) -> u64;
    fn get_commitment(self: @TContractState, commitment_id: u64) -> DiceSeedCommitment;
}
