export type Address = string

export type Token = {
  name: string
  address: Address
  decimals: number
  symbol: string
}

export type BridgeToken = Token & {
  id: string
  protocol: string
}

function createBalance(formatted = '0 STRK', unit = '0') {
  return {
    toFormatted: () => formatted,
    toUnit: () => unit,
  }
}

export class Amount {
  static parse(value: string) {
    const normalized = Number(value)
    return {
      isZero: () => !Number.isFinite(normalized) || normalized <= 0,
      toBase: () => BigInt(Math.max(0, Math.round(normalized * 1_000_000))),
    }
  }

  static fromRaw(raw: bigint | number | string) {
    const numeric = typeof raw === 'bigint' ? raw : BigInt(raw)
    return {
      toFormatted: () => `${numeric.toString()} STRK`,
      toUnit: () => numeric.toString(),
    }
  }
}

export type BridgeDepositFeeEstimation = {
  l1Fee: ReturnType<typeof createBalance>
  l2Fee: ReturnType<typeof createBalance>
  approvalFee: ReturnType<typeof createBalance>
  fastTransferBpFee?: number
}

export const ExternalChain = {
  ETHEREUM: 'ethereum',
  SOLANA: 'solana',
} as const

export class EkuboSwapProvider {}

export class ConnectedEthereumWallet {
  constructor(
    public readonly address: string,
    public readonly chainId: string,
  ) {}

  static from(config: { address: string; chainId: string }) {
    return new ConnectedEthereumWallet(config.address, config.chainId)
  }
}

function createMockWallet(address = '0x1') {
  return {
    address,
    disconnect: async () => undefined,
    ensureReady: async () => undefined,
    getChainId: () => '0x534e5f5345504f4c4941',
    balanceOf: async () => createBalance('0 STRK', '0'),
    getDepositBalance: async () => createBalance('100 USDC', '100000000'),
    getDepositFeeEstimate: async () =>
      ({
        l1Fee: createBalance('0.0002 ETH', '200000000000000'),
        l2Fee: createBalance('0.01 STRK', '10000000000000000'),
        approvalFee: createBalance('0.0001 ETH', '100000000000000'),
        fastTransferBpFee: 15,
      }) satisfies BridgeDepositFeeEstimation,
    swap: async () => ({
      hash: '0xswap',
      wait: async () => undefined,
    }),
    deposit: async () => ({
      hash: '0xbridge',
    }),
    execute: async () => ({
      hash: '0xexecute',
      wait: async () => undefined,
    }),
  }
}

const bridgeTokens: BridgeToken[] = [
  {
    id: 'eth-usdc',
    protocol: 'StarkGate',
    name: 'USD Coin',
    address: '0x123',
    decimals: 6,
    symbol: 'USDC',
  },
  {
    id: 'eth-strk',
    protocol: 'StarkGate',
    name: 'Starknet',
    address: '0x456',
    decimals: 18,
    symbol: 'STRK',
  },
]

export class StarkZap {
  constructor(_config?: unknown) {}

  getProvider() {
    return {
      waitForTransaction: async () => undefined,
    }
  }

  async getBridgingTokens(_chain: string) {
    return bridgeTokens
  }

  async onboard(options: { strategy: 'external' | 'signer' }) {
    return {
      wallet: createMockWallet(options.strategy === 'external' ? '0xexternal' : '0xsigner'),
      strategy: options.strategy,
      deployed: true,
    }
  }
}
