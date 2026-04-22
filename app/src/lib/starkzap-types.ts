import type { StarknetWindowObject } from '@starknet-io/get-starknet-core'
import type { MorosPrivyWalletLink } from './privy-bridge'

export type MorosNetwork = 'sepolia' | 'mainnet' | 'devnet'
export type SupportedWalletMode = 'external' | 'privy'

export type ExternalMorosWalletOption = {
  id: string
  name: string
  icon: string
  provider: StarknetWindowObject
}

export type ConnectMorosWalletOptions = {
  onProgress?: (event: { step: 'CONNECTED' | 'CHECK_DEPLOYED' | 'DEPLOYING' | 'FAILED' | 'READY' }) => void
}

export type ConnectMorosPrivyWalletOptions = ConnectMorosWalletOptions & {
  idToken: string
  signingToken: string
  walletLink?: MorosPrivyWalletLink
  resolveSigningToken?: () => Promise<string | undefined>
  resolvePaymasterToken?: () => Promise<string | undefined>
  signRawHash?: (input: { address: string; hash: `0x${string}` }) => Promise<string>
}

export type OpenDiceRoundPayload = {
  tableId: number
  wagerWei: string
  bankrollBalanceWei?: string
  targetBps: number
  rollOver: boolean
  clientSeed: string
  commitmentId: number
}

export type RouletteBetInput = {
  kind: number
  selection: number
  amountWei: string
}

export type OpenRouletteSpinPayload = {
  tableId: number
  totalWagerWei: string
  bankrollBalanceWei?: string
  clientSeed: string
  commitmentId: number
  bets: RouletteBetInput[]
}

export type OpenBaccaratRoundPayload = {
  tableId: number
  wagerWei: string
  bankrollBalanceWei?: string
  betSide: number
  clientSeed: string
  commitmentId: number
}

export type SwapToStrkPayload = {
  tokenAddress: string
  tokenSymbol: string
  tokenDecimals: number
  amountInput: string
  slippageBps: number
}
