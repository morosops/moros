#[starknet::contract]
pub mod BankrollVault {
    use starknet::storage::{
        Map, StorageMapReadAccess, StorageMapWriteAccess, StoragePointerReadAccess,
        StoragePointerWriteAccess,
    };
    use starknet::{ContractAddress, get_block_timestamp, get_caller_address, get_contract_address};
    use crate::interfaces::{IBankrollVault, IERC20Dispatcher, IERC20DispatcherTrait};
    use crate::types::HouseWithdrawalRequest;

    const HOUSE_WITHDRAW_DELAY_SECONDS: u64 = 172_800_u64;

    #[event]
    #[derive(Drop, starknet::Event)]
    pub enum Event {
        OperatorUpdated: OperatorUpdated,
        HouseLiquidityDeposited: HouseLiquidityDeposited,
        RewardsTreasuryFunded: RewardsTreasuryFunded,
        HouseWithdrawalQueued: HouseWithdrawalQueued,
        HouseWithdrawalCancelled: HouseWithdrawalCancelled,
        HouseLiquidityWithdrawn: HouseLiquidityWithdrawn,
        PublicDeposited: PublicDeposited,
        VaultDeposited: VaultDeposited,
        BalanceMoved: BalanceMoved,
        HandReserved: HandReserved,
        HandExposureLocked: HandExposureLocked,
        HandSettled: HandSettled,
        HandVoided: HandVoided,
        PublicWithdrawn: PublicWithdrawn,
    }

    #[derive(Drop, starknet::Event)]
    pub struct OperatorUpdated {
        pub operator: ContractAddress,
        pub active: bool,
    }

    #[derive(Drop, starknet::Event)]
    pub struct HouseLiquidityDeposited {
        pub provider: ContractAddress,
        pub amount: u128,
        pub house_available: u128,
    }

    #[derive(Drop, starknet::Event)]
    pub struct RewardsTreasuryFunded {
        pub recipient: ContractAddress,
        pub amount: u128,
        pub house_available: u128,
    }

    #[derive(Drop, starknet::Event)]
    pub struct HouseLiquidityWithdrawn {
        pub recipient: ContractAddress,
        pub amount: u128,
        pub house_available: u128,
    }

    #[derive(Drop, starknet::Event)]
    pub struct HouseWithdrawalQueued {
        pub request_id: u64,
        pub recipient: ContractAddress,
        pub amount: u128,
        pub execute_after: u64,
        pub house_available: u128,
    }

    #[derive(Drop, starknet::Event)]
    pub struct HouseWithdrawalCancelled {
        pub request_id: u64,
        pub recipient: ContractAddress,
        pub amount: u128,
        pub house_available: u128,
    }

    #[derive(Drop, starknet::Event)]
    pub struct PublicDeposited {
        pub player: ContractAddress,
        pub amount: u128,
    }

    #[derive(Drop, starknet::Event)]
    pub struct VaultDeposited {
        pub player: ContractAddress,
        pub amount: u128,
    }

    #[derive(Drop, starknet::Event)]
    pub struct BalanceMoved {
        pub player: ContractAddress,
        pub amount: u128,
        pub to_vault: bool,
        pub gambling_balance: u128,
        pub vault_balance: u128,
    }

    #[derive(Drop, starknet::Event)]
    pub struct HandReserved {
        pub player: ContractAddress,
        pub hand_id: u64,
        pub amount: u128,
    }

    #[derive(Drop, starknet::Event)]
    pub struct HandExposureLocked {
        pub hand_id: u64,
        pub amount: u128,
        pub total_locked: u128,
    }

    #[derive(Drop, starknet::Event)]
    pub struct HandSettled {
        pub player: ContractAddress,
        pub hand_id: u64,
        pub payout: u128,
    }

    #[derive(Drop, starknet::Event)]
    pub struct HandVoided {
        pub player: ContractAddress,
        pub hand_id: u64,
        pub refunded: u128,
    }

