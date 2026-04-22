import { create } from 'zustand'
import type { MorosWallet } from '../lib/wallet-types'

export type WalletConnectionState =
  | 'idle'
  | 'connecting'
  | 'preparing'
  | 'connected'
  | 'funding'
  | 'confirming'
  | 'error'

type WalletStore = {
  wallet?: MorosWallet
  address?: string
  strategy?: 'external' | 'privy'
  balanceFormatted?: string
  balanceUnit?: string
  status: WalletConnectionState
  pendingLabel?: string
  error?: string
  txHash?: string
  explorerUrl?: string
  setConnecting: (label?: string) => void
  setPreparing: (label?: string) => void
  setConnected: (
    wallet: MorosWallet,
    strategy: 'external' | 'privy',
    balanceFormatted: string,
    balanceUnit: string,
  ) => void
  setBalance: (balanceFormatted: string, balanceUnit: string) => void
  setFunding: (label?: string) => void
  setConfirming: (txHash: string, explorerUrl?: string, label?: string) => void
  setReady: () => void
  setError: (message: string) => void
  clearError: () => void
  reset: () => void
}

export const useWalletStore = create<WalletStore>((set) => ({
  status: 'idle',
  setConnecting: (pendingLabel = 'Connecting wallet...') =>
    set({ status: 'connecting', pendingLabel, error: undefined }),
  setPreparing: (pendingLabel = 'Preparing wallet...') =>
    set({ status: 'preparing', pendingLabel, error: undefined }),
  setConnected: (wallet, strategy, balanceFormatted, balanceUnit) =>
    set({
      wallet,
      address: wallet.address,
      strategy,
      balanceFormatted,
      balanceUnit,
      status: 'connected',
      pendingLabel: undefined,
      error: undefined,
    }),
  setBalance: (balanceFormatted, balanceUnit) =>
    set({ balanceFormatted, balanceUnit }),
  setFunding: (pendingLabel = 'Submitting...') =>
    set({ status: 'funding', pendingLabel, error: undefined }),
  setConfirming: (txHash, explorerUrl, pendingLabel = 'Confirming on Starknet...') =>
    set({ txHash, explorerUrl, pendingLabel, status: 'confirming', error: undefined }),
  setReady: () =>
    set((state) => ({
      status: state.wallet ? 'connected' : 'idle',
      pendingLabel: undefined,
      error: undefined,
    })),
  setError: (error) => set({ status: 'error', pendingLabel: undefined, error }),
  clearError: () => set({ error: undefined }),
  reset: () =>
    set({
      wallet: undefined,
      address: undefined,
      strategy: undefined,
      balanceFormatted: undefined,
      balanceUnit: undefined,
      status: 'idle',
      pendingLabel: undefined,
      error: undefined,
      txHash: undefined,
      explorerUrl: undefined,
    }),
}))
