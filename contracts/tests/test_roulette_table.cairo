use core::hash::HashStateTrait;
use core::poseidon::PoseidonTrait;
use moros_contracts::interfaces::{
    IBankrollVaultDispatcher, IBankrollVaultDispatcherTrait, IDealerCommitmentDispatcher,
    IDealerCommitmentDispatcherTrait, IERC20Dispatcher, IERC20DispatcherTrait,
    IRouletteTableDispatcher, IRouletteTableDispatcherTrait, ISessionRegistryDispatcher,
    ISessionRegistryDispatcherTrait, ITableRegistryDispatcher, ITableRegistryDispatcherTrait,
};
use moros_contracts::types::{DiceCommitmentStatus, GameKind, HandStatus};
use snforge_std::{
    ContractClassTrait, DeclareResultTrait, declare, start_cheat_block_number,
    start_cheat_caller_address, stop_cheat_block_number, stop_cheat_caller_address,
};
use starknet::ContractAddress;

const TABLE_ID: u64 = 3;
const ONE_STRK: u128 = 1_000_000_000_000_000_000;
const TABLE_MIN_WAGER: u128 = ONE_STRK;
const TABLE_MAX_WAGER: u128 = 100_u128 * ONE_STRK;
const DEFAULT_WAGER: u128 = 10_u128 * ONE_STRK;
const STARTING_BANKROLL: u128 = 1_000_u128 * ONE_STRK;
const HOUSE_LIQUIDITY: u128 = 10_000_u128 * ONE_STRK;
const INITIAL_SUPPLY: u128 = 100_000_u128 * ONE_STRK;
const MAX_ROULETTE_PAYOUT: u128 = 1_000_u128 * ONE_STRK;
const REVEAL_DEADLINE: u64 = 50_u64;
const SESSION_EXPIRY: u64 = 999_999_u64;
const SERVER_SEED: felt252 = 0x987654;
const CLIENT_SEED: felt252 = 0xCAFE;
const SERVER_SEED_DOMAIN: felt252 = 'MOROS_SERVER_SEED';
const ROULETTE_SPIN_DOMAIN: felt252 = 'MOROS_ROULETTE_SPIN';

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

#[derive(Copy, Drop)]
struct RouletteStack {
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
    roulette: IRouletteTableDispatcher,
    roulette_address: ContractAddress,
}

