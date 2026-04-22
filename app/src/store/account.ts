import { create } from 'zustand'

export type MorosAccountAuthMethod = 'google' | 'email' | 'wallet' | 'privy'

type AccountState = {
  userId?: string
  walletAddress?: string
  authMethod?: MorosAccountAuthMethod
  setResolvedAccount: (account: {
    userId?: string | null
    walletAddress?: string | null
    authMethod?: MorosAccountAuthMethod
  }) => void
  clearAccount: () => void
}

export const useAccountStore = create<AccountState>((set) => ({
  userId: undefined,
  walletAddress: undefined,
  authMethod: undefined,
  setResolvedAccount: (account) =>
    set((state) => ({
      userId: 'userId' in account ? account.userId ?? undefined : state.userId,
      walletAddress:
        'walletAddress' in account ? account.walletAddress ?? undefined : state.walletAddress,
      authMethod: 'authMethod' in account ? account.authMethod : state.authMethod,
    })),
  clearAccount: () => set({
    userId: undefined,
    walletAddress: undefined,
    authMethod: undefined,
  }),
}))