    #[derive(Drop, starknet::Event)]
    pub struct PublicWithdrawn {
        pub player: ContractAddress,
        pub amount: u128,
    }

    #[storage]
    struct Storage {
        owner: ContractAddress,
        asset: ContractAddress,
        operators: Map<ContractAddress, bool>,
        house_available: u128,
        house_locked: u128,
        next_house_withdrawal_id: u64,
        house_withdrawals: Map<u64, HouseWithdrawalRequest>,
        house_exposures: Map<u64, u128>,
        gambling_balances: Map<ContractAddress, u128>,
        vault_balances: Map<ContractAddress, u128>,
        reserved_balances: Map<(ContractAddress, u64), u128>,
        reservation_operators: Map<u64, ContractAddress>,
        reservation_players: Map<u64, ContractAddress>,
        exposure_operators: Map<u64, ContractAddress>,
        total_gambling_balances: u128,
        total_vault_balances: u128,
        total_reserved_balances: u128,
    }

    #[constructor]
    fn constructor(ref self: ContractState, owner: ContractAddress, asset: ContractAddress) {
        self.owner.write(owner);
        self.asset.write(asset);
        self.next_house_withdrawal_id.write(1_u64);
    }

    #[abi(embed_v0)]
    impl BankrollVaultImpl of IBankrollVault<ContractState> {
        fn asset(self: @ContractState) -> ContractAddress {
            self.asset.read()
        }

        fn set_operator(ref self: ContractState, operator: ContractAddress, active: bool) {
            self.assert_owner();
            self.operators.write(operator, active);
            self.emit(OperatorUpdated { operator, active });
        }

        fn deposit_house_liquidity(ref self: ContractState, amount: u128) -> u128 {
            assert(amount > 0_u128, 'HOUSE_DEPOSIT_ZERO');
            self.pull_token(get_caller_address(), amount);
            let next = self.house_available.read() + amount;
            self.house_available.write(next);
            self.assert_fully_backed();
            self
                .emit(
                    HouseLiquidityDeposited {
                        provider: get_caller_address(), amount, house_available: next,
                    },
                );
            next
        }

        fn fund_rewards_treasury(
            ref self: ContractState, recipient: ContractAddress, amount: u128,
        ) -> u128 {
            self.assert_owner();
            assert(amount > 0_u128, 'REWARD_FUND_ZERO');
            let available = self.house_available.read();
            assert(available >= amount, 'HOUSE_LIQUIDITY_LOW');
            let next_available = available - amount;
            self.house_available.write(next_available);
            self.push_token(recipient, amount);
            self.assert_fully_backed();
            self.emit(RewardsTreasuryFunded { recipient, amount, house_available: next_available });
            next_available
        }

        fn withdraw_house_liquidity(
            ref self: ContractState, recipient: ContractAddress, amount: u128,
        ) -> u128 {
            self.assert_owner();
            assert(amount > 0_u128, 'HOUSE_WITHDRAW_ZERO');
            let available = self.house_available.read();
            assert(available >= amount, 'HOUSE_LIQUIDITY_LOW');
            let request_id = self.next_house_withdrawal_id.read();
            self.next_house_withdrawal_id.write(request_id + 1_u64);
            let next_available = available - amount;
            let execute_after = get_block_timestamp() + HOUSE_WITHDRAW_DELAY_SECONDS;
            self.house_available.write(next_available);
            self
                .house_withdrawals
                .write(
                    request_id,
                    HouseWithdrawalRequest {
                        request_id, recipient, amount, execute_after, active: true,
                    },
                );
            self
                .emit(
                    HouseWithdrawalQueued {
                        request_id,
                        recipient,
                        amount,
                        execute_after,
                        house_available: next_available,
                    },
                );
            request_id.into()
        }

