use moros_contracts::interfaces::{
    IBankrollVaultDispatcher, IBankrollVaultDispatcherTrait, IERC20Dispatcher,
    IERC20DispatcherTrait,
};
use snforge_std::{
    ContractClassTrait, DeclareResultTrait, declare, start_cheat_block_timestamp,
    start_cheat_caller_address, stop_cheat_block_timestamp, stop_cheat_caller_address,
};
use starknet::ContractAddress;

const ONE_STRK: u128 = 1_000_000_000_000_000_000;
const HOUSE_LIQUIDITY: u128 = 2_000_u128 * ONE_STRK;
const PLAYER_DEPOSIT: u128 = 250_u128 * ONE_STRK;
const HAND_ID: u64 = 77;
const INITIAL_SUPPLY: u128 = 50_000_u128 * ONE_STRK;

fn owner() -> ContractAddress {
    0x1111.try_into().unwrap()
}

fn player() -> ContractAddress {
    0x2222.try_into().unwrap()
}

fn withdrawal_recipient() -> ContractAddress {
    0x3333.try_into().unwrap()
}

fn attacker() -> ContractAddress {
    0x4444.try_into().unwrap()
}

#[derive(Copy, Drop)]
struct VaultStack {
    token: IERC20Dispatcher,
    token_address: ContractAddress,
    vault: IBankrollVaultDispatcher,
    vault_address: ContractAddress,
}

fn deploy_stack() -> VaultStack {
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

    start_cheat_caller_address(token_address, owner());
    token.transfer(player(), PLAYER_DEPOSIT.into());
    token.approve(vault_address, HOUSE_LIQUIDITY.into());
    stop_cheat_caller_address(token_address);

    start_cheat_caller_address(vault_address, owner());
    vault.deposit_house_liquidity(HOUSE_LIQUIDITY);
    stop_cheat_caller_address(vault_address);

    start_cheat_caller_address(token_address, player());
    token.approve(vault_address, PLAYER_DEPOSIT.into());
    stop_cheat_caller_address(token_address);

    VaultStack { token, token_address, vault, vault_address }
}

#[test]
fn test_public_deposit_and_withdraw_move_real_strk() {
    let stack = deploy_stack();

    start_cheat_caller_address(stack.vault_address, player());
    stack.vault.deposit_public(player(), PLAYER_DEPOSIT);
    stop_cheat_caller_address(stack.vault_address);

    assert(stack.vault.balance_of(player()) == PLAYER_DEPOSIT, 'public balance credited');
    assert(stack.vault.gambling_balance_of(player()) == PLAYER_DEPOSIT, 'gambling credited');
    assert(stack.vault.vault_balance_of(player()) == 0_u128, 'vault starts empty');
    assert(stack.vault.total_player_liabilities() == PLAYER_DEPOSIT, 'liability tracked');
    assert(
        stack.token.balance_of(stack.vault_address).low == HOUSE_LIQUIDITY + PLAYER_DEPOSIT,
        'TOKEN_CUSTODY_OK',
    );

    start_cheat_caller_address(stack.vault_address, player());
    stack.vault.withdraw_public(player(), 50_u128 * ONE_STRK);
    stop_cheat_caller_address(stack.vault_address);

    assert(
        stack.vault.balance_of(player()) == PLAYER_DEPOSIT - (50_u128 * ONE_STRK),
        'PLAYER_BAL_DOWN',
    );
    assert(
        stack.vault.total_player_liabilities() == PLAYER_DEPOSIT - (50_u128 * ONE_STRK),
        'LIABILITY_DOWN',
    );
    assert(
        stack.token.balance_of(player()).low == 50_u128 * ONE_STRK,
        'player receives real STRK back',
    );
}

