#[starknet::contract]
pub mod RewardsTreasury {
    use starknet::storage::{
        Map, StorageMapReadAccess, StorageMapWriteAccess, StoragePointerReadAccess,
        StoragePointerWriteAccess,
    };
    use starknet::{ContractAddress, get_caller_address, get_contract_address};
    use crate::interfaces::{
        IBankrollVaultDispatcher, IBankrollVaultDispatcherTrait, IERC20Dispatcher,
        IERC20DispatcherTrait, IRewardsTreasury,
    };

    #[event]
    #[derive(Drop, starknet::Event)]
    pub enum Event {
        OperatorUpdated: OperatorUpdated,
        RewardBudgetCapUpdated: RewardBudgetCapUpdated,
        OperatorRewardLimitUpdated: OperatorRewardLimitUpdated,
        TreasuryFunded: TreasuryFunded,
        RewardsCreditedToVault: RewardsCreditedToVault,
    }

    #[derive(Drop, starknet::Event)]
    pub struct OperatorUpdated {
        pub operator: ContractAddress,
        pub active: bool,
    }

    #[derive(Drop, starknet::Event)]
    pub struct RewardBudgetCapUpdated {
        pub cap: u128,
    }

    #[derive(Drop, starknet::Event)]
    pub struct OperatorRewardLimitUpdated {
        pub operator: ContractAddress,
        pub limit: u128,
        pub claimed: u128,
    }

    #[derive(Drop, starknet::Event)]
    pub struct TreasuryFunded {
        pub funder: ContractAddress,
        pub amount: u128,
        pub total_funded: u128,
    }

    #[derive(Drop, starknet::Event)]
    pub struct RewardsCreditedToVault {
        pub operator: ContractAddress,
        pub player: ContractAddress,
        pub amount: u128,
        pub total_claimed: u128,
    }

    #[storage]
    struct Storage {
        owner: ContractAddress,
        asset: ContractAddress,
        bankroll_vault: ContractAddress,
        operators: Map<ContractAddress, bool>,
        reward_budget_cap: u128,
        operator_limits: Map<ContractAddress, u128>,
        operator_claimed: Map<ContractAddress, u128>,
        total_funded: u128,
        total_claimed: u128,
    }

    #[constructor]
    fn constructor(
        ref self: ContractState,
        owner: ContractAddress,
        asset: ContractAddress,
        bankroll_vault: ContractAddress,
    ) {
        self.owner.write(owner);
        self.asset.write(asset);
        self.bankroll_vault.write(bankroll_vault);
    }

    #[abi(embed_v0)]
    impl RewardsTreasuryImpl of IRewardsTreasury<ContractState> {
        fn asset(self: @ContractState) -> ContractAddress {
            self.asset.read()
        }

        fn bankroll_vault(self: @ContractState) -> ContractAddress {
            self.bankroll_vault.read()
        }

        fn set_operator(ref self: ContractState, operator: ContractAddress, active: bool) {
            self.assert_owner();
            self.operators.write(operator, active);
            self.emit(OperatorUpdated { operator, active });
        }

        fn set_reward_budget_cap(ref self: ContractState, cap: u128) {
            self.assert_owner();
            assert(cap >= self.total_claimed.read(), 'REWARD_BUDGET_LOW');
            self.reward_budget_cap.write(cap);
            self.emit(RewardBudgetCapUpdated { cap });
        }

        fn set_operator_reward_limit(
            ref self: ContractState, operator: ContractAddress, limit: u128,
        ) {
            self.assert_owner();
            let claimed = self.operator_claimed.read(operator);
            assert(limit >= claimed, 'OP_LIMIT_LOW');
            self.operator_limits.write(operator, limit);
            self.emit(OperatorRewardLimitUpdated { operator, limit, claimed });
        }

        fn fund(ref self: ContractState, amount: u128) -> u128 {
            assert(amount > 0_u128, 'REWARD_FUND_ZERO');
            self.pull_token(get_caller_address(), amount);
            let total_funded = self.total_funded.read() + amount;
            self.total_funded.write(total_funded);
            self.emit(TreasuryFunded { funder: get_caller_address(), amount, total_funded });
            total_funded
        }

        fn credit_to_vault(ref self: ContractState, player: ContractAddress, amount: u128) -> u128 {
            self.assert_operator();
            assert(amount > 0_u128, 'REWARD_CREDIT_ZERO');
            let operator = get_caller_address();
            let next_total_claimed = self.total_claimed.read() + amount;
            let budget_cap = self.reward_budget_cap.read();
            assert(budget_cap > 0_u128, 'REWARD_BUDGET_UNSET');
            assert(next_total_claimed <= budget_cap, 'REWARD_BUDGET_EXCEEDED');
            let operator_limit = self.operator_limits.read(operator);
            assert(operator_limit > 0_u128, 'OP_LIMIT_UNSET');
            let next_operator_claimed = self.operator_claimed.read(operator) + amount;
            assert(next_operator_claimed <= operator_limit, 'OP_LIMIT_EXCEEDED');
            let available = self.available_rewards();
            assert(available >= amount, 'REWARD_TREASURY_LOW');
            let token = IERC20Dispatcher { contract_address: self.asset.read() };
            let approved = token.approve(self.bankroll_vault.read(), amount.into());
            assert(approved, 'REWARD_APPROVE_FAILED');
            let next = IBankrollVaultDispatcher { contract_address: self.bankroll_vault.read() }
                .deposit_to_vault(player, amount);
            self.total_claimed.write(next_total_claimed);
            self.operator_claimed.write(operator, next_operator_claimed);
            self
                .emit(
                    RewardsCreditedToVault {
                        operator, player, amount, total_claimed: next_total_claimed,
                    },
                );
            next
        }

        fn available_rewards(self: @ContractState) -> u128 {
            IERC20Dispatcher { contract_address: self.asset.read() }
                .balance_of(get_contract_address())
                .low
        }

        fn total_funded(self: @ContractState) -> u128 {
            self.total_funded.read()
        }

        fn total_claimed(self: @ContractState) -> u128 {
            self.total_claimed.read()
        }

        fn reward_budget_cap(self: @ContractState) -> u128 {
            self.reward_budget_cap.read()
        }

        fn operator_reward_limit(self: @ContractState, operator: ContractAddress) -> u128 {
            self.operator_limits.read(operator)
        }

        fn operator_claimed(self: @ContractState, operator: ContractAddress) -> u128 {
            self.operator_claimed.read(operator)
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

        fn pull_token(ref self: ContractState, sender: ContractAddress, amount: u128) {
            let ok = IERC20Dispatcher { contract_address: self.asset.read() }
                .transfer_from(sender, get_contract_address(), amount.into());
            assert(ok, 'TOKEN_PULL_FAILED');
        }
    }
}
