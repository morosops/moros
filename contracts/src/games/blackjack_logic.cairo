use crate::types::{BlackjackSeat, HandOutcome, SeatStatus};

pub const MAX_CARD_SLOTS: u8 = 12;
pub const MAX_SEATS_PER_HAND: u8 = 4;
pub const MAX_SPLITS_PER_HAND: u8 = 3;
pub const SHOE_SIZE: u16 = 312_u16;
pub const RANKS_PER_SUIT: u16 = 13_u16;

pub fn assert_card_rank(card: u8) {
    assert(card >= 1_u8, 'CARD_LOW');
    assert(card <= 13_u8, 'CARD_HIGH');
}

pub fn assert_card_id(card_id: u16) {
    assert(card_id < SHOE_SIZE, 'CARD_ID_RANGE');
}

pub fn card_rank_from_id(card_id: u16) -> u8 {
    assert_card_id(card_id);
    let rank_zero_based: u16 = card_id % RANKS_PER_SUIT;
    let rank: u8 = rank_zero_based.try_into().unwrap();
    rank + 1_u8
}

pub fn card_value(card: u8) -> u8 {
    assert_card_rank(card);
    if card == 1_u8 {
        1_u8
    } else if card >= 10_u8 {
        10_u8
    } else {
        card
    }
}

pub fn total_from_parts(hard_total: u8, ace_count: u8) -> u8 {
    if ace_count > 0_u8 && hard_total + 10_u8 <= 21_u8 {
        hard_total + 10_u8
    } else {
        hard_total
    }
}

pub fn is_soft_total(hard_total: u8, ace_count: u8) -> bool {
    ace_count > 0_u8 && hard_total + 10_u8 <= 21_u8
}

pub fn add_card(hard_total: u8, ace_count: u8, card: u8) -> (u8, u8, u8) {
    let next_hard = hard_total + card_value(card);
    let next_aces = if card == 1_u8 {
        ace_count + 1_u8
    } else {
        ace_count
    };
    (next_hard, next_aces, total_from_parts(next_hard, next_aces))
}

pub fn can_split_cards(first: u8, second: u8) -> bool {
    assert_card_rank(first);
    assert_card_rank(second);
    if first == 1_u8 && second == 1_u8 {
        true
    } else {
        card_value(first) == card_value(second)
    }
}

pub fn is_blackjack(card_count: u8, hard_total: u8, ace_count: u8, split_depth: u8) -> bool {
    split_depth == 0_u8 && card_count == 2_u8 && total_from_parts(hard_total, ace_count) == 21_u8
}

pub fn dealer_should_stand(hard_total: u8, ace_count: u8) -> bool {
    total_from_parts(hard_total, ace_count) >= 17_u8
}

pub fn payout_for_outcome(outcome: HandOutcome, wager: u128) -> u128 {
    match outcome {
        HandOutcome::Pending => 0_u128,
        HandOutcome::Loss => 0_u128,
        HandOutcome::Push => wager,
        HandOutcome::Win => wager * 2_u128,
        HandOutcome::Blackjack => (wager * 5_u128) / 2_u128,
        HandOutcome::Surrender => wager / 2_u128,
    }
}

pub fn blackjack_bonus_liability(wager: u128) -> u128 {
    payout_for_outcome(HandOutcome::Blackjack, wager) - wager
}

pub fn settle_seat(
    seat: BlackjackSeat,
    split_depth: u8,
    dealer_total: u8,
    dealer_blackjack: bool,
    dealer_busted: bool,
) -> (HandOutcome, u128) {
    if seat.status == SeatStatus::Surrendered {
        return (HandOutcome::Surrender, payout_for_outcome(HandOutcome::Surrender, seat.wager));
    }
    if seat.status == SeatStatus::Busted {
        return (HandOutcome::Loss, 0_u128);
    }

    let player_total = total_from_parts(seat.hard_total, seat.ace_count);
    let player_blackjack = is_blackjack(
        seat.card_count, seat.hard_total, seat.ace_count, split_depth,
    );

    let outcome = if dealer_busted {
        if player_blackjack {
            HandOutcome::Blackjack
        } else {
            HandOutcome::Win
        }
    } else if dealer_blackjack {
        if player_blackjack {
            HandOutcome::Push
        } else {
            HandOutcome::Loss
        }
    } else if player_total > dealer_total {
        if player_blackjack {
            HandOutcome::Blackjack
        } else {
            HandOutcome::Win
        }
    } else if player_total < dealer_total {
        HandOutcome::Loss
    } else {
        HandOutcome::Push
    };

    (outcome, payout_for_outcome(outcome, seat.wager))
}
