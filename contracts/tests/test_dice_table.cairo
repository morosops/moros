use core::hash::HashStateTrait;
use core::poseidon::PoseidonTrait;
use moros_contracts::interfaces::{
    IBankrollVaultDispatcher, IBankrollVaultDispatcherTrait, IDealerCommitmentDispatcher,
    IDealerCommitmentDispatcherTrait, IDiceTableDispatcher, IDiceTableDispatcherTrait,
    IERC20Dispatcher, IERC20DispatcherTrait, ISessionRegistryDispatcher,
    ISessionRegistryDispatcherTrait, ITableRegistryDispatcher, ITableRegistryDispatcherTrait,
};
use moros_contracts::types::{DiceCommitmentStatus, GameKind, HandStatus};
use snforge_std::{
    ContractClassTrait, DeclareResultTrait, declare, start_cheat_block_number,
    start_cheat_caller_address, stop_cheat_block_number, stop_cheat_caller_address,
};
use starknet::ContractAddress;

const TABLE_ID: u64 = 1;
const ONE_STRK: u128 = 1_000_000_000_000_000_000;
const TABLE_MIN_WAGER: u128 = ONE_STRK;
const TABLE_MAX_WAGER: u128 = 100_u128 * ONE_STRK;
const DEFAULT_WAGER: u128 = 10_u128 * ONE_STRK;
const STARTING_BANKROLL: u128 = 1_000_u128 * ONE_STRK;
const HOUSE_LIQUIDITY: u128 = 10_000_u128 * ONE_STRK;
const INITIAL_SUPPLY: u128 = 100_000_u128 * ONE_STRK;
const MAX_DICE_PAYOUT: u128 = 500_u128 * ONE_STRK;
const REVEAL_DEADLINE: u64 = 50_u64;
const SESSION_EXPIRY: u64 = 999_999_u64;
const SERVER_SEED: felt252 = 0x123456;
const CLIENT_SEED: felt252 = 0xCAFE;
const DICE_VAULT_ID_OFFSET: u64 = 1_000_000_000_u64;
const SERVER_SEED_DOMAIN: felt252 = 'MOROS_SERVER_SEED';

