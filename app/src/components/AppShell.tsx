import {
  Suspense,
  lazy,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
} from 'react'
import { Link, NavLink, Outlet, useLocation } from 'react-router-dom'
import { MorosEmailState, useMorosAuthRuntime } from './MorosAuthProvider'
import {
  fetchAccountBalancesByWalletAddress,
  createMorosAccountResolveChallenge,
  fetchProfileClaimChallenge,
  fetchPlayerProfile,
  resolveMorosPrivyAccount,
  resolveMorosWalletAccount,
  upsertPlayerProfile,
} from '../lib/api'
import { formatStrk } from '../lib/format'
import { morosGames } from '../lib/game-config'
import { useMorosWallet } from '../hooks/useMorosWallet'
import { waitForPrivyRequestToken } from '../lib/privy-auth'
import { ensureMorosPrivyWallet } from '../lib/privy-bridge'
import { deriveMorosAccountState } from '../lib/account-state'
import type { ExternalMorosWalletOption } from '../lib/starkzap-types'
import { useAccountStore } from '../store/account'
import { useProfileStore } from '../store/profile'
import { useUiStore } from '../store/ui'
import { NavIcon, type NavIconName } from './NavIcon'

const utilityNav = [
  { navIcon: '/icons/ranking.svg', label: 'Leaderboard', to: '/leaderboard' },
] as const
const FUNDING_PROMPT_KEY_PREFIX = 'moros.funding.prompted.'

function shortAddress(address?: string) {
  if (!address) {
    return undefined
  }
  return `${address.slice(0, 6)}...${address.slice(-4)}`
}

function isExecutionWalletAddress(address?: string | null) {
  return typeof address === 'string' && /^0x[0-9a-f]+$/i.test(address.trim())
}

function avatarLetter(username?: string, address?: string) {
  if (username?.length) {
    return username[0]?.toUpperCase() ?? 'M'
  }
  if (address?.length) {
    return address.slice(2, 3).toUpperCase()
  }
  return 'M'
}

function normalizeUsername(value: string) {
  return value.trim().toLowerCase()
}

function formatAuthError(error: unknown, fallback: string) {
  const message = error instanceof Error ? error.message : fallback
  const normalized = message.toLowerCase()

  if (normalized.includes('origin not allowed')) {
    return 'Privy blocked this origin. Open Moros on localhost or add this exact origin to the Privy allowed domains.'
  }

  return message
}

function balanceAmountLabel(value?: string) {
  return value?.replace(/\s*STRK\s*$/i, '') ?? '0'
}

function gameIconStyle(icon: string): CSSProperties {
  return {
    '--icon-url': `url(${icon})`,
  } as CSSProperties
}

function readStoredFlag(key: string) {
  if (typeof window === 'undefined') {
    return null
  }

  try {
    return typeof window.localStorage?.getItem === 'function'
      ? window.localStorage.getItem(key)
      : null
  } catch {
    return null
  }
}

function writeStoredFlag(key: string, value: string) {
  if (typeof window === 'undefined') {
    return
  }

  try {
    if (typeof window.localStorage?.setItem === 'function') {
      window.localStorage.setItem(key, value)
    }
  } catch {
    // Storage can be unavailable in constrained environments.
  }
}

const LazyMorosAuthDialog = lazy(() =>
  import('./MorosAuthDialog').then((module) => ({ default: module.MorosAuthDialog })),
)
const LazyDepositModal = lazy(() =>
  import('./DepositModal').then((module) => ({ default: module.DepositModal })),
)
const LazyFundingPromptDialog = lazy(() =>
  import('./FundingPromptDialog').then((module) => ({ default: module.FundingPromptDialog })),
)
const LazySettingsDrawer = lazy(() =>
  import('./SettingsDrawer').then((module) => ({ default: module.SettingsDrawer })),
)
const LazyWithdrawModal = lazy(() =>
  import('./WithdrawModal').then((module) => ({ default: module.WithdrawModal })),
)
const LazyVaultModal = lazy(() =>
  import('./VaultModal').then((module) => ({ default: module.VaultModal })),
)

type LoginProvider = 'wallet' | 'google' | 'email' | 'privy'
type AuthDialogMode = 'login' | 'signup'
type ProfileMenuItem = {
  icon: NavIconName
  label: string
  href?: string
  to?: string
  onSelect?: () => void
}

const profilePrimaryItems: ProfileMenuItem[] = [
  { icon: 'profile', label: 'Profile' },
  { icon: 'vault', label: 'Vault' },
  { icon: 'leaderboard', label: 'Leaderboard', to: '/leaderboard' },
  { icon: 'rewards', label: 'Rewards', to: '/rewards' },
]

const profileSecondaryItems: ProfileMenuItem[] = [
  { icon: 'support', label: 'Support', href: 'mailto:admin@moros.bet' },
]

function resolveProfileProvider(
  strategy?: 'external' | 'privy',
  requestedProvider?: LoginProvider,
) {
  if (strategy === 'external') {
    return 'wallet'
  }
  if (requestedProvider === 'google') {
    return 'google'
  }
  if (requestedProvider === 'email') {
    return 'email'
  }
  return 'privy'
}

async function tryFetchProfile(walletAddress: string) {
  try {
    return await fetchPlayerProfile(walletAddress)
  } catch {
    return undefined
  }
}