        fn cancel_house_withdrawal(ref self: ContractState, request_id: u64) -> u128 {
            self.assert_owner();
            let mut request = self.load_house_withdrawal(request_id);
            assert(request.active, 'HOUSE_WITHDRAW_INACTIVE');
            request.active = false;
            self.house_withdrawals.write(request_id, request);
            let next_available = self.house_available.read() + request.amount;
            self.house_available.write(next_available);
            self
                .emit(
                    HouseWithdrawalCancelled {
                        request_id,
                        recipient: request.recipient,
                        amount: request.amount,
                        house_available: next_available,
                    },
                );
            next_available
        }

        fn execute_house_withdrawal(ref self: ContractState, request_id: u64) -> u128 {
            self.assert_owner();
            let mut request = self.load_house_withdrawal(request_id);
            assert(request.active, 'HOUSE_WITHDRAW_INACTIVE');
            assert(get_block_timestamp() >= request.execute_after, 'HOUSE_WITHDRAW_TIMELOCK');
            request.active = false;
            self.house_withdrawals.write(request_id, request);
            self.push_token(request.recipient, request.amount);
            self.assert_fully_backed();
            let house_available = self.house_available.read();
            self
                .emit(
                    HouseLiquidityWithdrawn {
                        recipient: request.recipient, amount: request.amount, house_available,
                    },
                );
            house_available
        }

        fn deposit_public(
            ref self: ContractState, recipient: ContractAddress, amount: u128,
        ) -> u128 {
            self.pull_token(get_caller_address(), amount);
            let next = self.gambling_balances.read(recipient) + amount;
            self.gambling_balances.write(recipient, next);
            self.total_gambling_balances.write(self.total_gambling_balances.read() + amount);
            self.assert_fully_backed();
            self.emit(PublicDeposited { player: recipient, amount });
            next
        }

        fn deposit_to_vault(
            ref self: ContractState, recipient: ContractAddress, amount: u128,
        ) -> u128 {
            self.pull_token(get_caller_address(), amount);
            let next = self.vault_balances.read(recipient) + amount;
            self.vault_balances.write(recipient, next);
            self.total_vault_balances.write(self.total_vault_balances.read() + amount);
            self.assert_fully_backed();
            self.emit(VaultDeposited { player: recipient, amount });
            next
        }

        fn move_to_vault(ref self: ContractState, amount: u128) -> (u128, u128) {
            let player = get_caller_address();
            let gambling = self.gambling_balances.read(player);
            assert(gambling >= amount, 'INSUFFICIENT_BAL');
            let next_gambling = gambling - amount;
            let next_vault = self.vault_balances.read(player) + amount;
            self.gambling_balances.write(player, next_gambling);
            self.vault_balances.write(player, next_vault);
            self.total_gambling_balances.write(self.total_gambling_balances.read() - amount);
            self.total_vault_balances.write(self.total_vault_balances.read() + amount);
            self.assert_fully_backed();
            self
                .emit(
                    BalanceMoved {
                        player,
                        amount,
                        to_vault: true,
                        gambling_balance: next_gambling,
                        vault_balance: next_vault,
                    },
                );
            (next_gambling, next_vault)
        }

        fn move_to_gambling(ref self: ContractState, amount: u128) -> (u128, u128) {
            let player = get_caller_address();
            let vault_balance = self.vault_balances.read(player);
            assert(vault_balance >= amount, 'INSUFFICIENT_VAULT');
            let next_vault = vault_balance - amount;
            let next_gambling = self.gambling_balances.read(player) + amount;
            self.vault_balances.write(player, next_vault);
            self.gambling_balances.write(player, next_gambling);
            self.total_vault_balances.write(self.total_vault_balances.read() - amount);
            self.total_gambling_balances.write(self.total_gambling_balances.read() + amount);
            self.assert_fully_backed();
            self
                .emit(
                    BalanceMoved {
                        player,
                        amount,
                        to_vault: false,
                        gambling_balance: next_gambling,
                        vault_balance: next_vault,
                    },
                );
            (next_gambling, next_vault)
        }