#[test]
fn test_vault_balance_is_not_available_to_gameplay_until_moved() {
    let stack = deploy_stack();

    start_cheat_caller_address(stack.vault_address, player());
    stack.vault.deposit_to_vault(player(), PLAYER_DEPOSIT);
    stop_cheat_caller_address(stack.vault_address);

    assert(stack.vault.balance_of(player()) == 0_u128, 'gambling untouched');
    assert(stack.vault.vault_balance_of(player()) == PLAYER_DEPOSIT, 'vault credited');
    assert(stack.vault.total_player_liabilities() == PLAYER_DEPOSIT, 'liability tracked');

    start_cheat_caller_address(stack.vault_address, owner());
    stack.vault.set_operator(owner(), true);
    stop_cheat_caller_address(stack.vault_address);

    start_cheat_caller_address(stack.vault_address, player());
    stack.vault.move_to_gambling(100_u128 * ONE_STRK);
    stop_cheat_caller_address(stack.vault_address);

    assert(stack.vault.balance_of(player()) == 100_u128 * ONE_STRK, 'gambling available');
    assert(
        stack.vault.vault_balance_of(player()) == PLAYER_DEPOSIT - (100_u128 * ONE_STRK),
        'vault reduced',
    );

    start_cheat_caller_address(stack.vault_address, owner());
    stack.vault.reserve_for_hand(player(), HAND_ID, 100_u128 * ONE_STRK);
    stop_cheat_caller_address(stack.vault_address);

    assert(stack.vault.balance_of(player()) == 0_u128, 'reserved from gambling only');
    assert(
        stack.vault.vault_balance_of(player()) == PLAYER_DEPOSIT - (100_u128 * ONE_STRK),
        'vault isolated',
    );
}

#[test]
fn test_player_can_move_gambling_balance_to_vault_and_withdraw_vault() {
    let stack = deploy_stack();

    start_cheat_caller_address(stack.vault_address, player());
    stack.vault.deposit_public(player(), PLAYER_DEPOSIT);
    stack.vault.move_to_vault(75_u128 * ONE_STRK);
    stack.vault.withdraw_from_vault(player(), 25_u128 * ONE_STRK);
    stop_cheat_caller_address(stack.vault_address);

    assert(
        stack.vault.balance_of(player()) == PLAYER_DEPOSIT - (75_u128 * ONE_STRK), 'gambling moved',
    );
    assert(stack.vault.vault_balance_of(player()) == 50_u128 * ONE_STRK, 'vault withdrawn');
    assert(
        stack.vault.total_player_liabilities() == PLAYER_DEPOSIT - (25_u128 * ONE_STRK),
        'liability after vault withdraw',
    );
}

#[test]
#[should_panic(expected: ('INSUFFICIENT_VAULT',))]
fn test_other_wallet_cannot_withdraw_player_vault_balance() {
    let stack = deploy_stack();

    start_cheat_caller_address(stack.vault_address, player());
    stack.vault.deposit_to_vault(player(), PLAYER_DEPOSIT);
    stop_cheat_caller_address(stack.vault_address);

    start_cheat_caller_address(stack.vault_address, attacker());
    stack.vault.withdraw_from_vault(player(), 1_u128 * ONE_STRK);
    stop_cheat_caller_address(stack.vault_address);
}

#[test]
fn test_player_can_withdraw_vault_balance_to_custom_recipient() {
    let stack = deploy_stack();

    start_cheat_caller_address(stack.vault_address, player());
    stack.vault.deposit_to_vault(player(), PLAYER_DEPOSIT);
    stack.vault.withdraw_from_vault(withdrawal_recipient(), 25_u128 * ONE_STRK);
    stop_cheat_caller_address(stack.vault_address);

    assert(
        stack.vault.vault_balance_of(player()) == PLAYER_DEPOSIT - (25_u128 * ONE_STRK),
        'VAULT_DEBITED',
    );
    assert(
        stack.token.balance_of(withdrawal_recipient()).low == 25_u128 * ONE_STRK, 'RECIPIENT_PAID',
    );
}

#[test]
#[should_panic(expected: ('USER_WITHDRAW_ONLY',))]
fn test_operator_cannot_withdraw_player_balance_without_player_caller() {
    let stack = deploy_stack();

    start_cheat_caller_address(stack.vault_address, player());
    stack.vault.deposit_public(player(), PLAYER_DEPOSIT);
    stop_cheat_caller_address(stack.vault_address);

    start_cheat_caller_address(stack.vault_address, owner());
    stack.vault.operator_withdraw_public(player(), withdrawal_recipient(), 40_u128 * ONE_STRK);
    stop_cheat_caller_address(stack.vault_address);
}

