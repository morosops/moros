import {
  PrivyProvider,
  getIdentityToken,
  useIdentityToken,
  useLoginWithEmail,
  useLoginWithOAuth,
  useLogout,
  usePrivy,
} from '@privy-io/react-auth'
import { useCreateWallet, useSignRawHash } from '@privy-io/react-auth/extended-chains'
import { useEffect, useRef, type PropsWithChildren } from 'react'
import { morosConfig } from '../lib/config'
import {
  MorosAuthContext,
  MOROS_PRIVY_HINT_KEY,
  type MorosAuthRuntime,
} from './moros-auth-context'

type PendingAuthAction =
  | { id: number; type: 'google' }
  | { id: number; type: 'email-send'; email: string }
  | { id: number; type: 'email-verify'; code: string }

type MorosPrivyRuntimeProps = PropsWithChildren<{
  ensureLoaded: () => Promise<void>
  pendingAction?: PendingAuthAction
  onPendingActionHandled: (id: number) => void
}>

type PrivyStarknetWalletLink = {
  wallet_id: string
  wallet_address: string
  public_key?: string
  user_id: string
}

type PrivyLinkedAccount = {
  id?: string | null
  address?: string
  chain_type?: string
  public_key?: string
  type?: string
  wallet_client_type?: string
}

function normalizeStarknetWallet(userId: string, wallet: unknown): PrivyStarknetWalletLink | undefined {
  const linked = wallet as PrivyLinkedAccount | undefined
  if (
    !linked ||
    (
      linked.type !== undefined &&
      linked.type !== 'wallet' &&
      linked.type !== 'smart_wallet'
    ) ||
    linked.chain_type !== 'starknet' ||
    typeof linked.id !== 'string' ||
    !linked.id ||
    typeof linked.address !== 'string' ||
    !linked.address
  ) {
    return undefined
  }

  return {
    wallet_id: linked.id,
    wallet_address: linked.address,
    public_key: typeof linked.public_key === 'string' ? linked.public_key : undefined,
    user_id: userId,
  }
}

function findStarknetWallet(
  user: { id: string; linked_accounts?: unknown[] } | null | undefined,
  preferredWalletAddress?: string,
) {
  if (!user?.linked_accounts) {
    return undefined
  }

  let firstWallet: PrivyStarknetWalletLink | undefined
  const normalizedPreferred =
    typeof preferredWalletAddress === 'string' && preferredWalletAddress.trim()
      ? preferredWalletAddress.trim().toLowerCase()
      : undefined

  for (const linkedAccount of user.linked_accounts) {
    const wallet = normalizeStarknetWallet(user.id, linkedAccount)
    if (wallet) {
      if (
        normalizedPreferred &&
        wallet.wallet_address.trim().toLowerCase() === normalizedPreferred
      ) {
        return wallet
      }
      firstWallet ??= wallet
    }
  }

  return firstWallet
}

function delay(ms: number) {
  return new Promise((resolve) => window.setTimeout(resolve, ms))
}