        fn reserve_for_hand(
            ref self: ContractState, player: ContractAddress, hand_id: u64, amount: u128,
        ) {
            self.assert_operator();
            assert(amount > 0_u128, 'RESERVE_ZERO');
            let operator = get_caller_address();
            self.assert_or_bind_reservation_owner(hand_id, operator, player);
            let balance = self.gambling_balances.read(player);
            assert(balance >= amount, 'INSUFFICIENT_BAL');
            let reserved = self.reserved_balances.read((player, hand_id));
            self.gambling_balances.write(player, balance - amount);
            self.reserved_balances.write((player, hand_id), reserved + amount);
            self.total_gambling_balances.write(self.total_gambling_balances.read() - amount);
            self.total_reserved_balances.write(self.total_reserved_balances.read() + amount);
            self.assert_fully_backed();
            self.emit(HandReserved { player, hand_id, amount: reserved + amount });
        }

        fn lock_house_exposure(ref self: ContractState, hand_id: u64, amount: u128) {
            self.assert_operator();
            assert(amount > 0_u128, 'EXPOSURE_ZERO');
            let operator = get_caller_address();
            self.assert_or_bind_exposure_owner(hand_id, operator);
            let reservation_operator = self.reservation_operators.read(hand_id);
            if reservation_operator != self.zero_address() {
                assert(reservation_operator == operator, 'HAND_OPERATOR_MISMATCH');
            }
            let available = self.house_available.read();
            assert(available >= amount, 'HOUSE_LIQUIDITY_LOW');
            let next_available = available - amount;
            let next_locked = self.house_locked.read() + amount;
            self.house_available.write(next_available);
            self.house_locked.write(next_locked);
            let hand_exposure = self.house_exposures.read(hand_id) + amount;
            self.house_exposures.write(hand_id, hand_exposure);
            self.assert_fully_backed();
            self.emit(HandExposureLocked { hand_id, amount, total_locked: hand_exposure });
        }

        fn settle_hand(
            ref self: ContractState, player: ContractAddress, hand_id: u64, payout: u128,
        ) -> u128 {
            self.assert_operator();
            let operator = get_caller_address();
            self.assert_bound_reservation_owner(hand_id, operator, player);
            self.assert_bound_exposure_owner(hand_id, operator);
            let reserved = self.reserved_balances.read((player, hand_id));
            assert(reserved > 0, 'HAND_NOT_RESERVED');
            let exposure = self.house_exposures.read(hand_id);
            assert(exposure > 0, 'HOUSE_EXPOSURE_MISSING');
            self.reserved_balances.write((player, hand_id), 0);
            self.house_exposures.write(hand_id, 0);
            self.reservation_operators.write(hand_id, 0.try_into().unwrap());
            self.reservation_players.write(hand_id, 0.try_into().unwrap());
            self.exposure_operators.write(hand_id, 0.try_into().unwrap());
            self.house_locked.write(self.house_locked.read() - exposure);
            self.total_reserved_balances.write(self.total_reserved_balances.read() - reserved);

            let mut available = self.house_available.read() + exposure;
            if reserved >= payout {
                available += reserved - payout;
            } else {
                let required_from_house = payout - reserved;
                assert(exposure >= required_from_house, 'HOUSE_EXPOSURE_LOW');
                available -= required_from_house;
            }
            self.house_available.write(available);

            let next = self.gambling_balances.read(player) + payout;
            self.gambling_balances.write(player, next);
            self.total_gambling_balances.write(self.total_gambling_balances.read() + payout);
            self.assert_fully_backed();
            self.emit(HandSettled { player, hand_id, payout });
            next
        }

