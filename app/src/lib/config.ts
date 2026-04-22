function parseTableId(value: string | undefined, fallback: number) {
  const parsed = Number.parseInt(value ?? '', 10)
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback
}

function isLoopbackHostname(hostname: string) {
  return (
    hostname === 'localhost' ||
    hostname === '127.0.0.1' ||
    hostname === '0.0.0.0' ||
    hostname === '::1'
  )
}

function isIpv4Hostname(hostname: string) {
  return /^\d{1,3}(?:\.\d{1,3}){3}$/.test(hostname)
}

export function resolveMorosServiceUrl(value: string, browserFallbackPath?: string) {
  return resolveMorosServiceUrlForOrigin(
    value,
    typeof window === 'undefined' ? undefined : window.location.origin,
    browserFallbackPath,
  )
}

export function resolveMorosServiceUrlForOrigin(
  value: string,
  currentOrigin?: string,
  browserFallbackPath?: string,
) {
  if (typeof window === 'undefined') {
    if (!currentOrigin) {
      return value
    }
  }

  try {
    const origin = currentOrigin ?? window.location.origin
    const browserHost = new URL(origin).hostname
    const resolved = new URL(value, origin)

    if (
      browserFallbackPath &&
      !isLoopbackHostname(browserHost) &&
      (isLoopbackHostname(resolved.hostname) || isIpv4Hostname(resolved.hostname))
    ) {
      return new URL(browserFallbackPath, origin).toString()
    }

    return resolved.toString()
  } catch {
    return value
  }
}

export function resolveMorosServicePath(root: string, pathname: string) {
  try {
    const normalizedPathname = pathname.startsWith('/') ? pathname.slice(1) : pathname
    const normalizedRoot = resolveMorosServiceUrl(root)
    const base = normalizedRoot.endsWith('/') ? normalizedRoot : `${normalizedRoot}/`
    return new URL(normalizedPathname, base).toString()
  } catch {
    return `${root}${pathname}`
  }
}

function resolveNetwork(value: string | undefined) {
  if (value === 'mainnet' || value === 'devnet') {
    return value
  }
  return 'sepolia'
}

const network = resolveNetwork(import.meta.env.VITE_MOROS_NETWORK)
const defaultGameplaySessionMaxWagerWei = '100000000000000000000'
const defaultPrivyBridgeUrl = '/auth'
const defaultCoordinatorUrl = '/coordinator'
const defaultRelayerUrl = '/relayer'
const defaultIndexerUrl = '/indexer'
const defaultDepositRouterUrl = '/deposit'

export const morosConfig = {
  network,
  ethereumRpcUrl: import.meta.env.VITE_MOROS_ETHEREUM_RPC_URL,
  privyAppId: import.meta.env.VITE_PRIVY_APP_ID,
  privyBridgeUrl: resolveMorosServiceUrl(
    import.meta.env.VITE_MOROS_PRIVY_BRIDGE_URL ?? defaultPrivyBridgeUrl,
    defaultPrivyBridgeUrl,
  ),
  paymasterUrl: resolveMorosServiceUrl(
    import.meta.env.VITE_MOROS_PAYMASTER_URL ??
      resolveMorosServicePath(
        import.meta.env.VITE_MOROS_PRIVY_BRIDGE_URL ?? defaultPrivyBridgeUrl,
        '/v1/paymaster',
      ),
    '/auth/v1/paymaster',
  ),
  depositRouterUrl: resolveMorosServiceUrl(
    import.meta.env.VITE_MOROS_DEPOSIT_ROUTER_URL ?? defaultDepositRouterUrl,
    defaultDepositRouterUrl,
  ),
  bankrollVault: import.meta.env.VITE_MOROS_BANKROLL_VAULT_ADDRESS,
  sessionRegistry: import.meta.env.VITE_MOROS_SESSION_REGISTRY_ADDRESS,
  gameplaySessionKey:
    import.meta.env.VITE_MOROS_GAMEPLAY_SESSION_KEY_ADDRESS ??
    import.meta.env.VITE_MOROS_OPERATOR_ADDRESS,
  gameplaySessionMaxWagerWei:
    import.meta.env.VITE_MOROS_GAMEPLAY_SESSION_MAX_WAGER_WEI ??
    defaultGameplaySessionMaxWagerWei,
  blackjackTableId: parseTableId(import.meta.env.VITE_MOROS_BLACKJACK_TABLE_ID, 2),
  diceTableId: parseTableId(import.meta.env.VITE_MOROS_DICE_TABLE_ID, 1),
  rouletteTableId: parseTableId(import.meta.env.VITE_MOROS_ROULETTE_TABLE_ID, 3),
  baccaratTableId: parseTableId(import.meta.env.VITE_MOROS_BACCARAT_TABLE_ID, 4),
  diceTable: import.meta.env.VITE_MOROS_DICE_TABLE_ADDRESS,
  rouletteTable: import.meta.env.VITE_MOROS_ROULETTE_TABLE_ADDRESS,
  baccaratTable: import.meta.env.VITE_MOROS_BACCARAT_TABLE_ADDRESS,
  strkToken:
    import.meta.env.VITE_MOROS_STRK_TOKEN_ADDRESS ??
    '0x04718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d',
  coordinatorUrl: resolveMorosServiceUrl(
    import.meta.env.VITE_MOROS_COORDINATOR_URL ?? defaultCoordinatorUrl,
    defaultCoordinatorUrl,
  ),
  relayerUrl: resolveMorosServiceUrl(
    import.meta.env.VITE_MOROS_RELAYER_URL ?? defaultRelayerUrl,
    defaultRelayerUrl,
  ),
  indexerUrl: resolveMorosServiceUrl(
    import.meta.env.VITE_MOROS_INDEXER_URL ?? defaultIndexerUrl,
    defaultIndexerUrl,
  ),
  privyEnabled: Boolean(import.meta.env.VITE_PRIVY_APP_ID),
} as const