function PrivyRuntimeProvider({
  children,
  ensureLoaded,
  pendingAction,
  onPendingActionHandled,
}: MorosPrivyRuntimeProps) {
  const { authenticated, getAccessToken, ready, user } = usePrivy()
  const { identityToken } = useIdentityToken()
  const { sendCode, loginWithCode, state: emailState } = useLoginWithEmail()
  const { initOAuth, loading: oauthLoading } = useLoginWithOAuth()
  const { logout } = useLogout()
  const { createWallet } = useCreateWallet()
  const { signRawHash } = useSignRawHash()
  const createdStarknetWalletRef = useRef<PrivyStarknetWalletLink | undefined>(undefined)
  const handledActionIdRef = useRef<number | undefined>(undefined)
  const currentUserRef = useRef(user)
  const createWalletRef = useRef(createWallet)
  const signRawHashRef = useRef(signRawHash)

  useEffect(() => {
    currentUserRef.current = user
  }, [user])

  useEffect(() => {
    createWalletRef.current = createWallet
  }, [createWallet])

  useEffect(() => {
    signRawHashRef.current = signRawHash
  }, [signRawHash])

  useEffect(() => {
    if (authenticated) {
      try {
        window.localStorage.setItem(MOROS_PRIVY_HINT_KEY, '1')
      } catch {
        // Storage can be unavailable in constrained environments.
      }
    }
  }, [authenticated])

  useEffect(() => {
    if (!pendingAction || handledActionIdRef.current === pendingAction.id) {
      return
    }

    handledActionIdRef.current = pendingAction.id

    const run = async () => {
      try {
        if (pendingAction.type === 'google') {
          await initOAuth({ provider: 'google' })
          return
        }

        if (pendingAction.type === 'email-send') {
          await sendCode({ email: pendingAction.email })
          return
        }

        await loginWithCode({ code: pendingAction.code })
      } finally {
        onPendingActionHandled(pendingAction.id)
      }
    }

    void run()
  }, [initOAuth, loginWithCode, onPendingActionHandled, pendingAction, sendCode])

  const runtime: MorosAuthRuntime = {
    enabled: true,
    loaded: true,
    loading: false,
    ready,
    authenticated,
    userId: user?.id,
    identityToken: identityToken ?? undefined,
    getAccessToken,
    emailState:
      emailState.status === 'error'
        ? 'error'
        : emailState.status === 'awaiting-code-input'
          ? 'awaiting-code'
          : emailState.status === 'sending-code'
            ? 'sending-code'
            : emailState.status === 'submitting-code'
              ? 'verifying'
              : 'idle',
    oauthLoading,
    ensureLoaded,
    sendEmailCode: async (email: string) => {
      await sendCode({ email })
    },
    verifyEmailCode: async (code: string) => {
      await loginWithCode({ code })
    },
    loginWithGoogle: async () => {
      await initOAuth({ provider: 'google' })
    },
    getIdentityToken: async () => getIdentityToken(),
    ensureStarknetWallet: async (options) => {
      const currentUser = currentUserRef.current
      if (!currentUser?.id) {
        throw new Error('User must be authenticated before creating a Moros wallet.')
      }

      const existingWallet = findStarknetWallet(currentUser, options?.preferredWalletAddress)
      if (existingWallet) {
        createdStarknetWalletRef.current = existingWallet
        return existingWallet
      }

      if (createdStarknetWalletRef.current?.user_id === currentUser.id) {
        return createdStarknetWalletRef.current
      }

      const created = await createWalletRef.current({ chainType: 'starknet' })
      const createdWallet =
        normalizeStarknetWallet(currentUser.id, created.wallet) ??
        findStarknetWallet(created.user)
      if (!createdWallet) {
        throw new Error('Privy did not return a Starknet wallet for this Moros account.')
      }

      createdStarknetWalletRef.current = createdWallet
      return createdWallet
    },
    signStarknetHash: async ({ address, hash }) => {
      let lastError: unknown
      for (let attempt = 0; attempt < 6; attempt += 1) {
        try {
          const signed = await signRawHashRef.current({
            address,
            chainType: 'starknet',
            hash,
          })
          return signed.signature
        } catch (error) {
          lastError = error
          const message = error instanceof Error ? error.message : String(error)
          if (!message.toLowerCase().includes('wallet not found') || attempt === 5) {
            throw error
          }
          await delay(150)
        }
      }

      throw lastError instanceof Error ? lastError : new Error('Wallet not found')
    },
    logout: async () => {
      try {
        window.localStorage.removeItem(MOROS_PRIVY_HINT_KEY)
      } catch {
        // Storage can be unavailable in constrained environments.
      }
      await logout()
    },
  }

  return <MorosAuthContext.Provider value={runtime}>{children}</MorosAuthContext.Provider>
}

export default function MorosPrivyRuntime(props: MorosPrivyRuntimeProps) {
  if (!morosConfig.privyAppId) {
    throw new Error('Privy is not configured.')
  }

  return (
    <PrivyProvider
      appId={morosConfig.privyAppId}
      config={{
        appearance: {
          accentColor: '#c9ccd6',
          theme: 'dark',
        },
      }}
    >
      <PrivyRuntimeProvider {...props} />
    </PrivyProvider>
  )
}