        fn void_hand(ref self: ContractState, player: ContractAddress, hand_id: u64) -> u128 {
            self.assert_operator();
            let operator = get_caller_address();
            self.assert_bound_reservation_owner(hand_id, operator, player);
            let reserved = self.reserved_balances.read((player, hand_id));
            assert(reserved > 0, 'HAND_NOT_RESERVED');
            let exposure = self.house_exposures.read(hand_id);
            if exposure > 0 {
                self.assert_bound_exposure_owner(hand_id, operator);
            }

            self.reserved_balances.write((player, hand_id), 0);
            self.house_exposures.write(hand_id, 0);
            self.reservation_operators.write(hand_id, 0.try_into().unwrap());
            self.reservation_players.write(hand_id, 0.try_into().unwrap());
            self.exposure_operators.write(hand_id, 0.try_into().unwrap());
            self.gambling_balances.write(player, self.gambling_balances.read(player) + reserved);
            self.total_reserved_balances.write(self.total_reserved_balances.read() - reserved);
            self.total_gambling_balances.write(self.total_gambling_balances.read() + reserved);
            if exposure > 0 {
                self.house_locked.write(self.house_locked.read() - exposure);
                self.house_available.write(self.house_available.read() + exposure);
            }
            let next = self.gambling_balances.read(player);
            self.assert_fully_backed();
            self.emit(HandVoided { player, hand_id, refunded: reserved });
            next
        }

        fn withdraw_public(
            ref self: ContractState, recipient: ContractAddress, amount: u128,
        ) -> u128 {
            let player = get_caller_address();
            let balance = self.gambling_balances.read(player);
            assert(balance >= amount, 'INSUFFICIENT_BAL');
            let next = balance - amount;
            self.gambling_balances.write(player, next);
            self.total_gambling_balances.write(self.total_gambling_balances.read() - amount);
            self.push_token(recipient, amount);
            self.assert_fully_backed();
            self.emit(PublicWithdrawn { player, amount });
            next
        }

        fn withdraw_from_vault(
            ref self: ContractState, recipient: ContractAddress, amount: u128,
        ) -> u128 {
            let player = get_caller_address();
            let balance = self.vault_balances.read(player);
            assert(balance >= amount, 'INSUFFICIENT_VAULT');
            let next = balance - amount;
            self.vault_balances.write(player, next);
            self.total_vault_balances.write(self.total_vault_balances.read() - amount);
            self.push_token(recipient, amount);
            self.assert_fully_backed();
            self.emit(PublicWithdrawn { player, amount });
            next
        }

        fn operator_withdraw_public(
            ref self: ContractState,
            player: ContractAddress,
            recipient: ContractAddress,
            amount: u128,
        ) -> u128 {
            let _ = player;
            let _ = recipient;
            let _ = amount;
            assert(false, 'USER_WITHDRAW_ONLY');
            0
        }

        fn balance_of(self: @ContractState, player: ContractAddress) -> u128 {
            self.gambling_balances.read(player)
        }

        fn gambling_balance_of(self: @ContractState, player: ContractAddress) -> u128 {
            self.gambling_balances.read(player)
        }

        fn vault_balance_of(self: @ContractState, player: ContractAddress) -> u128 {
            self.vault_balances.read(player)
        }

        fn reserved_of(self: @ContractState, player: ContractAddress, hand_id: u64) -> u128 {
            self.reserved_balances.read((player, hand_id))
        }

        fn total_player_liabilities(self: @ContractState) -> u128 {
            self.total_gambling_balances.read()
                + self.total_vault_balances.read()
                + self.total_reserved_balances.read()
        }

        fn house_available(self: @ContractState) -> u128 {
            self.house_available.read()
        }

        fn house_locked(self: @ContractState) -> u128 {
            self.house_locked.read()
        }

        fn hand_exposure_of(self: @ContractState, hand_id: u64) -> u128 {
            self.house_exposures.read(hand_id)
        }

        fn house_withdraw_delay_seconds(self: @ContractState) -> u64 {
            let _ = self;
            HOUSE_WITHDRAW_DELAY_SECONDS
        }

        fn house_withdrawal(self: @ContractState, request_id: u64) -> HouseWithdrawalRequest {
            self.load_house_withdrawal(request_id)
        }
    }

