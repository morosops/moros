use starknet::ContractAddress;

#[allow(starknet::store_no_default_variant)]
#[derive(Copy, Drop, Serde, PartialEq, Debug, starknet::Store)]
pub enum GameKind {
    Blackjack,
    Roulette,
    Baccarat,
    Dice,
}

#[allow(starknet::store_no_default_variant)]
#[derive(Copy, Drop, Serde, PartialEq, Debug, starknet::Store)]
pub enum TableStatus {
    Inactive,
    Active,
    Paused,
}

#[allow(starknet::store_no_default_variant)]
#[derive(Copy, Drop, Serde, PartialEq, Debug, starknet::Store)]
pub enum HandStatus {
    None,
    Active,
    AwaitingDealer,
    Settled,
    Voided,
    AwaitingInsurance,
}

#[allow(starknet::store_no_default_variant)]
#[derive(Copy, Drop, Serde, PartialEq, Debug, starknet::Store)]
pub enum SeatStatus {
    None,
    Active,
    Standing,
    Blackjack,
    Busted,
    Surrendered,
    Settled,
}

#[allow(starknet::store_no_default_variant)]
#[derive(Copy, Drop, Serde, PartialEq, Debug, starknet::Store)]
pub enum HandOutcome {
    Pending,
    Loss,
    Push,
    Win,
    Blackjack,
    Surrender,
}

#[allow(starknet::store_no_default_variant)]
#[derive(Copy, Drop, Serde, PartialEq, Debug, starknet::Store)]
pub enum DiceCommitmentStatus {
    None,
    Available,
    Locked,
    Revealed,
    Voided,
}

#[allow(starknet::store_no_default_variant)]
#[derive(Copy, Drop, Serde, PartialEq, Debug, starknet::Store)]
pub enum PlayerAction {
    Hit,
    Stand,
    Double,
    Split,
    Surrender,
    InsuranceTake,
    InsuranceDecline,
}

#[derive(Copy, Drop, Serde, PartialEq, Debug, starknet::Store)]
pub struct TableConfig {
    pub game_kind: GameKind,
    pub table_contract: ContractAddress,
    pub min_wager: u128,
    pub max_wager: u128,
    pub status: TableStatus,
}

#[derive(Copy, Drop, Serde, PartialEq, Debug, starknet::Store)]
pub struct SessionGrant {
    pub player: ContractAddress,
    pub session_key: ContractAddress,
    pub max_wager: u128,
    pub expires_at: u64,
    pub active: bool,
}

#[derive(Copy, Drop, Serde, PartialEq, Debug, starknet::Store)]
pub struct HouseWithdrawalRequest {
    pub request_id: u64,
    pub recipient: ContractAddress,
    pub amount: u128,
    pub execute_after: u64,
    pub active: bool,
}

#[derive(Copy, Drop, Serde, PartialEq, Debug, starknet::Store)]
pub struct DealerCommitmentState {
    pub table_id: u64,
    pub transcript_root: felt252,
    pub reveal_deadline: u64,
    pub reveal_count: u32,
    pub dealer_peek_required: bool,
    pub dealer_blackjack: bool,
    pub closed: bool,
}

#[derive(Copy, Drop, Serde, PartialEq, Debug, starknet::Store)]
pub struct DeckCommitmentState {
    pub table_id: u64,
    pub transcript_root: u256,
    pub timeout_window_blocks: u64,
    pub timeout_block: u64,
    pub reveal_count: u32,
    pub dealer_peek_required: bool,
    pub dealer_blackjack: bool,
    pub closed: bool,
}

#[derive(Copy, Drop, Serde, PartialEq, Debug, starknet::Store)]
pub struct BlackjackHand {
    pub hand_id: u64,
    pub table_id: u64,
    pub player: ContractAddress,
    pub wager: u128,
    pub status: HandStatus,
    pub transcript_root: u256,
    pub dealer_upcard: u8,
    pub dealer_card_count: u8,
    pub dealer_hard_total: u8,
    pub dealer_ace_count: u8,
    pub dealer_final_total: u8,
    pub action_count: u8,
    pub seat_count: u8,
    pub active_seat: u8,
    pub split_count: u8,
}

#[derive(Copy, Drop, Serde, PartialEq, Debug, starknet::Store)]
pub struct BlackjackSeat {
    pub wager: u128,
    pub status: SeatStatus,
    pub card_count: u8,
    pub hard_total: u8,
    pub ace_count: u8,
    pub can_double: bool,
    pub can_split: bool,
    pub doubled: bool,
    pub outcome: HandOutcome,
    pub payout: u128,
}

#[derive(Copy, Drop, Serde, PartialEq, Debug, starknet::Store)]
pub struct BlackjackCardRevealProof {
    pub deck_index: u64,
    pub card_id: u16,
    pub salt: u256,
    pub sibling_0: u256,
    pub sibling_1: u256,
    pub sibling_2: u256,
    pub sibling_3: u256,
    pub sibling_4: u256,
    pub sibling_5: u256,
    pub sibling_6: u256,
    pub sibling_7: u256,
    pub sibling_8: u256,
}

#[derive(Copy, Drop, Serde, PartialEq, Debug, starknet::Store)]
pub struct DiceRound {
    pub round_id: u64,
    pub table_id: u64,
    pub player: ContractAddress,
    pub wager: u128,
    pub status: HandStatus,
    pub transcript_root: felt252,
    pub commitment_id: u64,
    pub server_seed_hash: felt252,
    pub client_seed: felt252,
    pub target_bps: u32,
    pub roll_over: bool,
    pub roll_bps: u32,
    pub chance_bps: u32,
    pub multiplier_bps: u32,
    pub payout: u128,
    pub win: bool,
}

#[derive(Copy, Drop, Serde, PartialEq, Debug, starknet::Store)]
pub struct DiceSeedCommitment {
    pub commitment_id: u64,
    pub server_seed_hash: felt252,
    pub reveal_deadline: u64,
    pub status: DiceCommitmentStatus,
    pub round_id: u64,
}

#[derive(Copy, Drop, Serde, PartialEq, Debug, starknet::Store)]
pub struct RouletteBet {
    pub kind: u8,
    pub selection: u8,
    pub amount: u128,
    pub payout_multiplier: u128,
    pub payout: u128,
    pub win: bool,
}

#[derive(Copy, Drop, Serde, PartialEq, Debug, starknet::Store)]
pub struct RouletteSpin {
    pub spin_id: u64,
    pub table_id: u64,
    pub player: ContractAddress,
    pub wager: u128,
    pub status: HandStatus,
    pub transcript_root: felt252,
    pub commitment_id: u64,
    pub server_seed_hash: felt252,
    pub client_seed: felt252,
    pub result_number: u8,
    pub bet_count: u8,
    pub payout: u128,
}

#[derive(Copy, Drop, Serde, PartialEq, Debug, starknet::Store)]
pub struct BaccaratRound {
    pub round_id: u64,
    pub table_id: u64,
    pub player: ContractAddress,
    pub wager: u128,
    pub status: HandStatus,
    pub transcript_root: felt252,
    pub commitment_id: u64,
    pub server_seed_hash: felt252,
    pub client_seed: felt252,
    pub bet_side: u8,
    pub player_total: u8,
    pub banker_total: u8,
    pub player_card_count: u8,
    pub banker_card_count: u8,
    pub winner: u8,
    pub payout: u128,
}