fn hash_server_seed(server_seed: felt252) -> felt252 {
    PoseidonTrait::new().update(SERVER_SEED_DOMAIN).update(server_seed).finalize()
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

fn vault_round_id(round_id: u64) -> u64 {
    DICE_VAULT_ID_OFFSET + (TABLE_ID * DICE_VAULT_ID_OFFSET) + round_id
}

#[derive(Copy, Drop)]
struct DiceStack {
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
    dice: IDiceTableDispatcher,
    dice_address: ContractAddress,
}

fn deploy_stack() -> DiceStack {
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

    let dice_class = declare("DiceTable").unwrap().contract_class();
    let (dice_address, _) = dice_class
        .deploy(
            @array![
                owner().into(), vault_address.into(), table_registry_address.into(),
                sessions_address.into(), commitment_address.into(), MAX_DICE_PAYOUT.into(),
            ],
        )
        .unwrap();
    let dice = IDiceTableDispatcher { contract_address: dice_address };
    start_cheat_block_number(dice_address, 1_u64);

    start_cheat_caller_address(vault_address, owner());
    vault.set_operator(dice_address, true);
    stop_cheat_caller_address(vault_address);

    start_cheat_caller_address(commitment_address, owner());
    commitment.set_operator(dice_address, true);
    stop_cheat_caller_address(commitment_address);

    start_cheat_caller_address(dice_address, owner());
    dice.set_operator(owner(), true);
    stop_cheat_caller_address(dice_address);

    start_cheat_caller_address(table_registry_address, owner());
    table_registry
        .register_table(TABLE_ID, dice_address, GameKind::Dice, TABLE_MIN_WAGER, TABLE_MAX_WAGER);
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

    DiceStack {
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
        dice,
        dice_address,
    }
}

fn commit_seed(stack: DiceStack) -> u64 {
    start_cheat_caller_address(stack.dice_address, owner());
    let commitment_id = stack
        .dice
        .commit_server_seed(hash_server_seed(SERVER_SEED), REVEAL_DEADLINE);
    stop_cheat_caller_address(stack.dice_address);
    commitment_id
}

fn open_round(stack: DiceStack, target_bps: u32, roll_over: bool) -> u64 {
    let commitment_id = commit_seed(stack);
    start_cheat_caller_address(stack.dice_address, player());
    let round_id = stack
        .dice
        .open_round(
            TABLE_ID,
            player(),
            0.try_into().unwrap(),
            DEFAULT_WAGER,
            target_bps,
            roll_over,
            CLIENT_SEED,
            commitment_id,
        );
    stop_cheat_caller_address(stack.dice_address);
    round_id
}

#[test]
#[should_panic(expected: ('GAME_WAGER_CAP',))]
fn test_dice_rejects_round_above_absolute_game_cap() {
    let stack = deploy_stack();
    let commitment_id = commit_seed(stack);

    start_cheat_caller_address(stack.dice_address, player());
    stack
        .dice
        .open_round(
            TABLE_ID,
            player(),
            0.try_into().unwrap(),
            26_u128 * ONE_STRK,
            5_000_u32,
            false,
            CLIENT_SEED,
            commitment_id,
        );
    stop_cheat_caller_address(stack.dice_address);
}

#[test]
#[should_panic(expected: ('GAME_WAGER_CAP',))]
fn test_dice_owner_can_tighten_absolute_wager_cap() {
    let stack = deploy_stack();
    let commitment_id = commit_seed(stack);

    start_cheat_caller_address(stack.dice_address, owner());
    stack.dice.set_wager_cap(15_u128 * ONE_STRK);
    stop_cheat_caller_address(stack.dice_address);

    assert(stack.dice.get_wager_cap() == 15_u128 * ONE_STRK, 'CAP_UPDATED');

    start_cheat_caller_address(stack.dice_address, player());
    stack
        .dice
        .open_round(
            TABLE_ID,
            player(),
            0.try_into().unwrap(),
            16_u128 * ONE_STRK,
            5_000_u32,
            false,
            CLIENT_SEED,
            commitment_id,
        );
    stop_cheat_caller_address(stack.dice_address);
}

#[test]
fn test_dice_quote_matches_one_percent_house_edge() {
    let stack = deploy_stack();
    let (chance_bps, multiplier_bps, payout, exposure) = stack
        .dice
        .quote_payout(DEFAULT_WAGER, 5000_u32, false);

    assert(chance_bps == 5000_u32, 'chance should be 50pct');
    assert(multiplier_bps == 19800_u32, 'multiplier should be 1.98x');
    assert(payout == 19_800_000_000_000_000_000_u128, 'payout');
    assert(exposure == 9_800_000_000_000_000_000_u128, 'exposure');
}

#[test]
fn test_player_opens_and_operator_reveals_settled_round() {
    let stack = deploy_stack();
    let round_id = open_round(stack, 9800_u32, false);

    let opened = stack.dice.get_round(round_id);
    assert(opened.status == HandStatus::Active, 'round active');
    assert(opened.server_seed_hash == hash_server_seed(SERVER_SEED), 'seed hash stored');
    assert(opened.client_seed == CLIENT_SEED, 'client seed stored');
    assert(
        stack.vault.reserved_of(player(), vault_round_id(round_id)) == DEFAULT_WAGER, 'reserved',
    );

    start_cheat_caller_address(stack.dice_address, owner());
    stack.dice.settle_round(round_id, SERVER_SEED);
    stop_cheat_caller_address(stack.dice_address);

    let settled = stack.dice.get_round(round_id);
    assert(settled.status == HandStatus::Settled, 'round settled');
    assert(settled.roll_bps < 10000_u32, 'roll in range');
    let commitment = stack.dice.get_commitment(settled.commitment_id);
    assert(commitment.status == DiceCommitmentStatus::Revealed, 'commitment revealed');
    assert(stack.vault.house_locked() == 0_u128, 'house locked cleared');
}

#[test]
fn test_dice_expired_round_can_be_voided_after_block_timeout() {
    let stack = deploy_stack();
    let round_id = open_round(stack, 5000_u32, false);

    start_cheat_block_number(stack.dice_address, REVEAL_DEADLINE + 1_u64);
    stack.dice.void_expired_round(round_id);
    stop_cheat_block_number(stack.dice_address);

    let round = stack.dice.get_round(round_id);
    assert(round.status == HandStatus::Voided, 'round voided');
    assert(
        stack.vault.reserved_of(player(), vault_round_id(round_id)) == 0_u128, 'reserve cleared',
    );
    assert(stack.vault.house_locked() == 0_u128, 'house unlocked');
}

#[test]
fn test_zero_wager_round_can_open_and_settle_when_table_min_is_zero() {
    let stack = deploy_stack();

    start_cheat_caller_address(stack.table_registry_address, owner());
    stack.table_registry.set_table_limits(TABLE_ID, 0_u128, TABLE_MAX_WAGER);
    stop_cheat_caller_address(stack.table_registry_address);

    let commitment_id = commit_seed(stack);

    start_cheat_caller_address(stack.dice_address, player());
    let round_id = stack
        .dice
        .open_round(
            TABLE_ID,
            player(),
            0.try_into().unwrap(),
            0_u128,
            5000_u32,
            false,
            CLIENT_SEED,
            commitment_id,
        );
    stop_cheat_caller_address(stack.dice_address);

    let opened = stack.dice.get_round(round_id);
    assert(opened.wager == 0_u128, 'wager stored');
    assert(stack.vault.reserved_of(player(), vault_round_id(round_id)) == 0_u128, 'no reserve');

    start_cheat_caller_address(stack.dice_address, owner());
    stack.dice.settle_round(round_id, SERVER_SEED);
    stop_cheat_caller_address(stack.dice_address);

    let settled = stack.dice.get_round(round_id);
    assert(settled.status == HandStatus::Settled, 'round settled');
    assert(settled.payout == 0_u128, 'zero payout');
    assert(stack.vault.house_locked() == 0_u128, 'house not locked');
}

#[test]
#[should_panic(expected: ('BAD_SERVER_SEED',))]
fn test_settle_rejects_wrong_server_seed() {
    let stack = deploy_stack();
    let round_id = open_round(stack, 9800_u32, false);

    start_cheat_caller_address(stack.dice_address, owner());
    stack.dice.settle_round(round_id, 0xBAD);
    stop_cheat_caller_address(stack.dice_address);
}

#[test]
#[should_panic(expected: ('SESSION_CALLER',))]
fn test_operator_cannot_open_round_for_player_without_player_or_session_caller() {
    let stack = deploy_stack();
    let commitment_id = commit_seed(stack);

    start_cheat_caller_address(stack.dice_address, owner());
    stack
        .dice
        .open_round(
            TABLE_ID,
            player(),
            session_key(),
            DEFAULT_WAGER,
            5000_u32,
            false,
            CLIENT_SEED,
            commitment_id,
        );
    stop_cheat_caller_address(stack.dice_address);
}

#[test]
fn test_registered_session_key_can_open_with_player_limit() {
    let stack = deploy_stack();
    let commitment_id = commit_seed(stack);

    start_cheat_caller_address(stack.dice_address, session_key());
    let round_id = stack
        .dice
        .open_round(
            TABLE_ID,
            player(),
            session_key(),
            DEFAULT_WAGER,
            5000_u32,
            false,
            CLIENT_SEED,
            commitment_id,
        );
    stop_cheat_caller_address(stack.dice_address);

    let round = stack.dice.get_round(round_id);
    assert(round.status == HandStatus::Active, 'session opened round');
}

#[test]
#[should_panic(expected: ('PAYOUT_LIMIT',))]
fn test_dice_rejects_rounds_above_admin_payout_limit() {
    let stack = deploy_stack();
    let commitment_id = commit_seed(stack);

    start_cheat_caller_address(stack.dice_address, player());
    stack
        .dice
        .open_round(
            TABLE_ID,
            player(),
            0.try_into().unwrap(),
            DEFAULT_WAGER,
            100_u32,
            false,
            CLIENT_SEED,
            commitment_id,
        );
    stop_cheat_caller_address(stack.dice_address);
}

#[test]
#[should_panic(expected: ('HOUSE_EXPOSURE_CAP',))]
fn test_dice_rejects_rounds_above_dynamic_house_exposure_cap() {
    let stack = deploy_stack();
    let commitment_id = commit_seed(stack);

    start_cheat_caller_address(stack.dice_address, owner());
    stack.dice.set_risk_config(100_u32, 9800_u32, 9900_u32, 2_000_u128 * ONE_STRK);
    stop_cheat_caller_address(stack.dice_address);

    start_cheat_caller_address(stack.dice_address, player());
    stack
        .dice
        .open_round(
            TABLE_ID,
            player(),
            0.try_into().unwrap(),
            2_u128 * ONE_STRK,
            100_u32,
            false,
            CLIENT_SEED,
            commitment_id,
        );
    stop_cheat_caller_address(stack.dice_address);
}

#[test]
fn test_admin_can_tighten_table_limits_onchain() {
    let stack = deploy_stack();
    let tighter_max = 5_u128 * ONE_STRK;

    start_cheat_caller_address(stack.table_registry_address, owner());
    stack.table_registry.set_table_limits(TABLE_ID, TABLE_MIN_WAGER, tighter_max);
    stop_cheat_caller_address(stack.table_registry_address);

    let table = stack.table_registry.get_table(TABLE_ID);
    assert(table.max_wager == tighter_max, 'max wager updated');
}