export function AppShell() {
  const location = useLocation()
  const auth = useMorosAuthRuntime()
  const {
    address,
    connectPrivy,
    connectExternal,
    disconnect,
    ensureGameplaySession,
    listExternalWallets,
    pendingLabel,
    restore,
    signTypedData,
    status,
    strategy,
    withdraw,
  } = useMorosWallet()
  const usernameFromProfile = useProfileStore((state) => state.username)
  const authProviderFromProfile = useProfileStore((state) => state.authProvider)
  const setProfile = useProfileStore((state) => state.setProfile)
  const clearProfile = useProfileStore((state) => state.clearProfile)
  const accountUserId = useAccountStore((state) => state.userId)
  const accountWalletAddress = useAccountStore((state) => state.walletAddress)
  const setResolvedAccount = useAccountStore((state) => state.setResolvedAccount)
  const clearAccount = useAccountStore((state) => state.clearAccount)
  const sidebarCollapsed = useUiStore((state) => state.sidebarCollapsed)
  const toggleSidebar = useUiStore((state) => state.toggleSidebar)

  const [authDialogOpen, setAuthDialogOpen] = useState(false)
  const [authDialogMode, setAuthDialogMode] = useState<AuthDialogMode>('login')
  const [depositModalOpen, setDepositModalOpen] = useState(false)
  const [fundingPromptOpen, setFundingPromptOpen] = useState(false)
  const [authError, setAuthError] = useState<string>()
  const [balancesLoading, setBalancesLoading] = useState(false)
  const [morosBalances, setMorosBalances] = useState<{
    gambling_balance: string
    gambling_reserved: string
    user_id: string
    updated_at: string
    vault_balance: string
  }>()
  const [profileDrawerOpen, setProfileDrawerOpen] = useState(false)
  const [vaultModalOpen, setVaultModalOpen] = useState(false)
  const [walletOptions, setWalletOptions] = useState<ExternalMorosWalletOption[]>([])
  const [walletOptionsLoading, setWalletOptionsLoading] = useState(false)
  const [walletPopoverOpen, setWalletPopoverOpen] = useState(false)
  const [profileSyncing, setProfileSyncing] = useState(false)
  const [profileMenuOpen, setProfileMenuOpen] = useState(false)
  const [accountProvisioning, setAccountProvisioning] = useState(false)
  const [accountResolveRetryTick, setAccountResolveRetryTick] = useState(0)
  const [withdrawModalOpen, setWithdrawModalOpen] = useState(false)
  const pendingProviderRef = useRef<LoginProvider | undefined>(undefined)
  const privyProvisionAttemptRef = useRef<string | undefined>(undefined)
  const privyProvisionInFlightRef = useRef(false)
  const privyProvisionRetryTimerRef = useRef<number | undefined>(undefined)
  const accountResolveRetryTimerRef = useRef<number | undefined>(undefined)
  const privyWalletConnectAttemptRef = useRef<string | undefined>(undefined)
  const restoreAttemptedRef = useRef(false)
  const profileMenuRef = useRef<HTMLDivElement | null>(null)
  const topWalletRef = useRef<HTMLDivElement | null>(null)

  const { depositReady, onboardingBusy, onboardingLabel, resolvedWalletAddress, signedIn } = deriveMorosAccountState({
    authReady: auth.ready,
    authenticated: auth.authenticated,
    authLoading: auth.loading,
    oauthLoading: auth.oauthLoading,
    emailState: auth.emailState,
    accountUserId,
    accountWalletAddress,
    runtimeWalletAddress: address,
    walletStatus: status,
  })
  const walletBusy =
    status === 'connecting' || status === 'preparing' || status === 'funding' || status === 'confirming'
  const walletActionReady = Boolean(address)
  const isGameRoute = morosGames.some((game) => location.pathname.startsWith(game.route))
  const profileIdentity =
    usernameFromProfile ??
    shortAddress(resolvedWalletAddress) ??
    (signedIn ? 'Moros account' : undefined)
  const morosBalanceFormatted = useMemo(
    () => formatStrk(morosBalances?.gambling_balance),
    [morosBalances?.gambling_balance],
  )

  const syncProfile = useCallback(
    async (
      walletAddress: string,
      overrides?: {
        username?: string | null
        authProvider?: string
        createIfMissing?: boolean
      },
    ) => {
      setProfileSyncing(true)
      try {
        const upsertWithProof = async (
          username: string | null | undefined,
          authProvider: string,
        ) => {
          const challenge = await fetchProfileClaimChallenge({
            wallet_address: walletAddress,
            username,
            auth_provider: authProvider,
          })
          const signature = await signTypedData(challenge.typed_data)

          return upsertPlayerProfile({
            wallet_address: walletAddress,
            username,
            auth_provider: authProvider,
            challenge_id: challenge.challenge_id,
            signature,
          })
        }

        const existing = await tryFetchProfile(walletAddress)
        if (existing) {
          setProfile({
            username: existing.username,
            authProvider: existing.auth_provider,
          })

          if (
            overrides &&
            'username' in overrides &&
            normalizeUsername(overrides.username ?? '') !== normalizeUsername(existing.username ?? '')
          ) {
            const updated = await upsertWithProof(
              overrides.username ?? null,
              overrides.authProvider ?? existing.auth_provider,
            )
            setProfile({
              username: updated.username,
              authProvider: updated.auth_provider,
            })
            return updated
          }

          return existing
        }

        if (!overrides?.createIfMissing) {
          clearProfile()
          return undefined
        }

        const created = await upsertWithProof(overrides.username ?? null, overrides.authProvider ?? 'wallet')
        setProfile({
          username: created.username,
          authProvider: created.auth_provider,
        })
        return created
      } finally {
        setProfileSyncing(false)
      }
    },
    [clearProfile, setProfile, signTypedData],
  )

  const maybePromptFunding = useCallback((walletAddress?: string) => {
    if (typeof window === 'undefined' || !walletAddress) {
      return
    }

    const storageKey = `${FUNDING_PROMPT_KEY_PREFIX}${walletAddress.toLowerCase()}`
    if (readStoredFlag(storageKey) === '1') {
      return
    }

    writeStoredFlag(storageKey, '1')
    setFundingPromptOpen(true)
  }, [])

  useEffect(() => {
    if (restoreAttemptedRef.current) {
      return
    }

    restoreAttemptedRef.current = true
    void restore()
  }, [restore])

  useEffect(() => {
    document.documentElement.dataset.morosMode = 'dark'
    writeStoredFlag('moros-dark-mode', '1')
  }, [])

  useEffect(() => {
    setProfileMenuOpen(false)
    setWalletPopoverOpen(false)
  }, [location.pathname])

  const refreshMorosBalances = useCallback(async () => {
    if (!resolvedWalletAddress) {
      setMorosBalances(undefined)
      return
    }

    setBalancesLoading(true)
    try {
      const nextBalances = await fetchAccountBalancesByWalletAddress(resolvedWalletAddress)
      setMorosBalances(nextBalances)
    } finally {
      setBalancesLoading(false)
    }
  }, [resolvedWalletAddress])

  useEffect(() => {
    if (!resolvedWalletAddress) {
      setMorosBalances(undefined)
      return
    }

    let cancelled = false

    const load = async () => {
      try {
        const nextBalances = await fetchAccountBalancesByWalletAddress(resolvedWalletAddress)
        if (!cancelled) {
          setMorosBalances(nextBalances)
          setBalancesLoading(false)
        }
      } catch {
        if (!cancelled) {
          setBalancesLoading(false)
        }
      }
    }

    setBalancesLoading(true)
    void load()
    const interval = window.setInterval(() => {
      void load()
    }, 12000)

    return () => {
      cancelled = true
      window.clearInterval(interval)
    }
  }, [resolvedWalletAddress])

  useEffect(() => {
    if (!profileMenuOpen && !walletPopoverOpen) {
      return
    }

    const handlePointerDown = (event: PointerEvent) => {
      const target = event.target as Node | null
      if (profileMenuOpen && profileMenuRef.current && target && !profileMenuRef.current.contains(target)) {
        setProfileMenuOpen(false)
      }
      if (walletPopoverOpen && topWalletRef.current && target && !topWalletRef.current.contains(target)) {
        setWalletPopoverOpen(false)
      }
    }

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        setProfileMenuOpen(false)
        setWalletPopoverOpen(false)
      }
    }

    document.addEventListener('pointerdown', handlePointerDown)
    document.addEventListener('keydown', handleKeyDown)
    return () => {
      document.removeEventListener('pointerdown', handlePointerDown)
      document.removeEventListener('keydown', handleKeyDown)
    }
  }, [profileMenuOpen, walletPopoverOpen])

  useEffect(() => {
    if (!resolvedWalletAddress) {
      clearProfile()
      return
    }

    let cancelled = false
    void tryFetchProfile(resolvedWalletAddress)
      .then((profile) => {
        if (cancelled) {
          return
        }
        if (!profile) {
          clearProfile()
          return
        }
        setProfile({
          username: profile.username,
          authProvider: profile.auth_provider,
        })
      })
      .catch(() => {
        if (!cancelled) {
          clearProfile()
        }
      })

    return () => {
      cancelled = true
    }
  }, [clearProfile, resolvedWalletAddress, setProfile])

  const resolvePrivyIdentityToken = useCallback(async () => {
    return (
      await waitForPrivyRequestToken(auth, {
        attempts: 60,
        delayMs: 150,
      })
    ) ?? null
  }, [auth])

  const ensureCanonicalAccount = useCallback(async (
    input: {
      walletAddress?: string
      authProvider?: string
      linkedVia: string
      makePrimary?: boolean
      authMethod: LoginProvider | 'wallet'
      identityToken?: string | null
    },
  ) => {
    let account
    if (input.authProvider === 'privy') {
      const identityToken = input.identityToken ?? await resolvePrivyIdentityToken()
      if (!identityToken) {
        throw new Error('Moros auth is still preparing.')
      }
      account = await resolveMorosPrivyAccount(identityToken, {
        wallet_address: input.walletAddress,
        linked_via: input.linkedVia,
        make_primary: input.makePrimary,
      })
    } else {
      if (!input.walletAddress) {
        throw new Error('Wallet address is required to resolve a Moros account.')
      }
      const challenge = await createMorosAccountResolveChallenge({
        wallet_address: input.walletAddress,
        linked_via: input.linkedVia,
        make_primary: input.makePrimary,
      })
      const signature = await signTypedData(challenge.typed_data)
      account = await resolveMorosWalletAccount({
        wallet_address: input.walletAddress,
        challenge_id: challenge.challenge_id,
        signature,
      })
    }
    setResolvedAccount({
      userId: account.user_id,
      walletAddress: isExecutionWalletAddress(account.wallet_address)
        ? account.wallet_address
        : input.walletAddress,
      authMethod: input.authMethod === 'wallet'
        ? 'wallet'
        : input.authMethod === 'google'
          ? 'google'
          : input.authMethod === 'email'
            ? 'email'
            : 'privy',
    })
    return account
  }, [resolvePrivyIdentityToken, setResolvedAccount, signTypedData])

  const ensurePrivyAccount = useCallback(async (identityToken: string) => {
    if (accountWalletAddress) {
      return accountWalletAddress
    }

    const walletLink = await ensureMorosPrivyWallet(identityToken)
    if (!walletLink) {
      throw new Error('Privy Starknet wallet is not ready. Retry in a moment.')
    }
    const authMethod = pendingProviderRef.current === 'google'
      ? 'google'
      : pendingProviderRef.current === 'email'
        ? 'email'
        : 'privy'
    setResolvedAccount({
      walletAddress: walletLink.wallet_address,
      authMethod,
    })
    const canonicalAccount = await ensureCanonicalAccount({
      walletAddress: walletLink.wallet_address,
      authProvider: 'privy',
      linkedVia: 'privy_wallet',
      makePrimary: true,
      authMethod,
      identityToken,
    })
    setResolvedAccount({
      userId: canonicalAccount.user_id,
      walletAddress: walletLink.wallet_address,
      authMethod,
    })
    setAuthError(undefined)
    setAuthDialogOpen(false)
    maybePromptFunding(walletLink.wallet_address)
    return walletLink.wallet_address
  }, [
    accountWalletAddress,
    ensureCanonicalAccount,
    maybePromptFunding,
    setResolvedAccount,
  ])

  const scheduleAccountResolveRetry = useCallback((delayMs = 1200) => {
    if (typeof window === 'undefined') {
      return
    }

    if (accountResolveRetryTimerRef.current) {
      window.clearTimeout(accountResolveRetryTimerRef.current)
    }

    accountResolveRetryTimerRef.current = window.setTimeout(() => {
      accountResolveRetryTimerRef.current = undefined
      setAccountResolveRetryTick((current) => current + 1)
    }, delayMs)
  }, [])

  const provisionPrivyAccount = useCallback(async () => {
    if (
      !auth.ready ||
      !auth.authenticated ||
      !accountUserId ||
      accountWalletAddress ||
      privyProvisionInFlightRef.current
    ) {
      return
    }

    try {
      privyProvisionInFlightRef.current = true
      const identityToken = await resolvePrivyIdentityToken()
      if (!identityToken) {
        return
      }

      if (privyProvisionAttemptRef.current === identityToken) {
        return
      }

      privyProvisionAttemptRef.current = identityToken
      setAccountProvisioning(true)
      await ensurePrivyAccount(identityToken)
      setAuthError(undefined)
    } catch (error) {
      privyProvisionAttemptRef.current = undefined
      setAuthError(formatAuthError(error, 'Login failed.'))
    } finally {
      privyProvisionInFlightRef.current = false
      setAccountProvisioning(false)
    }
  }, [
    accountUserId,
    accountWalletAddress,
    auth.authenticated,
    auth.ready,
    ensurePrivyAccount,
    resolvePrivyIdentityToken,
  ])

  useEffect(() => {
    if (!auth.ready || !auth.authenticated) {
      privyProvisionAttemptRef.current = undefined
      privyProvisionInFlightRef.current = false
      privyWalletConnectAttemptRef.current = undefined
      if (privyProvisionRetryTimerRef.current) {
        window.clearTimeout(privyProvisionRetryTimerRef.current)
        privyProvisionRetryTimerRef.current = undefined
      }
      if (accountResolveRetryTimerRef.current) {
        window.clearTimeout(accountResolveRetryTimerRef.current)
        accountResolveRetryTimerRef.current = undefined
      }
      setAccountProvisioning(false)
      return
    }

    if (accountWalletAddress) {
      if (privyProvisionRetryTimerRef.current) {
        window.clearTimeout(privyProvisionRetryTimerRef.current)
        privyProvisionRetryTimerRef.current = undefined
      }
      if (accountResolveRetryTimerRef.current) {
        window.clearTimeout(accountResolveRetryTimerRef.current)
        accountResolveRetryTimerRef.current = undefined
      }
      if (!accountUserId && privyProvisionInFlightRef.current) {
        return
      }
      setAccountProvisioning(false)
      return
    }

    if (!accountUserId) {
      if (privyProvisionRetryTimerRef.current) {
        window.clearTimeout(privyProvisionRetryTimerRef.current)
        privyProvisionRetryTimerRef.current = undefined
      }
      return
    }

    if (accountResolveRetryTimerRef.current) {
      window.clearTimeout(accountResolveRetryTimerRef.current)
      accountResolveRetryTimerRef.current = undefined
    }

    void provisionPrivyAccount()
  }, [
    accountUserId,
    accountWalletAddress,
    auth.authenticated,
    auth.ready,
    provisionPrivyAccount,
  ])

  useEffect(() => {
    if (!auth.ready || !auth.authenticated || !accountUserId || accountWalletAddress) {
      return
    }

    if (privyProvisionRetryTimerRef.current) {
      window.clearTimeout(privyProvisionRetryTimerRef.current)
      privyProvisionRetryTimerRef.current = undefined
    }

    privyProvisionRetryTimerRef.current = window.setTimeout(() => {
      privyProvisionRetryTimerRef.current = undefined
      void provisionPrivyAccount()
    }, 1500)

    return () => {
      if (privyProvisionRetryTimerRef.current) {
        window.clearTimeout(privyProvisionRetryTimerRef.current)
        privyProvisionRetryTimerRef.current = undefined
      }
    }
  }, [
    accountUserId,
    accountWalletAddress,
    auth.authenticated,
    auth.ready,
    provisionPrivyAccount,
  ])

  useEffect(() => {
    if (accountUserId || accountProvisioning || (!resolvedWalletAddress && (!auth.ready || !auth.authenticated))) {
      return
    }

    let cancelled = false
    void (async () => {
      try {
        if (resolvedWalletAddress) {
          const usesPrivyIdentity = auth.authenticated && strategy !== 'external'
          const identityToken = usesPrivyIdentity ? await resolvePrivyIdentityToken() : null
          if (usesPrivyIdentity && !identityToken) {
            if (!cancelled) {
              scheduleAccountResolveRetry()
            }
            return
          }
          if (cancelled) {
            return
          }
          setAccountProvisioning(true)
          await ensureCanonicalAccount({
            walletAddress: resolvedWalletAddress,
            authProvider: usesPrivyIdentity ? 'privy' : undefined,
            linkedVia: usesPrivyIdentity ? 'privy_wallet_sync' : 'wallet_restore',
            makePrimary: false,
            authMethod: strategy === 'external'
              ? 'wallet'
              : pendingProviderRef.current === 'google'
                ? 'google'
                : pendingProviderRef.current === 'email'
                  ? 'email'
                  : 'privy',
            identityToken,
          })
          return
        }

        if (!auth.ready || !auth.authenticated) {
          return
        }

        const identityToken = await resolvePrivyIdentityToken()
        if (!identityToken) {
          if (!cancelled) {
            scheduleAccountResolveRetry()
          }
          return
        }
        if (cancelled) {
          return
        }
        setAccountProvisioning(true)
        await ensurePrivyAccount(identityToken)
      } catch (error) {
        if (!cancelled) {
          setAuthError(formatAuthError(error, 'Moros account sync failed.'))
        }
      } finally {
        if (!cancelled) {
          setAccountProvisioning(false)
        }
      }
    })()

    return () => {
      cancelled = true
    }
  }, [
    accountProvisioning,
    accountUserId,
    auth.authenticated,
    auth.ready,
    ensureCanonicalAccount,
    ensurePrivyAccount,
    resolvePrivyIdentityToken,
    resolvedWalletAddress,
    scheduleAccountResolveRetry,
    strategy,
    accountResolveRetryTick,
  ])

  useEffect(() => {
    const privyWalletAlreadyConnected =
      Boolean(address) && Boolean(accountWalletAddress) &&
      address?.toLowerCase() === accountWalletAddress?.toLowerCase()

    if (!auth.ready || !auth.authenticated || !accountWalletAddress || privyWalletAlreadyConnected) {
      return
    }

    const connectKey = `${accountWalletAddress.toLowerCase()}:${auth.userId ?? 'privy'}`
    if (privyWalletConnectAttemptRef.current === connectKey) {
      return
    }

    let cancelled = false
    privyWalletConnectAttemptRef.current = connectKey

    const connectProvisionedWallet = async () => {
      const identityToken = await resolvePrivyIdentityToken()
      if (!identityToken || cancelled) {
        privyWalletConnectAttemptRef.current = undefined
        return
      }

      try {
        const connected = await connectPrivy(identityToken)
        if (!cancelled) {
          await ensureCanonicalAccount({
            walletAddress: connected.address,
            authProvider: 'privy',
            linkedVia: 'privy_connect',
            makePrimary: true,
            authMethod: pendingProviderRef.current === 'google'
              ? 'google'
              : pendingProviderRef.current === 'email'
                ? 'email'
                : 'privy',
            identityToken,
          })
        }
      } catch (error) {
        if (!cancelled) {
          privyWalletConnectAttemptRef.current = undefined
          setAuthError(formatAuthError(error, 'Moros wallet setup failed.'))
        }
      }
    }

    void connectProvisionedWallet()

    return () => {
      cancelled = true
    }
  }, [
    accountWalletAddress,
    address,
    auth.authenticated,
    auth.ready,
    auth.userId,
    connectPrivy,
    ensureCanonicalAccount,
    resolvePrivyIdentityToken,
  ])

  const openAuthDialog = useCallback((mode: AuthDialogMode) => {
    setAuthError(undefined)
    setAuthDialogMode(mode)
    setWalletOptions([])
    setWalletOptionsLoading(false)
    if (auth.enabled) {
      void auth.ensureLoaded().catch(() => undefined)
    }
    setAuthDialogOpen(true)
  }, [auth])

  const handleSignupOpen = useCallback(() => {
    openAuthDialog('signup')
  }, [openAuthDialog])

  const handleLoginOpen = useCallback(() => {
    openAuthDialog('login')
  }, [openAuthDialog])

  const handleLoginWarmup = useCallback(() => {
    if (auth.enabled) {
      void auth.ensureLoaded().catch(() => undefined)
    }
  }, [auth])

  const handleDepositOpen = useCallback(async () => {
    if (!signedIn) {
      openAuthDialog('login')
      return
    }

    setWalletPopoverOpen(false)
    setDepositModalOpen(true)
  }, [openAuthDialog, signedIn])

  useEffect(() => {
    if (!auth.authenticated && !resolvedWalletAddress && auth.ready && !auth.loading) {
      clearAccount()
    }
  }, [
    auth.authenticated,
    auth.loading,
    auth.ready,
    clearAccount,
    resolvedWalletAddress,
  ])

  useEffect(() => () => {
    if (accountResolveRetryTimerRef.current) {
      window.clearTimeout(accountResolveRetryTimerRef.current)
      accountResolveRetryTimerRef.current = undefined
    }
  }, [])

  const handleGoogleLogin = useCallback(async () => {
    pendingProviderRef.current = 'google'
    setAuthError(undefined)
    try {
      await auth.loginWithGoogle()
    } catch (error) {
      setAuthError(formatAuthError(error, 'Google login failed.'))
    }
  }, [auth])

  const handleEmailCodeSend = useCallback(async (email: string) => {
    pendingProviderRef.current = 'email'
    setAuthError(undefined)
    try {
      await auth.sendEmailCode(email)
    } catch (error) {
      setAuthError(formatAuthError(error, 'Email login failed.'))
    }
  }, [auth])

  const handleEmailCodeVerify = useCallback(async (code: string) => {
    pendingProviderRef.current = 'email'
    setAuthError(undefined)
    try {
      await auth.verifyEmailCode(code)
    } catch (error) {
      setAuthError(formatAuthError(error, 'Email verification failed.'))
    }
  }, [auth])

  const handleOpenWallets = useCallback(async () => {
    setAuthError(undefined)
    setWalletOptionsLoading(true)
    try {
      const options = await listExternalWallets()
      setWalletOptions(options)
      if (!options.length) {
        setAuthError('No Starknet wallet detected. Install Braavos or Argent X, or continue with Google/email.')
      }
    } catch (error) {
      setWalletOptions([])
      setAuthError(formatAuthError(error, 'Wallet discovery failed.'))
    } finally {
      setWalletOptionsLoading(false)
    }
  }, [listExternalWallets])

  const handleWalletLogin = useCallback(async (provider?: ExternalMorosWalletOption['provider']) => {
    pendingProviderRef.current = 'wallet'
    setAuthError(undefined)
    try {
      const connected = await connectExternal(provider)
      await ensureCanonicalAccount({
        walletAddress: connected.address,
        linkedVia: 'wallet_login',
        makePrimary: true,
        authMethod: 'wallet',
      })
      const profile = await syncProfile(connected.address, {
        createIfMissing: authDialogMode !== 'signup',
        username: null,
        authProvider: 'wallet',
      })
      setAuthDialogOpen(false)
      maybePromptFunding(connected.address)
    } catch (error) {
      setAuthError(formatAuthError(error, 'Wallet login failed.'))
    }
  }, [authDialogMode, connectExternal, ensureCanonicalAccount, maybePromptFunding, syncProfile])

  const handleSaveProfile = useCallback(async (username?: string) => {
    let activeAddress = address
    if (!activeAddress && auth.authenticated && strategy !== 'external') {
      try {
        const identityToken = await resolvePrivyIdentityToken()
        if (identityToken) {
          const connected = await connectPrivy(identityToken)
          activeAddress = connected.address
        }
      } catch (error) {
        setAuthError(formatAuthError(error, 'Moros wallet setup failed.'))
        return
      }
    }

    if (!activeAddress) {
      setAuthError('Connect a wallet before saving a profile.')
      return
    }

    const normalizedUsername = normalizeUsername(username ?? '')
    const nextUsername = normalizedUsername.length ? normalizedUsername : null
    const provider = resolveProfileProvider(strategy, pendingProviderRef.current)
    try {
      const profile = await syncProfile(activeAddress, {
        createIfMissing: true,
        username: nextUsername,
        authProvider: provider,
      })

      setProfile({
        username: profile?.username,
        authProvider: profile?.auth_provider ?? provider,
      })
      setAuthError(undefined)
      setAuthDialogOpen(false)
      maybePromptFunding(activeAddress)
    } catch (error) {
      setAuthError(error instanceof Error ? error.message : 'Profile save failed.')
    }
  }, [address, auth.authenticated, connectPrivy, maybePromptFunding, resolvePrivyIdentityToken, setProfile, strategy, syncProfile])

  const handleProfileOpen = useCallback(() => {
    setProfileMenuOpen(false)
    setProfileDrawerOpen(true)
  }, [])

  const handleVaultOpen = useCallback(() => {
    setProfileMenuOpen(false)
    setVaultModalOpen(true)
  }, [])

  const handleWithdrawOpen = useCallback(() => {
    setWalletPopoverOpen(false)
    setWithdrawModalOpen(true)
  }, [])

  async function handleLogout() {
    setWalletPopoverOpen(false)
    setProfileMenuOpen(false)
    setProfileDrawerOpen(false)
    setVaultModalOpen(false)
    setWithdrawModalOpen(false)
    setAuthDialogOpen(false)
    setAuthError(undefined)
    pendingProviderRef.current = undefined
    clearProfile()
    clearAccount()
    setDepositModalOpen(false)
    await disconnect()
    if (auth.authenticated) {
      await auth.logout()
    }
  }

  const dialogEmailState: MorosEmailState = auth.emailState

  const avatarTitle = useMemo(() => {
    if (!resolvedWalletAddress) {
      return 'Profile'
    }
    if (profileIdentity) {
      return profileIdentity
    }
    return shortAddress(resolvedWalletAddress) ?? 'Profile'
  }, [profileIdentity, resolvedWalletAddress])

  const resolvedProfilePrimaryItems = useMemo(
    () =>
      profilePrimaryItems.map((item) => {
        if (item.label === 'Profile') {
          return { ...item, onSelect: handleProfileOpen }
        }
        if (item.label === 'Vault') {
          return { ...item, onSelect: handleVaultOpen }
        }
        return item
      }),
    [handleProfileOpen, handleVaultOpen],
  )

  const resolvedProfileSecondaryItems = profileSecondaryItems

  function renderProfileItem(item: ProfileMenuItem) {
    const content = (
      <>
        <NavIcon className="profile-dropdown__item-icon" name={item.icon} variant="fill" />
        <span className="profile-dropdown__item-label">{item.label}</span>
      </>
    )

    if (item.to) {
      return (
        <Link
          className="profile-dropdown__item"
          key={item.label}
          onClick={() => setProfileMenuOpen(false)}
          to={item.to}
        >
          {content}
        </Link>
      )
    }

    if (item.href) {
      return (
        <a
          className="profile-dropdown__item"
          href={item.href}
          key={item.label}
          onClick={() => setProfileMenuOpen(false)}
        >
          {content}
        </a>
      )
    }

    return (
      <button
        className="profile-dropdown__item"
        key={item.label}
        onClick={() => {
          setProfileMenuOpen(false)
          item.onSelect?.()
        }}
        type="button"
      >
        {content}
      </button>
    )
  }

  return (
    <div className="app-shell">
      <header className="top-header">
        <Link className="top-brand" to="/">
          <img alt="" className="brand__mark" src="/transparent.png?v=3" />
          <strong>Moros</strong>
        </Link>

        <div className="top-header__center">
          {signedIn ? (
            <div className="top-wallet" ref={topWalletRef}>
              <button
                aria-expanded={walletPopoverOpen}
                aria-haspopup="dialog"
                aria-label={`Open wallet, balance ${morosBalanceFormatted}`}
                className="balance-chip balance-chip--wallet"
                onClick={() => {
                  setProfileMenuOpen(false)
                  setWalletPopoverOpen((open) => !open)
                }}
                type="button"
              >
                <span className="balance-chip__amount">
                  {balanceAmountLabel(morosBalanceFormatted)}
                </span>
                <span className="balance-chip__unit">STRK</span>
              </button>

              {walletPopoverOpen ? (
                <div aria-label="Wallet" className="wallet-popover" role="dialog">
                  <div className="wallet-popover__head">
                    <strong>{balanceAmountLabel(morosBalanceFormatted)} STRK</strong>
                    <small>{shortAddress(resolvedWalletAddress) ?? 'Moros wallet'}</small>
                  </div>
                  <div className="wallet-popover__balance-grid">
                    <div className="wallet-popover__balance-row">
                      <span>Wallet</span>
                      <strong>{morosBalanceFormatted}</strong>
                    </div>
                    <div className="wallet-popover__balance-row">
                      <span>Vault</span>
                      <strong>{formatStrk(morosBalances?.vault_balance)}</strong>
                    </div>
                  </div>
                  <div className="wallet-popover__actions">
                    <button
                      className="button button--ghost wallet-popover__action"
                      onClick={handleWithdrawOpen}
                      type="button"
                    >
                      Withdraw
                    </button>
                    <button
                      className="button button--primary wallet-popover__action"
                      onClick={() => void handleDepositOpen()}
                      type="button"
                    >
                      Deposit
                    </button>
                  </div>
                  {balancesLoading ? (
                    <div className="wallet-funds__inline-meta">
                      <span>Refreshing wallet balance…</span>
                    </div>
                  ) : null}
                </div>
              ) : null}
            </div>
          ) : null}
        </div>

        <div className="top-header__right">
          {signedIn ? (
            <>
              <button
                aria-label="Deposit"
                className="button button--primary button--compact top-header__deposit-button"
                disabled={!depositReady || profileSyncing}
                onClick={() => void handleDepositOpen()}
                type="button"
                title="Deposit"
              >
                Deposit
              </button>
              <button
                aria-label="Notifications"
                className="top-header__icon-button"
                title="Notifications"
                type="button"
              >
                <img alt="" className="top-header__icon-image" src="/icons/bell.svg" />
              </button>
            </>
          ) : null}
          {signedIn ? (
            <div
              className="top-profile"
              ref={profileMenuRef}
            >
              <button
                aria-expanded={profileMenuOpen}
                aria-haspopup="menu"
                className="avatar-button"
                onClick={() => {
                  setWalletPopoverOpen(false)
                  setProfileMenuOpen((open) => !open)
                }}
                title={avatarTitle}
                type="button"
              >
                {avatarLetter(usernameFromProfile, resolvedWalletAddress)}
              </button>

              {profileMenuOpen ? (
                <div className="profile-dropdown" role="menu">
                  <div className="profile-dropdown__section profile-dropdown__section--header">
                    <div className="profile-dropdown__header">
                      <span className="profile-dropdown__avatar">{avatarLetter(usernameFromProfile, resolvedWalletAddress)}</span>
                      <div className="profile-dropdown__identity">
                        <strong>{profileIdentity ?? 'Moros account'}</strong>
                      </div>
                    </div>
                  </div>

                  <div className="profile-dropdown__section">
                    {resolvedProfilePrimaryItems.map((item) => renderProfileItem(item))}
                    {resolvedProfileSecondaryItems.map((item) => renderProfileItem(item))}
                  </div>

                  <div className="profile-dropdown__section profile-dropdown__section--logout">
                    <button className="profile-dropdown__item profile-dropdown__item--danger" onClick={() => void handleLogout()} type="button">
                      <NavIcon className="profile-dropdown__item-icon" name="logout" />
                      <span className="profile-dropdown__item-label">Logout</span>
                    </button>
                  </div>
                </div>
              ) : null}
            </div>
          ) : (
            <div className="top-header__auth-actions">
              <button
                className="button button--compact top-header__auth-button"
                disabled={walletBusy || profileSyncing}
                onClick={handleSignupOpen}
                onFocus={handleLoginWarmup}
                onPointerDown={handleLoginWarmup}
                type="button"
              >
                Sign up
              </button>
              <button
                className="button button--compact top-header__auth-button"
                disabled={walletBusy || profileSyncing}
                onClick={handleLoginOpen}
                onFocus={handleLoginWarmup}
                onPointerDown={handleLoginWarmup}
                type="button"
              >
                Login
              </button>
            </div>
          )}
        </div>
      </header>

      <div className={isGameRoute ? (sidebarCollapsed ? 'shell-body shell-body--collapsed shell-body--game' : 'shell-body shell-body--game') : (sidebarCollapsed ? 'shell-body shell-body--collapsed' : 'shell-body')}>
        <aside className={sidebarCollapsed ? 'game-sidebar game-sidebar--collapsed' : 'game-sidebar'} aria-label="Sidebar">
          <div className="sidebar-toolbar">
            <button
              aria-label={sidebarCollapsed ? 'Expand sidebar' : 'Collapse sidebar'}
              className="sidebar-toggle"
              onClick={toggleSidebar}
              type="button"
            >
              <img alt="" aria-hidden="true" className="sidebar-toggle__icon" draggable={false} src="/icons/hamburger.svg" />
            </button>
          </div>

          <nav className="sidebar-nav">
            {morosGames.map((game) => (
              <NavLink
                className={({ isActive }) => (isActive ? 'sidebar-nav__link sidebar-nav__link--active' : 'sidebar-nav__link')}
                key={game.route}
                to={game.route}
              >
                <span className="sidebar-nav__icon" aria-hidden="true">
                  <span className="sidebar-nav__game-icon-mask" style={gameIconStyle(game.navIcon)} />
                </span>
                {!sidebarCollapsed ? <span className="sidebar-nav__label">{game.title}</span> : null}
              </NavLink>
            ))}
          </nav>

          <div className="sidebar-divider" />

          <nav className="sidebar-nav">
            {utilityNav.map((item) => (
              <NavLink
                className={({ isActive }) => (isActive ? 'sidebar-nav__link sidebar-nav__link--active' : 'sidebar-nav__link')}
                key={item.to}
                to={item.to}
              >
                <span className="sidebar-nav__icon" aria-hidden="true">
                  <span className="sidebar-nav__game-icon-mask" style={gameIconStyle(item.navIcon)} />
                </span>
                {!sidebarCollapsed ? <span className="sidebar-nav__label">{item.label}</span> : null}
              </NavLink>
            ))}
          </nav>
        </aside>

        <main className={isGameRoute ? 'content content--game' : 'content'}>
          <Outlet />
        </main>
      </div>

      {authDialogOpen ? (
        <Suspense fallback={null}>
          <LazyMorosAuthDialog
            address={resolvedWalletAddress}
            authReady={auth.ready}
            emailState={dialogEmailState}
            error={authError}
            needsUsername={false}
            oauthLoading={auth.oauthLoading}
            onClose={() => setAuthDialogOpen(false)}
            onGoogleLogin={handleGoogleLogin}
            onOpenWallets={handleOpenWallets}
            onSaveProfile={handleSaveProfile}
            onSendEmailCode={handleEmailCodeSend}
            onVerifyEmailCode={handleEmailCodeVerify}
            onWalletLogin={handleWalletLogin}
            open
            pendingLabel={
              walletBusy
                ? pendingLabel
                : onboardingBusy
                  ? onboardingLabel
                  : accountProvisioning
                    ? 'Syncing account...'
                  : profileSyncing
                    ? 'Syncing profile...'
                    : undefined
            }
            mode={authDialogMode}
            privyEnabled={auth.enabled}
            walletOptions={walletOptions}
            walletsLoading={walletOptionsLoading}
            username={usernameFromProfile}
          />
        </Suspense>
      ) : null}
      {depositModalOpen ? (
        <Suspense fallback={null}>
          <LazyDepositModal
            balanceFormatted={morosBalanceFormatted}
            onClose={() => {
              setDepositModalOpen(false)
            }}
            open
            resolveIdToken={auth.authenticated ? resolvePrivyIdentityToken : undefined}
            signedIn={signedIn}
            walletAddress={resolvedWalletAddress}
          />
        </Suspense>
      ) : null}
      {withdrawModalOpen ? (
        <Suspense fallback={null}>
          <LazyWithdrawModal
            balanceFormatted={morosBalanceFormatted}
            balances={morosBalances}
            ensureGameplaySession={ensureGameplaySession}
            onClose={() => setWithdrawModalOpen(false)}
            onSettled={refreshMorosBalances}
            onboardingLabel={onboardingLabel}
            open
            signedIn={signedIn}
            userId={accountUserId}
            walletActionReady={walletActionReady}
            walletAddress={resolvedWalletAddress}
            withdraw={withdraw}
          />
        </Suspense>
      ) : null}
      {vaultModalOpen ? (
        <Suspense fallback={null}>
          <LazyVaultModal
            balances={morosBalances}
            ensureGameplaySession={ensureGameplaySession}
            onClose={() => setVaultModalOpen(false)}
            onSettled={refreshMorosBalances}
            onboardingLabel={onboardingLabel}
            open
            signedIn={signedIn}
            userId={accountUserId}
            walletActionReady={walletActionReady}
            walletAddress={resolvedWalletAddress}
          />
        </Suspense>
      ) : null}
      {profileDrawerOpen ? (
        <Suspense fallback={null}>
          <LazySettingsDrawer
            authProvider={authProviderFromProfile}
            onClose={() => setProfileDrawerOpen(false)}
            onSaveUsername={handleSaveProfile}
            open
            userId={accountUserId}
            walletAddress={resolvedWalletAddress}
          />
        </Suspense>
      ) : null}
      {fundingPromptOpen ? (
        <Suspense fallback={null}>
          <LazyFundingPromptDialog
            onDeposit={() => {
              setFundingPromptOpen(false)
              void handleDepositOpen()
            }}
            onSkip={() => setFundingPromptOpen(false)}
            open
          />
        </Suspense>
      ) : null}
    </div>
  )
}