    #[generate_trait]
    impl InternalImpl of InternalTrait {
        fn assert_owner(self: @ContractState) {
            assert(get_caller_address() == self.owner.read(), 'OWNER_ONLY');
        }

        fn assert_operator(self: @ContractState) {
            let caller = get_caller_address();
            assert(caller == self.owner.read() || self.is_operator(caller), 'OPERATOR_ONLY');
        }

        fn is_operator(self: @ContractState, operator: ContractAddress) -> bool {
            self.operators.read(operator)
        }

        fn zero_address(self: @ContractState) -> ContractAddress {
            let _ = self;
            0.try_into().unwrap()
        }

        fn assert_or_bind_reservation_owner(
            ref self: ContractState,
            hand_id: u64,
            operator: ContractAddress,
            player: ContractAddress,
        ) {
            let existing_operator = self.reservation_operators.read(hand_id);
            if existing_operator == self.zero_address() {
                self.reservation_operators.write(hand_id, operator);
                self.reservation_players.write(hand_id, player);
            } else {
                assert(existing_operator == operator, 'HAND_OPERATOR_MISMATCH');
                assert(self.reservation_players.read(hand_id) == player, 'HAND_PLAYER_MISMATCH');
            }
        }

        fn assert_bound_reservation_owner(
            self: @ContractState,
            hand_id: u64,
            operator: ContractAddress,
            player: ContractAddress,
        ) {
            let existing_operator = self.reservation_operators.read(hand_id);
            assert(existing_operator != self.zero_address(), 'HAND_OWNER_MISSING');
            assert(existing_operator == operator, 'HAND_OPERATOR_MISMATCH');
            assert(self.reservation_players.read(hand_id) == player, 'HAND_PLAYER_MISMATCH');
        }

        fn assert_or_bind_exposure_owner(
            ref self: ContractState, hand_id: u64, operator: ContractAddress,
        ) {
            let existing_operator = self.exposure_operators.read(hand_id);
            if existing_operator == self.zero_address() {
                self.exposure_operators.write(hand_id, operator);
            } else {
                assert(existing_operator == operator, 'EXPOSURE_OPERATOR_MISMATCH');
            }
        }

        fn assert_bound_exposure_owner(
            self: @ContractState, hand_id: u64, operator: ContractAddress,
        ) {
            let existing_operator = self.exposure_operators.read(hand_id);
            assert(existing_operator != self.zero_address(), 'EXPOSURE_OWNER_MISSING');
            assert(existing_operator == operator, 'EXPOSURE_OPERATOR_MISMATCH');
        }

        fn load_house_withdrawal(self: @ContractState, request_id: u64) -> HouseWithdrawalRequest {
            let request = self.house_withdrawals.read(request_id);
            assert(request.request_id == request_id, 'HOUSE_WITHDRAW_NOT_FOUND');
            request
        }

        fn pull_token(ref self: ContractState, sender: ContractAddress, amount: u128) {
            let ok = IERC20Dispatcher { contract_address: self.asset.read() }
                .transfer_from(sender, get_contract_address(), amount.into());
            assert(ok, 'TOKEN_PULL_FAILED');
        }

        fn push_token(ref self: ContractState, recipient: ContractAddress, amount: u128) {
            let ok = IERC20Dispatcher { contract_address: self.asset.read() }
                .transfer(recipient, amount.into());
            assert(ok, 'TOKEN_PUSH_FAILED');
        }

        fn assert_fully_backed(self: @ContractState) {
            let token_balance = IERC20Dispatcher { contract_address: self.asset.read() }
                .balance_of(get_contract_address())
                .low;
            let liabilities = self.total_player_liabilities();
            let house_liquidity = self.house_available.read() + self.house_locked.read();
            assert(token_balance >= liabilities + house_liquidity, 'VAULT_UNBACKED');
        }
    }
}
