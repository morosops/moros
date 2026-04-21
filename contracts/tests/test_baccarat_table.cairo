use core::hash::HashStateTrait;
use core::poseidon::PoseidonTrait;
use moros_contracts::interfaces::{
    IBaccaratTableDispatcher, IBaccaratTableDispatcherTrait, IBankrollVaultDispatcher,
    IBankrollVaultDispatcherTrait, IDealerCommitmentDispatcher, IDealerCommitmentDispatcherTrait,
    IERC20Dispatcher, IERC20DispatcherTrait, ISessionRegistryDispatcher,
    ISessionRegistryDispatcherTrait, ITableRegistryDispatcher, ITableRegistryDispatcherTrait,
};
use moros_contracts::types::{DiceCommitmentStatus, GameKind, HandStatus};
use snforge_std::{
    ContractClassTrait, DeclareResultTrait, declare, start_cheat_block_number,
    start_cheat_caller_address, stop_cheat_block_number, stop_cheat_caller_address,
};
use starknet::ContractAddress;

const TABLE_ID: u64 = 4;
const ONE_STRK: u128 = 1_000_000_000_000_000_000;
const TABLE_MIN_WAGER: u128 = ONE_STRK;
const TABLE_MAX_WAGER: u128 = 100_u128 * ONE_STRK;
const DEFAULT_WAGER: u128 = 10_u128 * ONE_STRK;
const STARTING_BANKROLL: u128 = 1_000_u128 * ONE_STRK;
const HOUSE_LIQUIDITY: u128 = 10_000_u128 * ONE_STRK;
const INITIAL_SUPPLY: u128 = 100_000_u128 * ONE_STRK;
const MAX_BACCARAT_PAYOUT: u128 = 1_000_u128 * ONE_STRK;
const REVEAL_DEADLINE: u64 = 50_u64;
const SESSION_EXPIRY: u64 = 999_999_u64;
const SERVER_SEED: felt252 = 0x424242;
const CLIENT_SEED: felt252 = 0xF00D;
const BACCARAT_VAULT_ID_OFFSET: u64 = 3_000_000_000_u64;
const BACCARAT_SHOE_CARDS: u32 = 416_u32;
const SERVER_SEED_DOMAIN: felt252 = 'MOROS_SERVER_SEED';
const BACCARAT_SHOE_DOMAIN: felt252 = 'MOROS_BACCARAT_SHOE';
const BACCARAT_CARD_DOMAIN: felt252 = 'MOROS_BAC_CARD';
const BACCARAT_TRANSCRIPT_DOMAIN: felt252 = 'MOROS_BAC_ROOT';

fn hash_server_seed(server_seed: felt252) -> felt252 {
    PoseidonTrait::new().update(SERVER_SEED_DOMAIN).update(server_seed).finalize()
}

#[derive(Copy, Drop)]
struct DrawnBaccaratCard {
    position: u16,
    card: u8,
    draw_index: u8,
    attempt: u8,
    commitment: felt252,
}