fn deploy_stack() -> RouletteStack {
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

    let roulette_class = declare("RouletteTable").unwrap().contract_class();
    let (roulette_address, _) = roulette_class
        .deploy(
            @array![
                owner().into(), vault_address.into(), table_registry_address.into(),
                sessions_address.into(), commitment_address.into(), MAX_ROULETTE_PAYOUT.into(),
            ],
        )
        .unwrap();
    let roulette = IRouletteTableDispatcher { contract_address: roulette_address };
    start_cheat_block_number(roulette_address, 1_u64);

    start_cheat_caller_address(vault_address, owner());
    vault.set_operator(roulette_address, true);
    stop_cheat_caller_address(vault_address);

    start_cheat_caller_address(commitment_address, owner());
    commitment.set_operator(roulette_address, true);
    stop_cheat_caller_address(commitment_address);

    start_cheat_caller_address(roulette_address, owner());
    roulette.set_operator(owner(), true);
    stop_cheat_caller_address(roulette_address);

    start_cheat_caller_address(table_registry_address, owner());
    table_registry
        .register_table(
            TABLE_ID, roulette_address, GameKind::Roulette, TABLE_MIN_WAGER, TABLE_MAX_WAGER,
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

    RouletteStack {
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
        roulette,
        roulette_address,
    }
}

fn commit_seed(stack: RouletteStack) -> u64 {
    start_cheat_caller_address(stack.roulette_address, owner());
    let commitment_id = stack
        .roulette
        .commit_server_seed(hash_server_seed(SERVER_SEED), REVEAL_DEADLINE);
    stop_cheat_caller_address(stack.roulette_address);
    commitment_id
}

fn open_red_spin(stack: RouletteStack) -> u64 {
    open_single_bet_spin(stack, 1_u8, 0_u8, DEFAULT_WAGER)
}

fn open_single_bet_spin(stack: RouletteStack, kind: u8, selection: u8, amount: u128) -> u64 {
    let commitment_id = commit_seed(stack);
    start_cheat_caller_address(stack.roulette_address, player());
    let spin_id = stack
        .roulette
        .open_spin(
            TABLE_ID,
            player(),
            0.try_into().unwrap(),
            amount,
            CLIENT_SEED,
            commitment_id,
            1_u8,
            kind,
            selection,
            amount,
            0_u8,
            0_u8,
            0_u128,
            0_u8,
            0_u8,
            0_u128,
            0_u8,
            0_u8,
            0_u128,
            0_u8,
            0_u8,
            0_u128,
            0_u8,
            0_u8,
            0_u128,
            0_u8,
            0_u8,
            0_u128,
            0_u8,
            0_u8,
            0_u128,
        );
    stop_cheat_caller_address(stack.roulette_address);
    spin_id
}

#[test]
#[should_panic(expected: ('BET_TYPE_CAP',))]
fn test_roulette_rejects_straight_bet_above_absolute_cap() {
    let stack = deploy_stack();
    let _ = open_single_bet_spin(stack, 0_u8, 17_u8, 26_u128 * ONE_STRK);
}

#[test]
#[should_panic(expected: ('BET_TYPE_CAP',))]
fn test_roulette_rejects_dozen_bet_above_absolute_cap() {
    let stack = deploy_stack();
    let _ = open_single_bet_spin(stack, 7_u8, 1_u8, 71_u128 * ONE_STRK);
}

#[test]
#[should_panic(expected: ('BET_TYPE_CAP',))]
fn test_roulette_owner_can_tighten_per_bet_caps() {
    let stack = deploy_stack();

    start_cheat_caller_address(stack.roulette_address, owner());
    stack.roulette.set_bet_caps(5_u128 * ONE_STRK, 25_u128 * ONE_STRK, 40_u128 * ONE_STRK);
    stop_cheat_caller_address(stack.roulette_address);

    let (straight_cap, dozen_column_cap, even_money_cap) = stack.roulette.get_bet_caps();
    assert(straight_cap == 5_u128 * ONE_STRK, 'STRAIGHT_CAP');
    assert(dozen_column_cap == 25_u128 * ONE_STRK, 'DOZEN_CAP');
    assert(even_money_cap == 40_u128 * ONE_STRK, 'EVEN_CAP');

    let _ = open_single_bet_spin(stack, 0_u8, 17_u8, 6_u128 * ONE_STRK);
}

fn expected_result_number(server_seed: felt252, client_seed: felt252, spin_id: u64) -> u8 {
    let player_felt: felt252 = player().into();
    let mixed = PoseidonTrait::new()
        .update(ROULETTE_SPIN_DOMAIN)
        .update(server_seed)
        .update(client_seed)
        .update(player_felt)
        .update(spin_id.into())
        .finalize();
    let mixed_u256: u256 = mixed.into();
    let denom: u256 = 37_u32.into();
    let result = mixed_u256 % denom;
    result.low.try_into().unwrap()
}

#[test]
fn test_roulette_expired_spin_can_be_voided_after_block_timeout() {
    let stack = deploy_stack();
    let spin_id = open_red_spin(stack);

    start_cheat_block_number(stack.roulette_address, REVEAL_DEADLINE + 1_u64);
    stack.roulette.void_expired_spin(spin_id);
    stop_cheat_block_number(stack.roulette_address);

    let spin = stack.roulette.get_spin(spin_id);
    assert(spin.status == HandStatus::Voided, 'spin voided');
    assert(stack.vault.reserved_of(player(), 8_000_000_001_u64) == 0_u128, 'reserve cleared');
    assert(stack.vault.house_locked() == 0_u128, 'house unlocked');
}

fn open_straight_spin(stack: RouletteStack, selection: u8) -> u64 {
    open_single_bet_spin(stack, 0_u8, selection, DEFAULT_WAGER)
}

#[test]
fn test_roulette_player_opens_and_operator_reveals_spin() {
    let stack = deploy_stack();
    let spin_id = open_red_spin(stack);
    let spin = stack.roulette.get_spin(spin_id);
    assert(spin.status == HandStatus::Active, 'spin active');
    assert(spin.bet_count == 1_u8, 'bet count');
    assert(stack.vault.reserved_of(player(), 8_000_000_001_u64) == DEFAULT_WAGER, 'reserved');

    start_cheat_caller_address(stack.roulette_address, owner());
    stack.roulette.settle_spin(spin_id, SERVER_SEED);
    stop_cheat_caller_address(stack.roulette_address);

    let settled = stack.roulette.get_spin(spin_id);
    assert(settled.status == HandStatus::Settled, 'spin settled');
    assert(settled.result_number <= 36_u8, 'result range');
    assert(stack.vault.house_locked() == 0_u128, 'house locked cleared');
    let commitment = stack.roulette.get_commitment(settled.commitment_id);
    assert(commitment.status == DiceCommitmentStatus::Revealed, 'commitment revealed');
}

#[test]
fn test_roulette_admin_can_lower_payout_rtp() {
    let stack = deploy_stack();
    start_cheat_caller_address(stack.roulette_address, owner());
    stack.roulette.set_risk_config(4865_u32, MAX_ROULETTE_PAYOUT);
    stop_cheat_caller_address(stack.roulette_address);

    let spin_id = stack.roulette.peek_next_spin_id();
    let selection = expected_result_number(SERVER_SEED, CLIENT_SEED, spin_id);
    let opened_spin_id = open_single_bet_spin(stack, 0_u8, selection, 2_u128 * ONE_STRK);

    start_cheat_caller_address(stack.roulette_address, owner());
    stack.roulette.settle_spin(opened_spin_id, SERVER_SEED);
    stop_cheat_caller_address(stack.roulette_address);

    let settled = stack.roulette.get_spin(opened_spin_id);
    assert(settled.result_number == selection, 'target result');
    assert(settled.payout == 36_u128 * ONE_STRK, 'scaled payout');
}

#[test]
fn test_roulette_supports_split_and_top_line_bets() {
    let stack = deploy_stack();
    let wager = 2_u128 * ONE_STRK;
    let preview_spin_id = stack.roulette.peek_next_spin_id();
    let result = expected_result_number(SERVER_SEED, CLIENT_SEED, preview_spin_id);
    let (kind, selection, expected_payout) = if result == 0_u8 {
        (13_u8, 0_u8, wager * 9_u128)
    } else if result <= 33_u8 {
        (10_u8, result, wager * 18_u128)
    } else if (result % 3_u8) == 0_u8 {
        (10_u8, 40_u8 + result - 1_u8, wager * 18_u128)
    } else {
        (10_u8, 40_u8 + result, wager * 18_u128)
    };
    let spin_id = open_single_bet_spin(stack, kind, selection, wager);

    start_cheat_caller_address(stack.roulette_address, owner());
    stack.roulette.settle_spin(spin_id, SERVER_SEED);
    stop_cheat_caller_address(stack.roulette_address);

    let settled = stack.roulette.get_spin(spin_id);
    let bet = stack.roulette.get_bet(spin_id, 0_u8);
    assert(settled.result_number == result, 'result mismatch');
    assert(bet.win, 'bet should win');
    assert(bet.payout == expected_payout, 'split/top-line payout');
    assert(settled.payout == expected_payout, 'spin payout');
}

#[test]
#[should_panic(expected: ('HOUSE_EDGE_HIGH',))]
fn test_roulette_rejects_payout_rtp_above_standard() {
    let stack = deploy_stack();
    start_cheat_caller_address(stack.roulette_address, owner());
    stack.roulette.set_risk_config(10000_u32, MAX_ROULETTE_PAYOUT);
    stop_cheat_caller_address(stack.roulette_address);
}

#[test]
#[should_panic(expected: ('BAD_SERVER_SEED',))]
fn test_roulette_rejects_wrong_server_seed() {
    let stack = deploy_stack();
    let spin_id = open_red_spin(stack);

    start_cheat_caller_address(stack.roulette_address, owner());
    stack.roulette.settle_spin(spin_id, 0xBAD);
    stop_cheat_caller_address(stack.roulette_address);
}

#[test]
#[should_panic(expected: ('SESSION_CALLER',))]
fn test_operator_cannot_open_roulette_spin_for_player() {
    let stack = deploy_stack();
    let commitment_id = commit_seed(stack);

    start_cheat_caller_address(stack.roulette_address, owner());
    stack
        .roulette
        .open_spin(
            TABLE_ID,
            player(),
            session_key(),
            DEFAULT_WAGER,
            CLIENT_SEED,
            commitment_id,
            1_u8,
            1_u8,
            0_u8,
            DEFAULT_WAGER,
            0_u8,
            0_u8,
            0_u128,
            0_u8,
            0_u8,
            0_u128,
            0_u8,
            0_u8,
            0_u128,
            0_u8,
            0_u8,
            0_u128,
            0_u8,
            0_u8,
            0_u128,
            0_u8,
            0_u8,
            0_u128,
            0_u8,
            0_u8,
            0_u128,
        );
    stop_cheat_caller_address(stack.roulette_address);
}

#[test]
#[should_panic(expected: ('PAYOUT_LIMIT',))]
fn test_roulette_rejects_payout_above_admin_cap() {
    let stack = deploy_stack();
    let commitment_id = commit_seed(stack);

    start_cheat_caller_address(stack.roulette_address, player());
    stack
        .roulette
        .open_spin(
            TABLE_ID,
            player(),
            0.try_into().unwrap(),
            100_u128 * ONE_STRK,
            CLIENT_SEED,
            commitment_id,
            1_u8,
            9_u8,
            0_u8,
            100_u128 * ONE_STRK,
            0_u8,
            0_u8,
            0_u128,
            0_u8,
            0_u8,
            0_u128,
            0_u8,
            0_u8,
            0_u128,
            0_u8,
            0_u8,
            0_u128,
            0_u8,
            0_u8,
            0_u128,
            0_u8,
            0_u8,
            0_u128,
            0_u8,
            0_u8,
            0_u128,
        );
    stop_cheat_caller_address(stack.roulette_address);
}

#[test]
#[should_panic(expected: ('HOUSE_EXPOSURE_CAP',))]
fn test_roulette_rejects_spin_above_dynamic_house_exposure_cap() {
    let stack = deploy_stack();
    let commitment_id = commit_seed(stack);

    start_cheat_caller_address(stack.roulette_address, player());
    stack
        .roulette
        .open_spin(
            TABLE_ID,
            player(),
            0.try_into().unwrap(),
            3_u128 * ONE_STRK,
            CLIENT_SEED,
            commitment_id,
            1_u8,
            0_u8,
            7_u8,
            3_u128 * ONE_STRK,
            0_u8,
            0_u8,
            0_u128,
            0_u8,
            0_u8,
            0_u128,
            0_u8,
            0_u8,
            0_u128,
            0_u8,
            0_u8,
            0_u128,
            0_u8,
            0_u8,
            0_u128,
            0_u8,
            0_u8,
            0_u128,
            0_u8,
            0_u8,
            0_u128,
        );
    stop_cheat_caller_address(stack.roulette_address);
}