#[test]
fn test_house_liquidity_is_locked_and_released_on_settle() {
    let stack = deploy_stack();

    start_cheat_caller_address(stack.vault_address, player());
    stack.vault.deposit_public(player(), PLAYER_DEPOSIT);
    stop_cheat_caller_address(stack.vault_address);

    start_cheat_caller_address(stack.vault_address, owner());
    stack.vault.reserve_for_hand(player(), HAND_ID, 100_u128 * ONE_STRK);
    stack.vault.lock_house_exposure(HAND_ID, 100_u128 * ONE_STRK);
    stop_cheat_caller_address(stack.vault_address);

    assert(
        stack.vault.house_available() == HOUSE_LIQUIDITY - (100_u128 * ONE_STRK),
        'HOUSE_LOCKED_ONE',
    );
    assert(stack.vault.house_locked() == 100_u128 * ONE_STRK, 'house exposure locked');
    assert(
        stack.vault.reserved_of(player(), HAND_ID) == 100_u128 * ONE_STRK, 'player wager reserved',
    );
    assert(stack.vault.total_player_liabilities() == PLAYER_DEPOSIT, 'liability includes reserved');

    start_cheat_caller_address(stack.vault_address, owner());
    stack.vault.settle_hand(player(), HAND_ID, 200_u128 * ONE_STRK);
    stop_cheat_caller_address(stack.vault_address);

    assert(
        stack.vault.balance_of(player()) == PLAYER_DEPOSIT
            - (100_u128 * ONE_STRK)
            + (200_u128 * ONE_STRK),
        'player receives backed payout',
    );
    assert(
        stack.vault.house_available() == HOUSE_LIQUIDITY - (100_u128 * ONE_STRK), 'HOUSE_STD_WIN',
    );
    assert(stack.vault.house_locked() == 0_u128, 'house exposure released');
    assert(stack.vault.hand_exposure_of(HAND_ID) == 0_u128, 'hand exposure cleared');
    assert(
        stack.vault.total_player_liabilities() == PLAYER_DEPOSIT
            - (100_u128 * ONE_STRK)
            + (200_u128 * ONE_STRK),
        'settled liability tracked',
    );
}

#[test]
#[should_panic(expected: 'HOUSE_LIQUIDITY_LOW')]
fn test_house_cannot_withdraw_locked_or_missing_liquidity() {
    let stack = deploy_stack();

    start_cheat_caller_address(stack.vault_address, owner());
    stack.vault.withdraw_house_liquidity(owner(), HOUSE_LIQUIDITY + ONE_STRK);
    stop_cheat_caller_address(stack.vault_address);
}

#[test]
fn test_house_withdrawals_are_queued_timelocked_and_executable() {
    let stack = deploy_stack();

    start_cheat_block_timestamp(stack.vault_address, 1_u64);
    start_cheat_caller_address(stack.vault_address, owner());
    let request_id_raw = stack
        .vault
        .withdraw_house_liquidity(withdrawal_recipient(), 100_u128 * ONE_STRK);
    stop_cheat_caller_address(stack.vault_address);

    let request_id: u64 = request_id_raw.try_into().unwrap();
    let request = stack.vault.house_withdrawal(request_id);
    assert(request.request_id == request_id, 'REQUEST_STORED');
    assert(request.active, 'REQUEST_ACTIVE');
    assert(
        request.execute_after == 1_u64 + stack.vault.house_withdraw_delay_seconds(),
        'EXECUTE_AFTER',
    );
    assert(
        stack.vault.house_available() == HOUSE_LIQUIDITY - (100_u128 * ONE_STRK), 'HOUSE_RESERVED',
    );

    start_cheat_block_timestamp(
        stack.vault_address, 1_u64 + stack.vault.house_withdraw_delay_seconds(),
    );
    start_cheat_caller_address(stack.vault_address, owner());
    stack.vault.execute_house_withdrawal(request_id);
    stop_cheat_caller_address(stack.vault_address);
    stop_cheat_block_timestamp(stack.vault_address);

    assert(
        stack.token.balance_of(withdrawal_recipient()).low == 100_u128 * ONE_STRK,
        'HOUSE_WITHDRAW_PAID',
    );
    assert(
        stack.vault.house_available() == HOUSE_LIQUIDITY - (100_u128 * ONE_STRK),
        'HOUSE_AVAILABLE_STAYS_RESERVED',
    );
    assert(!stack.vault.house_withdrawal(request_id).active, 'REQUEST_EXECUTED');
}