fn shoe_position(
    server_seed: felt252, client_seed: felt252, round_id: u64, draw_index: u8, attempt: u8,
) -> u16 {
    let player_felt: felt252 = player().into();
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

fn baccarat_card_from_position(position: u16) -> u8 {
    let zero_based = position % 52_u16;
    let rank_zero_based: u8 = (zero_based % 13_u16).try_into().unwrap();
    rank_zero_based + 1_u8
}

fn position_already_used(
    position: u16, used_count: u8, used_0: u16, used_1: u16, used_2: u16, used_3: u16, used_4: u16,
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

fn baccarat_card_commitment(
    round_id: u64,
    hand_index: u8,
    card_index: u8,
    draw_index: u8,
    attempt: u8,
    position: u16,
    card: u8,
) -> felt252 {
    PoseidonTrait::new()
        .update(BACCARAT_CARD_DOMAIN)
        .update(round_id.into())
        .update(hand_index.into())
        .update(card_index.into())
        .update(draw_index.into())
        .update(attempt.into())
        .update(position.into())
        .update(card.into())
        .finalize()
}

fn draw_unique_card(
    round_id: u64,
    hand_index: u8,
    card_index: u8,
    draw_index: u8,
    used_count: u8,
    used_0: u16,
    used_1: u16,
    used_2: u16,
    used_3: u16,
    used_4: u16,
) -> DrawnBaccaratCard {
    let mut attempt = 0_u8;
    loop {
        let position = shoe_position(SERVER_SEED, CLIENT_SEED, round_id, draw_index, attempt);
        if !position_already_used(position, used_count, used_0, used_1, used_2, used_3, used_4) {
            let card = baccarat_card_from_position(position);
            let commitment = baccarat_card_commitment(
                round_id, hand_index, card_index, draw_index, attempt, position, card,
            );
            return DrawnBaccaratCard { position, card, draw_index, attempt, commitment };
        }
        attempt += 1_u8;
        assert(attempt < 32_u8, 'draw collision');
    }
}

fn baccarat_card_value(card: u8) -> u8 {
    if card == 0_u8 {
        return 0_u8;
    }
    if card >= 10_u8 {
        return 0_u8;
    }
    card
}

fn baccarat_total(card_0: u8, card_1: u8, card_2: u8) -> u8 {
    (baccarat_card_value(card_0)
        + baccarat_card_value(card_1)
        + baccarat_card_value(card_2)) % 10_u8
}

fn banker_draws(total: u8, player_drew: bool, player_third_value: u8) -> bool {
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

fn transcript_root(
    round_id: u64,
    p0_commitment: felt252,
    b0_commitment: felt252,
    p1_commitment: felt252,
    b1_commitment: felt252,
    p2_commitment: felt252,
    b2_commitment: felt252,
) -> felt252 {
    let player_felt: felt252 = player().into();
    PoseidonTrait::new()
        .update(BACCARAT_TRANSCRIPT_DOMAIN)
        .update(hash_server_seed(SERVER_SEED))
        .update(CLIENT_SEED)
        .update(player_felt)
        .update(round_id.into())
        .update(p0_commitment)
        .update(b0_commitment)
        .update(p1_commitment)
        .update(b1_commitment)
        .update(p2_commitment)
        .update(b2_commitment)
        .finalize()
}

fn assert_revealed_card(
    stack: BaccaratStack,
    round_id: u64,
    hand_index: u8,
    card_index: u8,
    expected: DrawnBaccaratCard,
) {
    assert(
        stack.baccarat.get_card(round_id, hand_index, card_index) == expected.card, 'card match',
    );
    assert(
        stack.baccarat.get_card_position(round_id, hand_index, card_index) == expected.position,
        'position match',
    );
    assert(
        stack.baccarat.get_card_draw_index(round_id, hand_index, card_index) == expected.draw_index,
        'draw index match',
    );
    assert(
        stack.baccarat.get_card_attempt(round_id, hand_index, card_index) == expected.attempt,
        'attempt match',
    );
    assert(
        stack.baccarat.get_card_commitment(round_id, hand_index, card_index) == expected.commitment,
        'commitment match',
    );
}

fn vault_round_id(round_id: u64) -> u64 {
    BACCARAT_VAULT_ID_OFFSET + (TABLE_ID * BACCARAT_VAULT_ID_OFFSET) + round_id
}

fn owner() -> ContractAddress {
    0x111.try_into().unwrap()
}

fn player() -> ContractAddress {
    0x222.try_into().unwrap()
}

fn session_key() -> ContractAddress {
    0x333.try_into().unwrap()
}

#[derive(Copy, Drop)]
struct BaccaratStack {
    token: IERC20Dispatcher,
    token_address: ContractAddress,
    vault: IBankrollVaultDispatcher,
    vault_address: ContractAddress,
    table_registry: ITableRegistryDispatcher,
    table_registry_address: ContractAddress,
    sessions: ISessionRegistryDispatcher,
    sessions_address: ContractAddress,
    commitment: IDealerCommitmentDispatcher,
    commitment_address: ContractAddress,
    baccarat: IBaccaratTableDispatcher,
    baccarat_address: ContractAddress,
}

fn deploy_stack() -> BaccaratStack {
    let token_class = declare("MockStrkToken").unwrap().contract_class();
    let (token_address, _) = token_class
        .deploy(@array![owner().into(), INITIAL_SUPPLY.into(), 'TSTRK'.into(), 'STRK'.into()])
        .unwrap();
    let token = IERC20Dispatcher { contract_address: token_address };

    let vault_class = declare("BankrollVault").unwrap().contract_class();
    let (vault_address, _) = vault_class
        .deploy(@array![owner().into(), token_address.into()])
        .unwrap();
    let vault = IBankrollVaultDispatcher { contract_address: vault_address };

    let registry_class = declare("TableRegistry").unwrap().contract_class();
    let (table_registry_address, _) = registry_class.deploy(@array![owner().into()]).unwrap();
    let table_registry = ITableRegistryDispatcher { contract_address: table_registry_address };

    let sessions_class = declare("SessionRegistry").unwrap().contract_class();
    let (sessions_address, _) = sessions_class.deploy(@array![owner().into()]).unwrap();
    let sessions = ISessionRegistryDispatcher { contract_address: sessions_address };

    let commitment_class = declare("DealerCommitment").unwrap().contract_class();
    let (commitment_address, _) = commitment_class.deploy(@array![owner().into()]).unwrap();
    let commitment = IDealerCommitmentDispatcher { contract_address: commitment_address };

    let baccarat_class = declare("BaccaratTable").unwrap().contract_class();
    let (baccarat_address, _) = baccarat_class
        .deploy(
            @array![
                owner().into(), vault_address.into(), table_registry_address.into(),
                sessions_address.into(), commitment_address.into(), MAX_BACCARAT_PAYOUT.into(),
            ],
        )
        .unwrap();
    let baccarat = IBaccaratTableDispatcher { contract_address: baccarat_address };
    start_cheat_block_number(baccarat_address, 1_u64);

    start_cheat_caller_address(vault_address, owner());
    vault.set_operator(baccarat_address, true);
    stop_cheat_caller_address(vault_address);

    start_cheat_caller_address(commitment_address, owner());
    commitment.set_operator(baccarat_address, true);
    stop_cheat_caller_address(commitment_address);

    start_cheat_caller_address(table_registry_address, owner());
    table_registry
        .register_table(
            TABLE_ID, baccarat_address, GameKind::Baccarat, TABLE_MIN_WAGER, TABLE_MAX_WAGER,
        );
    stop_cheat_caller_address(table_registry_address);

    start_cheat_caller_address(sessions_address, player());
    sessions.register_session_key(player(), session_key(), TABLE_MAX_WAGER, SESSION_EXPIRY);
    stop_cheat_caller_address(sessions_address);

    start_cheat_caller_address(token_address, owner());
    token.transfer(player(), STARTING_BANKROLL.into());
    token.approve(vault_address, HOUSE_LIQUIDITY.into());
    stop_cheat_caller_address(token_address);

    start_cheat_caller_address(vault_address, owner());
    vault.deposit_house_liquidity(HOUSE_LIQUIDITY);
    stop_cheat_caller_address(vault_address);

    start_cheat_caller_address(token_address, player());
    token.approve(vault_address, STARTING_BANKROLL.into());
    stop_cheat_caller_address(token_address);

    start_cheat_caller_address(vault_address, player());
    vault.deposit_public(player(), STARTING_BANKROLL);
    stop_cheat_caller_address(vault_address);

    BaccaratStack {
        token,
        token_address,
        vault,
        vault_address,
        table_registry,
        table_registry_address,
        sessions,
        sessions_address,
        commitment,
        commitment_address,
        baccarat,
        baccarat_address,
    }
}

fn commit_seed(stack: BaccaratStack) -> u64 {
    start_cheat_caller_address(stack.baccarat_address, owner());
    let commitment_id = stack
        .baccarat
        .commit_server_seed(hash_server_seed(SERVER_SEED), REVEAL_DEADLINE);
    stop_cheat_caller_address(stack.baccarat_address);
    commitment_id
}

#[test]
fn test_baccarat_expired_round_can_be_voided_after_block_timeout() {
    let stack = deploy_stack();
    let round_id = open_round(stack, 1_u8, DEFAULT_WAGER);

    start_cheat_block_number(stack.baccarat_address, REVEAL_DEADLINE + 1_u64);
    stack.baccarat.void_expired_round(round_id);
    stop_cheat_block_number(stack.baccarat_address);

    let round = stack.baccarat.get_round(round_id);
    assert(round.status == HandStatus::Voided, 'round voided');
    assert(
        stack.vault.reserved_of(player(), vault_round_id(round_id)) == 0_u128, 'reserve cleared',
    );
    assert(stack.vault.house_locked() == 0_u128, 'house unlocked');
}

fn open_round(stack: BaccaratStack, bet_side: u8, wager: u128) -> u64 {
    let commitment_id = commit_seed(stack);
    start_cheat_caller_address(stack.baccarat_address, player());
    let round_id = stack
        .baccarat
        .open_round(
            TABLE_ID, player(), 0.try_into().unwrap(), wager, bet_side, CLIENT_SEED, commitment_id,
        );
    stop_cheat_caller_address(stack.baccarat_address);
    round_id
}

#[test]
fn test_baccarat_player_opens_and_operator_reveals_round() {
    let stack = deploy_stack();
    let round_id = open_round(stack, 1_u8, DEFAULT_WAGER);
    let round = stack.baccarat.get_round(round_id);
    assert(round.status == HandStatus::Active, 'round active');
    assert(round.bet_side == 1_u8, 'banker bet');

    start_cheat_caller_address(stack.baccarat_address, owner());
    stack.baccarat.settle_round(round_id, SERVER_SEED);
    stop_cheat_caller_address(stack.baccarat_address);

    let settled = stack.baccarat.get_round(round_id);
    assert(settled.status == HandStatus::Settled, 'round settled');
    assert(settled.player_total <= 9_u8, 'player total range');
    assert(settled.banker_total <= 9_u8, 'banker total range');
    assert(settled.player_card_count >= 2_u8, 'player cards');
    assert(settled.banker_card_count >= 2_u8, 'banker cards');
    assert(stack.vault.house_locked() == 0_u128, 'house locked cleared');
    let commitment = stack.baccarat.get_commitment(settled.commitment_id);
    assert(commitment.status == DiceCommitmentStatus::Revealed, 'commitment revealed');

    let p0 = draw_unique_card(round_id, 0_u8, 0_u8, 0_u8, 0_u8, 0_u16, 0_u16, 0_u16, 0_u16, 0_u16);
    let b0 = draw_unique_card(
        round_id, 1_u8, 0_u8, 1_u8, 1_u8, p0.position, 0_u16, 0_u16, 0_u16, 0_u16,
    );
    let p1 = draw_unique_card(
        round_id, 0_u8, 1_u8, 2_u8, 2_u8, p0.position, b0.position, 0_u16, 0_u16, 0_u16,
    );
    let b1 = draw_unique_card(
        round_id, 1_u8, 1_u8, 3_u8, 3_u8, p0.position, b0.position, p1.position, 0_u16, 0_u16,
    );
    assert_revealed_card(stack, round_id, 0_u8, 0_u8, p0);
    assert_revealed_card(stack, round_id, 1_u8, 0_u8, b0);
    assert_revealed_card(stack, round_id, 0_u8, 1_u8, p1);
    assert_revealed_card(stack, round_id, 1_u8, 1_u8, b1);

    let mut player_total = baccarat_total(p0.card, p1.card, 0_u8);
    let mut banker_total = baccarat_total(b0.card, b1.card, 0_u8);
    let mut player_count = 2_u8;
    let mut banker_count = 2_u8;
    let mut player_third = 0_u8;
    let mut player_third_position = 0_u16;
    let mut p2_commitment: felt252 = 0;
    let mut b2_commitment: felt252 = 0;
    let natural = player_total >= 8_u8 || banker_total >= 8_u8;
    if !natural {
        if player_total <= 5_u8 {
            let p2 = draw_unique_card(
                round_id,
                0_u8,
                2_u8,
                4_u8,
                4_u8,
                p0.position,
                b0.position,
                p1.position,
                b1.position,
                0_u16,
            );
            assert_revealed_card(stack, round_id, 0_u8, 2_u8, p2);
            player_third = p2.card;
            player_third_position = p2.position;
            p2_commitment = p2.commitment;
            player_total = baccarat_total(p0.card, p1.card, player_third);
            player_count = 3_u8;
        }
        if banker_draws(banker_total, player_count == 3_u8, baccarat_card_value(player_third)) {
            let (banker_draw_index, used_count) = if player_count == 3_u8 {
                (5_u8, 5_u8)
            } else {
                (4_u8, 4_u8)
            };
            let b2 = draw_unique_card(
                round_id,
                1_u8,
                2_u8,
                banker_draw_index,
                used_count,
                p0.position,
                b0.position,
                p1.position,
                b1.position,
                player_third_position,
            );
            assert_revealed_card(stack, round_id, 1_u8, 2_u8, b2);
            b2_commitment = b2.commitment;
            banker_total = baccarat_total(b0.card, b1.card, b2.card);
            banker_count = 3_u8;
        }
    }

    assert(settled.player_total == player_total, 'player transcript total');
    assert(settled.banker_total == banker_total, 'banker transcript total');
    assert(settled.player_card_count == player_count, 'player transcript count');
    assert(settled.banker_card_count == banker_count, 'banker transcript count');
    assert(
        settled
            .transcript_root == transcript_root(
                round_id,
                p0.commitment,
                b0.commitment,
                p1.commitment,
                b1.commitment,
                p2_commitment,
                b2_commitment,
            ),
        'transcript root',
    );
}

#[test]
#[should_panic(expected: ('BAD_SERVER_SEED',))]
fn test_baccarat_rejects_wrong_server_seed() {
    let stack = deploy_stack();
    let round_id = open_round(stack, 0_u8, DEFAULT_WAGER);

    start_cheat_caller_address(stack.baccarat_address, owner());
    stack.baccarat.settle_round(round_id, 0xBAD);
    stop_cheat_caller_address(stack.baccarat_address);
}

#[test]
#[should_panic(expected: ('SESSION_CALLER',))]
fn test_operator_cannot_open_baccarat_round_for_player() {
    let stack = deploy_stack();
    let commitment_id = commit_seed(stack);

    start_cheat_caller_address(stack.baccarat_address, owner());
    stack
        .baccarat
        .open_round(
            TABLE_ID, player(), session_key(), DEFAULT_WAGER, 0_u8, CLIENT_SEED, commitment_id,
        );
    stop_cheat_caller_address(stack.baccarat_address);
}

#[test]
#[should_panic(expected: ('PAYOUT_LIMIT',))]
fn test_baccarat_rejects_payout_above_admin_cap() {
    let stack = deploy_stack();
    start_cheat_caller_address(stack.baccarat_address, owner());
    stack.baccarat.set_risk_config(100_u128 * ONE_STRK);
    stop_cheat_caller_address(stack.baccarat_address);

    open_round(stack, 2_u8, 20_u128 * ONE_STRK);
}

#[test]
#[should_panic(expected: ('HOUSE_EXPOSURE_CAP',))]
fn test_baccarat_rejects_round_above_dynamic_house_exposure_cap() {
    let stack = deploy_stack();
    open_round(stack, 2_u8, 13_u128 * ONE_STRK);
}

#[test]
#[should_panic(expected: ('GAME_WAGER_CAP',))]
fn test_baccarat_rejects_round_above_absolute_game_cap_even_if_table_limit_is_higher() {
    let stack = deploy_stack();

    start_cheat_caller_address(stack.table_registry_address, owner());
    stack.table_registry.set_table_limits(TABLE_ID, TABLE_MIN_WAGER, 250_u128 * ONE_STRK);
    stop_cheat_caller_address(stack.table_registry_address);

    open_round(stack, 1_u8, 101_u128 * ONE_STRK);
}

#[test]
#[should_panic(expected: ('GAME_WAGER_CAP',))]
fn test_baccarat_owner_can_tighten_absolute_wager_cap() {
    let stack = deploy_stack();

    start_cheat_caller_address(stack.baccarat_address, owner());
    stack.baccarat.set_wager_cap(40_u128 * ONE_STRK);
    stop_cheat_caller_address(stack.baccarat_address);

    assert(stack.baccarat.get_wager_cap() == 40_u128 * ONE_STRK, 'CAP_UPDATED');

    open_round(stack, 1_u8, 41_u128 * ONE_STRK);
}
