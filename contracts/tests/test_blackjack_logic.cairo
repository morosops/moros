use moros_contracts::games::blackjack_logic;
use moros_contracts::types::{BlackjackSeat, HandOutcome, SeatStatus};

fn active_seat(wager: u128, card_count: u8, hard_total: u8, ace_count: u8) -> BlackjackSeat {
    BlackjackSeat {
        wager,
        status: SeatStatus::Active,
        card_count,
        hard_total,
        ace_count,
        can_double: true,
        can_split: false,
        doubled: false,
        outcome: HandOutcome::Pending,
        payout: 0_u128,
    }
}

#[test]
fn test_ace_reprices_from_soft_to_hard_when_hit_would_bust() {
    let (hard_total, ace_count, total) = blackjack_logic::add_card(0_u8, 0_u8, 1_u8);
    assert(hard_total == 1_u8, 'ace hard total');
    assert(ace_count == 1_u8, 'ace count');
    assert(total == 11_u8, 'single ace should play as 11');
    assert(blackjack_logic::is_soft_total(hard_total, ace_count), 'single ace should be soft');

    let (hard_total, ace_count, total) = blackjack_logic::add_card(hard_total, ace_count, 9_u8);
    assert(hard_total == 10_u8, 'ace nine hard total');
    assert(total == 20_u8, 'ace nine total');
    assert(blackjack_logic::is_soft_total(hard_total, ace_count), 'ace nine should be soft');

    let (hard_total, ace_count, total) = blackjack_logic::add_card(hard_total, ace_count, 5_u8);
    assert(hard_total == 15_u8, 'soft hand hard total');
    assert(ace_count == 1_u8, 'soft hand ace count');
    assert(total == 15_u8, 'ACE_REPRICE');
    assert(!blackjack_logic::is_soft_total(hard_total, ace_count), 'soft hand repriced to hard');
}

#[test]
fn test_face_cards_are_tens_and_can_split_on_value() {
    assert(blackjack_logic::card_value(10_u8) == 10_u8, 'ten value');
    assert(blackjack_logic::card_value(12_u8) == 10_u8, 'queen value');
    assert(blackjack_logic::card_value(13_u8) == 10_u8, 'king value');
    assert(blackjack_logic::can_split_cards(12_u8, 13_u8), 'queen king can split');
    assert(!blackjack_logic::can_split_cards(9_u8, 10_u8), 'nine ten cannot split');
}

#[test]
fn test_blackjack_only_exists_for_unsplit_two_card_twenty_one() {
    assert(blackjack_logic::is_blackjack(2_u8, 11_u8, 1_u8, 0_u8), 'natural blackjack');
    assert(!blackjack_logic::is_blackjack(2_u8, 11_u8, 1_u8, 1_u8), 'split 21 is not blackjack');
    assert(
        !blackjack_logic::is_blackjack(3_u8, 11_u8, 1_u8, 0_u8), 'three card 21 is not blackjack',
    );
}

#[test]
fn test_dealer_stands_on_soft_seventeen() {
    assert(blackjack_logic::dealer_should_stand(7_u8, 1_u8), 'dealer stands on soft 17');
    assert(!blackjack_logic::dealer_should_stand(6_u8, 1_u8), 'dealer hits soft 16');
    assert(blackjack_logic::dealer_should_stand(17_u8, 0_u8), 'dealer stands on hard 17');
}

#[test]
fn test_payout_schedule_matches_table_rules() {
    let wager = 100_u128;
    assert(blackjack_logic::payout_for_outcome(HandOutcome::Loss, wager) == 0_u128, 'loss');
    assert(blackjack_logic::payout_for_outcome(HandOutcome::Push, wager) == 100_u128, 'push');
    assert(blackjack_logic::payout_for_outcome(HandOutcome::Win, wager) == 200_u128, 'win');
    assert(
        blackjack_logic::payout_for_outcome(HandOutcome::Blackjack, wager) == 250_u128, 'blackjack',
    );
}

#[test]
fn test_settle_seat_handles_pushes_losses_and_busts() {
    let natural = active_seat(100_u128, 2_u8, 11_u8, 1_u8);
    let (outcome, payout) = blackjack_logic::settle_seat(natural, 0_u8, 21_u8, true, false);
    assert(outcome == HandOutcome::Push, 'natural pushes dealer blackjack');
    assert(payout == 100_u128, 'natural push payout');

    let three_card_twenty_one = active_seat(100_u128, 3_u8, 21_u8, 0_u8);
    let (outcome, payout) = blackjack_logic::settle_seat(
        three_card_twenty_one, 0_u8, 21_u8, true, false,
    );
    assert(outcome == HandOutcome::Loss, 'DEALER_BJ_WINS');
    assert(payout == 0_u128, 'dealer blackjack loss payout');

    let busted = BlackjackSeat { status: SeatStatus::Busted, ..three_card_twenty_one };
    let (outcome, payout) = blackjack_logic::settle_seat(busted, 0_u8, 18_u8, false, false);
    assert(outcome == HandOutcome::Loss, 'busted seat loses');
    assert(payout == 0_u128, 'busted payout');
}

#[test]
fn test_settle_seat_treats_split_twenty_one_as_standard_win() {
    let split_twenty_one = active_seat(100_u128, 2_u8, 11_u8, 1_u8);
    let (outcome, payout) = blackjack_logic::settle_seat(
        split_twenty_one, 1_u8, 20_u8, false, false,
    );
    assert(outcome == HandOutcome::Win, 'split 21 is a normal win');
    assert(payout == 200_u128, 'split 21 payout');
}

#[test]
fn test_settle_seat_pays_natural_against_dealer_bust() {
    let player_natural = active_seat(100_u128, 2_u8, 11_u8, 1_u8);
    let (outcome, payout) = blackjack_logic::settle_seat(player_natural, 0_u8, 24_u8, false, true);
    assert(outcome == HandOutcome::Blackjack, 'natural vs busted dealer');
    assert(payout == 250_u128, 'NATURAL_PAYOUT');
}
