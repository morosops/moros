#[starknet::contract]
pub mod TableRegistry {
    use starknet::storage::{
        Map, StorageMapReadAccess, StorageMapWriteAccess, StoragePointerReadAccess,
        StoragePointerWriteAccess,
    };
    use starknet::{ContractAddress, get_caller_address};
    use crate::interfaces::ITableRegistry;
    use crate::types::{GameKind, TableConfig, TableStatus};

    #[event]
    #[derive(Drop, starknet::Event)]
    pub enum Event {
        TableRegistered: TableRegistered,
        TableStatusUpdated: TableStatusUpdated,
        TableLimitsUpdated: TableLimitsUpdated,
    }

    #[derive(Drop, starknet::Event)]
    pub struct TableRegistered {
        pub table_id: u64,
        pub game_kind: GameKind,
        pub table_contract: ContractAddress,
        pub min_wager: u128,
        pub max_wager: u128,
    }

    #[derive(Drop, starknet::Event)]
    pub struct TableStatusUpdated {
        pub table_id: u64,
        pub status: TableStatus,
    }

    #[derive(Drop, starknet::Event)]
    pub struct TableLimitsUpdated {
        pub table_id: u64,
        pub min_wager: u128,
        pub max_wager: u128,
    }

    #[storage]
    struct Storage {
        owner: ContractAddress,
        tables: Map<u64, TableConfig>,
    }

    #[constructor]
    fn constructor(ref self: ContractState, owner: ContractAddress) {
        self.owner.write(owner);
    }

    #[abi(embed_v0)]
    impl TableRegistryImpl of ITableRegistry<ContractState> {
        fn register_table(
            ref self: ContractState,
            table_id: u64,
            table_contract: ContractAddress,
            game_kind: GameKind,
            min_wager: u128,
            max_wager: u128,
        ) {
            assert(get_caller_address() == self.owner.read(), 'OWNER_ONLY');
            assert(max_wager >= min_wager, 'WAGER_RANGE');
            self
                .tables
                .write(
                    table_id,
                    TableConfig {
                        game_kind,
                        table_contract,
                        min_wager,
                        max_wager,
                        status: TableStatus::Active,
                    },
                );
            self
                .emit(
                    TableRegistered { table_id, game_kind, table_contract, min_wager, max_wager },
                );
        }

        fn set_table_status(ref self: ContractState, table_id: u64, status: TableStatus) {
            assert(get_caller_address() == self.owner.read(), 'OWNER_ONLY');
            let mut config = self.tables.read(table_id);
            config.status = status;
            self.tables.write(table_id, config);
            self.emit(TableStatusUpdated { table_id, status });
        }

        fn set_table_limits(
            ref self: ContractState, table_id: u64, min_wager: u128, max_wager: u128,
        ) {
            assert(get_caller_address() == self.owner.read(), 'OWNER_ONLY');
            assert(max_wager >= min_wager, 'WAGER_RANGE');
            let mut config = self.tables.read(table_id);
            config.min_wager = min_wager;
            config.max_wager = max_wager;
            self.tables.write(table_id, config);
            self.emit(TableLimitsUpdated { table_id, min_wager, max_wager });
        }

        fn get_table(self: @ContractState, table_id: u64) -> TableConfig {
            self.tables.read(table_id)
        }
    }
}
