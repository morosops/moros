import {
  Suspense,
  lazy,
  startTransition,
  useCallback,
  useContext,
  useMemo,
  useRef,
  useState,
  type PropsWithChildren,
} from 'react'
import { morosConfig } from '../lib/config'
import {
  MorosAuthContext,
  MOROS_PRIVY_HINT_KEY,
  noopRuntime,
  type MorosAuthRuntime,
  type MorosEmailState,
} from './moros-auth-context'

type PendingAuthAction =
  | { id: number; type: 'google' }
  | { id: number; type: 'email-send'; email: string }
  | { id: number; type: 'email-verify'; code: string }

type PendingAuthActionPayload =
  | { type: 'google' }
  | { type: 'email-send'; email: string }
  | { type: 'email-verify'; code: string }

const LazyMorosPrivyRuntime = lazy(() => import('./MorosPrivyRuntime'))

let privyRuntimeModulePromise: Promise<unknown> | null = null
const PRIVY_SESSION_STORAGE_KEYS = [
  MOROS_PRIVY_HINT_KEY,
  'privy:id_token',
  'privy-id-token',
  'privy:token',
  'privy-token',
  'privy:refresh_token',
  'privy-refresh-token',
  'privy-session',
]

function readStorageFlag(key: string) {
  if (typeof window === 'undefined') {
    return null
  }

  try {
    return window.localStorage.getItem(key)
  } catch {
    return null
  }
}

function writeStorageFlag(key: string, value: string | null) {
  if (typeof window === 'undefined') {
    return
  }

  try {
    if (value === null) {
      window.localStorage.removeItem(key)
      return
    }
    window.localStorage.setItem(key, value)
  } catch {
    // Storage can be unavailable in constrained environments.
  }
}

function preloadPrivyRuntime() {
  if (!privyRuntimeModulePromise) {
    privyRuntimeModulePromise = import('./MorosPrivyRuntime')
  }
  return privyRuntimeModulePromise
}

function shouldBootstrapPrivy() {
  if (!morosConfig.privyAppId || typeof window === 'undefined') {
    return false
  }

  const search = new URLSearchParams(window.location.search)
  if (
    search.has('privy_oauth_code') ||
    search.has('privy_oauth_state') ||
    search.has('privy_oauth_provider')
  ) {
    return true
  }

  return PRIVY_SESSION_STORAGE_KEYS.some((key) => Boolean(readStorageFlag(key)))
}

function markPrivyBootstrapRequested() {
  writeStorageFlag(MOROS_PRIVY_HINT_KEY, '1')
}

export function MorosAuthProvider({ children }: PropsWithChildren) {
  const [loadRequested, setLoadRequested] = useState(() => shouldBootstrapPrivy())
  const [pendingAction, setPendingAction] = useState<PendingAuthAction>()
  const pendingActionIdRef = useRef(0)

  const ensureLoaded = useCallback(async () => {
    if (!morosConfig.privyAppId) {
      throw new Error('Privy is not configured.')
    }

    markPrivyBootstrapRequested()
    startTransition(() => {
      setLoadRequested(true)
    })
    await preloadPrivyRuntime()
  }, [])

  const queuePendingAction = useCallback(
    async (action: PendingAuthActionPayload) => {
      const nextId = pendingActionIdRef.current + 1
      pendingActionIdRef.current = nextId
      setPendingAction({ ...action, id: nextId })
      await ensureLoaded()
    },
    [ensureLoaded],
  )

  const pendingRuntime = useMemo<MorosAuthRuntime>(() => {
    if (!morosConfig.privyAppId) {
      return noopRuntime
    }

    const emailState: MorosEmailState =
      pendingAction?.type === 'email-send'
        ? 'sending-code'
        : pendingAction?.type === 'email-verify'
          ? 'verifying'
          : 'idle'

    return {
      enabled: true,
      loaded: loadRequested,
      loading: loadRequested,
      ready: false,
      authenticated: false,
      emailState,
      oauthLoading: pendingAction?.type === 'google',
      ensureLoaded,
      getAccessToken: async () => null,
      sendEmailCode: async (email: string) => {
        await queuePendingAction({ type: 'email-send', email })
      },
      verifyEmailCode: async (code: string) => {
        await queuePendingAction({ type: 'email-verify', code })
      },
      loginWithGoogle: async () => {
        await queuePendingAction({ type: 'google' })
      },
      getIdentityToken: async () => null,
      logout: async () => {
        writeStorageFlag(MOROS_PRIVY_HINT_KEY, null)
      },
    }
  }, [ensureLoaded, loadRequested, pendingAction, queuePendingAction])

  if (!morosConfig.privyAppId) {
    return <MorosAuthContext.Provider value={noopRuntime}>{children}</MorosAuthContext.Provider>
  }

  if (!loadRequested) {
    return <MorosAuthContext.Provider value={pendingRuntime}>{children}</MorosAuthContext.Provider>
  }

  return (
    <Suspense fallback={<MorosAuthContext.Provider value={pendingRuntime}>{children}</MorosAuthContext.Provider>}>
      <LazyMorosPrivyRuntime
        ensureLoaded={ensureLoaded}
        onPendingActionHandled={(id) => {
          setPendingAction((current) => (current?.id === id ? undefined : current))
        }}
        pendingAction={pendingAction}
      >
        {children}
      </LazyMorosPrivyRuntime>
    </Suspense>
  )
}

export function useMorosAuthRuntime() {
  return useContext(MorosAuthContext)
}

export type { MorosAuthRuntime, MorosEmailState }
