import { createContext } from 'react'

export type MorosEmailState = 'idle' | 'sending-code' | 'awaiting-code' | 'verifying' | 'error'

export type MorosAuthRuntime = {
  enabled: boolean
  loaded: boolean
  loading: boolean
  ready: boolean
  authenticated: boolean
  userId?: string
  identityToken?: string
  accessToken?: string
  emailState: MorosEmailState
  oauthLoading: boolean
  ensureLoaded: () => Promise<void>
  getAccessToken: () => Promise<string | null>
  sendEmailCode: (email: string) => Promise<void>
  verifyEmailCode: (code: string) => Promise<void>
  loginWithGoogle: () => Promise<void>
  getIdentityToken: () => Promise<string | null>
  ensureStarknetWallet?: (options?: {
    preferredWalletAddress?: string
  }) => Promise<{
    wallet_id: string
    wallet_address: string
    public_key?: string
    user_id: string
  }>
  signStarknetHash?: (input: { address: string; hash: `0x${string}` }) => Promise<string>
  logout: () => Promise<void>
}

export const MOROS_PRIVY_HINT_KEY = 'moros.auth.privy'

const notConfiguredError = new Error('Privy is not configured.')

export const noopRuntime: MorosAuthRuntime = {
  enabled: false,
  loaded: false,
  loading: false,
  ready: true,
  authenticated: false,
  emailState: 'idle',
  oauthLoading: false,
  ensureLoaded: async () => {
    throw notConfiguredError
  },
  getAccessToken: async () => null,
  sendEmailCode: async () => {
    throw notConfiguredError
  },
  verifyEmailCode: async () => {
    throw notConfiguredError
  },
  loginWithGoogle: async () => {
    throw notConfiguredError
  },
  getIdentityToken: async () => null,
  logout: async () => undefined,
}

export const MorosAuthContext = createContext<MorosAuthRuntime>(noopRuntime)