#[test]
#[should_panic(expected: ('HOUSE_WITHDRAW_TIMELOCK',))]
fn test_house_withdrawal_cannot_execute_before_timelock() {
    let stack = deploy_stack();

    start_cheat_block_timestamp(stack.vault_address, 1_u64);
    start_cheat_caller_address(stack.vault_address, owner());
    let request_id_raw = stack.vault.withdraw_house_liquidity(owner(), 10_u128 * ONE_STRK);
    let request_id: u64 = request_id_raw.try_into().unwrap();
    stack.vault.execute_house_withdrawal(request_id);
    stop_cheat_caller_address(stack.vault_address);
}

#[test]
fn test_house_withdrawal_can_be_cancelled_and_restores_liquidity() {
    let stack = deploy_stack();

    start_cheat_caller_address(stack.vault_address, owner());
    let request_id_raw = stack
        .vault
        .withdraw_house_liquidity(withdrawal_recipient(), 25_u128 * ONE_STRK);
    let request_id: u64 = request_id_raw.try_into().unwrap();
    let restored = stack.vault.cancel_house_withdrawal(request_id);
    stop_cheat_caller_address(stack.vault_address);

    assert(restored == HOUSE_LIQUIDITY, 'HOUSE_RESTORED');
    assert(stack.vault.house_available() == HOUSE_LIQUIDITY, 'AVAILABLE_RESTORED');
    assert(!stack.vault.house_withdrawal(request_id).active, 'REQUEST_CLOSED');
    stop_cheat_block_timestamp(stack.vault_address);
}

#[test]
fn test_owner_can_fund_rewards_treasury_from_house_liquidity() {
    let stack = deploy_stack();
    let treasury_recipient: ContractAddress = 0x7777.try_into().unwrap();

    start_cheat_caller_address(stack.vault_address, owner());
    let next_available = stack.vault.fund_rewards_treasury(treasury_recipient, 75_u128 * ONE_STRK);
    stop_cheat_caller_address(stack.vault_address);

    assert(next_available == HOUSE_LIQUIDITY - (75_u128 * ONE_STRK), 'HOUSE_AVAILABLE_REDUCED');
    assert(
        stack.token.balance_of(treasury_recipient).low == 75_u128 * ONE_STRK,
        'REWARDS_TREASURY_FUNDED',
    );
    assert(
        stack.vault.house_available() == HOUSE_LIQUIDITY - (75_u128 * ONE_STRK), 'HOUSE_TRACKED',
    );
}

#[test]
#[should_panic(expected: ('HAND_OPERATOR_MISMATCH',))]
fn test_reserved_hand_id_cannot_be_reused_by_another_operator() {
    let stack = deploy_stack();

    start_cheat_caller_address(stack.vault_address, player());
    stack.vault.deposit_public(player(), PLAYER_DEPOSIT);
    stop_cheat_caller_address(stack.vault_address);

    start_cheat_caller_address(stack.vault_address, owner());
    stack.vault.set_operator(attacker(), true);
    stack.vault.reserve_for_hand(player(), HAND_ID, 25_u128 * ONE_STRK);
    stop_cheat_caller_address(stack.vault_address);

    start_cheat_caller_address(stack.vault_address, attacker());
    stack.vault.reserve_for_hand(player(), HAND_ID, 25_u128 * ONE_STRK);
    stop_cheat_caller_address(stack.vault_address);
}

#[test]
#[should_panic(expected: ('HAND_OPERATOR_MISMATCH',))]
fn test_reserved_hand_cannot_be_settled_by_another_operator() {
    let stack = deploy_stack();

    start_cheat_caller_address(stack.vault_address, player());
    stack.vault.deposit_public(player(), PLAYER_DEPOSIT);
    stop_cheat_caller_address(stack.vault_address);

    start_cheat_caller_address(stack.vault_address, owner());
    stack.vault.set_operator(attacker(), true);
    stack.vault.reserve_for_hand(player(), HAND_ID, 25_u128 * ONE_STRK);
    stack.vault.lock_house_exposure(HAND_ID, 25_u128 * ONE_STRK);
    stop_cheat_caller_address(stack.vault_address);

    start_cheat_caller_address(stack.vault_address, attacker());
    stack.vault.settle_hand(player(), HAND_ID, 50_u128 * ONE_STRK);
    stop_cheat_caller_address(stack.vault_address);
}
