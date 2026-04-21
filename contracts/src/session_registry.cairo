#[starknet::contract]
pub mod SessionRegistry {
    use starknet::storage::{
        Map, StorageMapReadAccess, StorageMapWriteAccess, StoragePointerWriteAccess,
    };
    use starknet::{ContractAddress, get_caller_address};
    use crate::interfaces::ISessionRegistry;
    use crate::types::SessionGrant;

    #[event]
    #[derive(Drop, starknet::Event)]
    pub enum Event {
        SessionKeyRegistered: SessionKeyRegistered,
        SessionKeyRevoked: SessionKeyRevoked,
    }

    #[derive(Drop, starknet::Event)]
    pub struct SessionKeyRegistered {
        pub player: ContractAddress,
        pub session_key: ContractAddress,
        pub max_wager: u128,
        pub expires_at: u64,
    }

    #[derive(Drop, starknet::Event)]
    pub struct SessionKeyRevoked {
        pub player: ContractAddress,
        pub session_key: ContractAddress,
    }

    #[storage]
    struct Storage {
        owner: ContractAddress,
        sessions: Map<(ContractAddress, ContractAddress), SessionGrant>,
    }

    #[constructor]
    fn constructor(ref self: ContractState, owner: ContractAddress) {
        self.owner.write(owner);
    }

    #[abi(embed_v0)]
    impl SessionRegistryImpl of ISessionRegistry<ContractState> {
        fn register_session_key(
            ref self: ContractState,
            player: ContractAddress,
            session_key: ContractAddress,
            max_wager: u128,
            expires_at: u64,
        ) {
            let caller = get_caller_address();
            assert(caller == player, 'NOT_ALLOWED');
            self
                .sessions
                .write(
                    (player, session_key),
                    SessionGrant { player, session_key, max_wager, expires_at, active: true },
                );
            self.emit(SessionKeyRegistered { player, session_key, max_wager, expires_at });
        }

        fn revoke_session_key(
            ref self: ContractState, player: ContractAddress, session_key: ContractAddress,
        ) {
            let caller = get_caller_address();
            assert(caller == player, 'NOT_ALLOWED');
            let mut grant = self.sessions.read((player, session_key));
            grant.active = false;
            self.sessions.write((player, session_key), grant);
            self.emit(SessionKeyRevoked { player, session_key });
        }

        fn get_session(
            self: @ContractState, player: ContractAddress, session_key: ContractAddress,
        ) -> SessionGrant {
            self.sessions.read((player, session_key))
        }

        fn is_action_allowed(
            self: @ContractState,
            player: ContractAddress,
            session_key: ContractAddress,
            wager: u128,
            now_ts: u64,
        ) -> bool {
            let grant = self.sessions.read((player, session_key));
            grant.active && now_ts <= grant.expires_at && wager <= grant.max_wager
        }
    }
}
