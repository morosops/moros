use moros_contracts::interfaces::{
    IBankrollVaultDispatcher, IBankrollVaultDispatcherTrait, IERC20Dispatcher,
    IERC20DispatcherTrait, IRewardsTreasuryDispatcher, IRewardsTreasuryDispatcherTrait,
};
use snforge_std::{
    ContractClassTrait, DeclareResultTrait, declare, start_cheat_caller_address,
    stop_cheat_caller_address,
};
use starknet::ContractAddress;

const ONE_STRK: u128 = 1_000_000_000_000_000_000;
const INITIAL_SUPPLY: u128 = 50_000_u128 * ONE_STRK;
const FUNDING_AMOUNT: u128 = 500_u128 * ONE_STRK;

fn owner() -> ContractAddress {
    0x1111.try_into().unwrap()
}

fn player() -> ContractAddress {
    0x2222.try_into().unwrap()
}

fn operator() -> ContractAddress {
    0x3333.try_into().unwrap()
}

#[derive(Copy, Drop)]
struct RewardsStack {
    token: IERC20Dispatcher,
    token_address: ContractAddress,
    vault: IBankrollVaultDispatcher,
    vault_address: ContractAddress,
    treasury: IRewardsTreasuryDispatcher,
    treasury_address: ContractAddress,
}

fn deploy_stack() -> RewardsStack {
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

    let treasury_class = declare("RewardsTreasury").unwrap().contract_class();
    let (treasury_address, _) = treasury_class
        .deploy(@array![owner().into(), token_address.into(), vault_address.into()])
        .unwrap();
    let treasury = IRewardsTreasuryDispatcher { contract_address: treasury_address };

    start_cheat_caller_address(token_address, owner());
    token.transfer(player(), FUNDING_AMOUNT.into());
    token.approve(treasury_address, FUNDING_AMOUNT.into());
    stop_cheat_caller_address(token_address);

    RewardsStack { token, token_address, vault, vault_address, treasury, treasury_address }
}

#[test]
fn test_rewards_treasury_accepts_funding_and_tracks_balance() {
    let stack = deploy_stack();

    start_cheat_caller_address(stack.treasury_address, owner());
    let total_funded = stack.treasury.fund(FUNDING_AMOUNT);
    stop_cheat_caller_address(stack.treasury_address);

    assert(total_funded == FUNDING_AMOUNT, 'TOTAL_FUNDED');
    assert(stack.treasury.total_funded() == FUNDING_AMOUNT, 'FUNDED_TRACKED');
    assert(stack.treasury.total_claimed() == 0_u128, 'CLAIMED_ZERO');
    assert(stack.treasury.available_rewards() == FUNDING_AMOUNT, 'AVAILABLE_BALANCE');
}

#[test]
fn test_rewards_treasury_operator_can_credit_player_vault() {
    let stack = deploy_stack();

    start_cheat_caller_address(stack.treasury_address, owner());
    stack.treasury.fund(FUNDING_AMOUNT);
    stack.treasury.set_operator(operator(), true);
    stack.treasury.set_reward_budget_cap(100_u128 * ONE_STRK);
    stack.treasury.set_operator_reward_limit(operator(), 100_u128 * ONE_STRK);
    stop_cheat_caller_address(stack.treasury_address);

    start_cheat_caller_address(stack.treasury_address, operator());
    let next_vault = stack.treasury.credit_to_vault(player(), 75_u128 * ONE_STRK);
    stop_cheat_caller_address(stack.treasury_address);

    assert(next_vault == 75_u128 * ONE_STRK, 'VAULT_CREDITED');
    assert(stack.vault.vault_balance_of(player()) == 75_u128 * ONE_STRK, 'PLAYER_VAULT');
    assert(stack.treasury.total_claimed() == 75_u128 * ONE_STRK, 'CLAIMED_TRACKED');
    assert(stack.treasury.reward_budget_cap() == 100_u128 * ONE_STRK, 'BUDGET_TRACKED');
    assert(
        stack.treasury.operator_reward_limit(operator()) == 100_u128 * ONE_STRK, 'LIMIT_TRACKED',
    );
    assert(stack.treasury.operator_claimed(operator()) == 75_u128 * ONE_STRK, 'OP_CLAIMED');
    assert(
        stack.treasury.available_rewards() == FUNDING_AMOUNT - (75_u128 * ONE_STRK),
        'REMAINING_REWARDS',
    );
}

#[test]
#[should_panic(expected: ('REWARD_BUDGET_UNSET',))]
fn test_rewards_treasury_requires_budget_cap_before_credit() {
    let stack = deploy_stack();

    start_cheat_caller_address(stack.treasury_address, owner());
    stack.treasury.fund(FUNDING_AMOUNT);
    stack.treasury.set_operator(operator(), true);
    stack.treasury.set_operator_reward_limit(operator(), 100_u128 * ONE_STRK);
    stop_cheat_caller_address(stack.treasury_address);

    start_cheat_caller_address(stack.treasury_address, operator());
    stack.treasury.credit_to_vault(player(), ONE_STRK);
    stop_cheat_caller_address(stack.treasury_address);
}

#[test]
#[should_panic(expected: ('REWARD_BUDGET_EXCEEDED',))]
fn test_rewards_treasury_enforces_global_budget_cap() {
    let stack = deploy_stack();

    start_cheat_caller_address(stack.treasury_address, owner());
    stack.treasury.fund(FUNDING_AMOUNT);
    stack.treasury.set_operator(operator(), true);
    stack.treasury.set_reward_budget_cap(10_u128 * ONE_STRK);
    stack.treasury.set_operator_reward_limit(operator(), 100_u128 * ONE_STRK);
    stop_cheat_caller_address(stack.treasury_address);

    start_cheat_caller_address(stack.treasury_address, operator());
    stack.treasury.credit_to_vault(player(), 11_u128 * ONE_STRK);
    stop_cheat_caller_address(stack.treasury_address);
}

#[test]
#[should_panic(expected: ('OP_LIMIT_EXCEEDED',))]
fn test_rewards_treasury_enforces_operator_limit() {
    let stack = deploy_stack();

    start_cheat_caller_address(stack.treasury_address, owner());
    stack.treasury.fund(FUNDING_AMOUNT);
    stack.treasury.set_operator(operator(), true);
    stack.treasury.set_reward_budget_cap(100_u128 * ONE_STRK);
    stack.treasury.set_operator_reward_limit(operator(), 10_u128 * ONE_STRK);
    stop_cheat_caller_address(stack.treasury_address);

    start_cheat_caller_address(stack.treasury_address, operator());
    stack.treasury.credit_to_vault(player(), 11_u128 * ONE_STRK);
    stop_cheat_caller_address(stack.treasury_address);
}
