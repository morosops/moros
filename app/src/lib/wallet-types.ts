export type MorosExecuteCall = {
  contractAddress: string
  entrypoint: string
  calldata: string[]
}

export type MorosExecuteOptions = {
  feeMode?: 'user_pays' | 'sponsored'
}

export type MorosEnsureReadyOptions = {
  deploy?: 'never' | 'if_needed' | 'always'
  feeMode?: 'user_pays' | 'sponsored'
  onProgress?: (event: { step: 'CONNECTED' | 'CHECK_DEPLOYED' | 'DEPLOYING' | 'FAILED' | 'READY' }) => void
}

export type MorosExecution = {
  hash: string
  explorerUrl?: string
  wait: () => Promise<unknown>
}

export type MorosExternalExecution = {
  hash: string
}

export type MorosBalance = {
  toFormatted: (compact?: boolean) => string
  toUnit: () => string
}

export type MorosSwapRequest = {
  tokenIn: unknown
  tokenOut: unknown
  amountIn: unknown
  slippageBps?: bigint
  provider?: string
}

export type MorosWallet = {
  address: string
  disconnect: () => Promise<void>
  username?: () => Promise<string | undefined>
  getController?: () => unknown
  signMessage: (typedData: unknown) => Promise<string[]>
  getChainId: () => unknown
  ensureReady: (options?: MorosEnsureReadyOptions) => Promise<void>
  balanceOf: (token: unknown) => Promise<MorosBalance>
  swap: (request: MorosSwapRequest, options?: MorosExecuteOptions) => Promise<MorosExecution>
  deposit: (
    recipient: string,
    amount: unknown,
    token: unknown,
    externalWallet: unknown,
    options?: unknown,
  ) => Promise<MorosExternalExecution>
  getDepositBalance: (token: unknown, externalWallet: unknown) => Promise<MorosBalance>
  getDepositFeeEstimate: (token: unknown, externalWallet: unknown, options?: unknown) => Promise<unknown>
  execute: (
    calls: MorosExecuteCall[],
    options?: MorosExecuteOptions,
  ) => Promise<MorosExecution>
}
