/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_MOROS_NETWORK?: string
  readonly VITE_MOROS_BLACKJACK_TABLE_ID?: string
  readonly VITE_MOROS_DICE_TABLE_ID?: string
  readonly VITE_MOROS_ROULETTE_TABLE_ID?: string
  readonly VITE_MOROS_BACCARAT_TABLE_ID?: string
  readonly VITE_MOROS_BANKROLL_VAULT_ADDRESS?: string
  readonly VITE_MOROS_DICE_TABLE_ADDRESS?: string
  readonly VITE_MOROS_ROULETTE_TABLE_ADDRESS?: string
  readonly VITE_MOROS_BACCARAT_TABLE_ADDRESS?: string
  readonly VITE_MOROS_STRK_TOKEN_ADDRESS?: string
  readonly VITE_MOROS_COORDINATOR_URL?: string
  readonly VITE_MOROS_RELAYER_URL?: string
  readonly VITE_MOROS_INDEXER_URL?: string
  readonly VITE_MOROS_DEPOSIT_ROUTER_URL?: string
  readonly VITE_MOROS_SESSION_REGISTRY_ADDRESS?: string
  readonly VITE_MOROS_GAMEPLAY_SESSION_KEY_ADDRESS?: string
  readonly VITE_MOROS_GAMEPLAY_SESSION_MAX_WAGER_WEI?: string
  readonly VITE_MOROS_OPERATOR_ADDRESS?: string
  readonly VITE_PRIVY_APP_ID?: string
  readonly VITE_MOROS_PRIVY_BRIDGE_URL?: string
  readonly VITE_MOROS_PAYMASTER_URL?: string
}

interface ImportMeta {
  readonly env: ImportMetaEnv
}
