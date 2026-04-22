import {
  Amount,
  BridgeTokenRepository,
  ConnectedEthereumWallet,
  ExternalChain,
  type Address,
  type BridgeDepositFeeEstimation,
  type BridgeToken,
  networks,
  type Token,
} from './starkzap-runtime'
import { morosConfig } from './config'
import type { SwapToStrkPayload } from './starkzap-types'
import type { MorosWallet } from './wallet-types'

export type MorosBridgeToken = BridgeToken
export type MorosEthereumBridgeWallet = ConnectedEthereumWallet

type EthereumProvider = {
  request<T = unknown>(args: {
    method: string
    params?: unknown[] | Record<string, unknown> | object
  }): Promise<T>
}

const STRK_TOKEN: Token = {
  name: 'STRK',
  address: morosConfig.strkToken as Address,
  decimals: 18,
  symbol: 'STRK',
}

let bridgeTokenRepository: BridgeTokenRepository | null = null
const financeWalletReadyCache = new Map<string, Promise<void>>()

function resolveNetwork(value: string) {
  if (value === 'mainnet' || value === 'devnet') {
    return value
  }
  return 'sepolia'
}

function getBridgeTokenRepository() {
  if (!bridgeTokenRepository) {
    bridgeTokenRepository = new BridgeTokenRepository()
  }

  return bridgeTokenRepository
}

function getBridgeTokenEnv(): 'mainnet' | 'testnet' {
  return networks[resolveNetwork(morosConfig.network)].chainId.isMainnet() ? 'mainnet' : 'testnet'
}

function getFeeMode(): 'user_pays' | 'sponsored' {
  if (morosConfig.paymasterUrl) {
    return 'sponsored'
  }
  return 'user_pays'
}

function getInjectedEthereumProvider(): EthereumProvider {
  const provider = (window as typeof window & { ethereum?: EthereumProvider }).ethereum
  if (!provider) {
    throw new Error('No injected Ethereum wallet found. Open Moros in a browser with MetaMask or another EIP-1193 wallet.')
  }
  return provider
}

async function prepareFinanceWalletForExecution(wallet: MorosWallet) {
  const cached = financeWalletReadyCache.get(wallet.address)
  if (cached) {
    return cached
  }

  const pending = wallet.ensureReady({
    deploy: 'if_needed',
    feeMode: getFeeMode(),
  }).finally(() => {
    financeWalletReadyCache.delete(wallet.address)
  })

  financeWalletReadyCache.set(wallet.address, pending)
  return pending
}

export async function listEthereumBridgeTokens() {
  return getBridgeTokenRepository().getTokens({
    env: getBridgeTokenEnv(),
    chain: ExternalChain.ETHEREUM,
  })
}

export async function connectEthereumBridgeWallet(wallet: MorosWallet) {
  const provider = getInjectedEthereumProvider()
  const accounts = await provider.request<string[]>({
    method: 'eth_requestAccounts',
  })
  const address = accounts?.[0]
  if (!address) {
    throw new Error('No Ethereum account was returned by the injected wallet.')
  }

  const chainId = await provider.request<string>({ method: 'eth_chainId' })
  return ConnectedEthereumWallet.from(
    {
      chain: ExternalChain.ETHEREUM,
      provider,
      address,
      chainId,
    },
    wallet.getChainId() as Parameters<typeof ConnectedEthereumWallet.from>[1],
  )
}

export async function getEthereumBridgeBalance(
  wallet: MorosWallet,
  token: BridgeToken,
  externalWallet: ConnectedEthereumWallet,
) {
  return wallet.getDepositBalance(token, externalWallet)
}

export async function getEthereumBridgeFeeEstimate(
  wallet: MorosWallet,
  token: BridgeToken,
  externalWallet: ConnectedEthereumWallet,
) {
  return wallet.getDepositFeeEstimate(token, externalWallet)
}

export async function bridgeFromEthereum(
  wallet: MorosWallet,
  token: BridgeToken,
  externalWallet: ConnectedEthereumWallet,
  amountInput: string,
) {
  const amount = Amount.parse(amountInput, token.decimals, token.symbol)
  if (amount.isZero()) {
    throw new Error('Bridge amount must be greater than zero.')
  }

  return wallet.deposit(wallet.address as Address, amount, token, externalWallet)
}

export function formatBridgeFeeEstimate(estimate: BridgeDepositFeeEstimation) {
  if ('l1Fee' in estimate) {
    const parts = [
      `L1 ${estimate.l1Fee.toFormatted(false)}`,
      `L2 ${estimate.l2Fee.toFormatted(false)}`,
      `Approval ${estimate.approvalFee.toFormatted(false)}`,
    ]

    if ('fastTransferBpFee' in estimate && typeof estimate.fastTransferBpFee === 'number') {
      parts.push(`Fast ${(estimate.fastTransferBpFee / 100).toFixed(2)}%`)
    }

    return parts.join(' · ')
  }

  return `Local ${estimate.localFee.toFormatted(false)} · Interchain ${estimate.interchainFee.toFormatted(false)}`
}

export async function swapToStrk(wallet: MorosWallet, payload: SwapToStrkPayload) {
  const symbol = payload.tokenSymbol.trim().toUpperCase()
  if (!payload.tokenAddress || !payload.tokenAddress.startsWith('0x')) {
    throw new Error('Enter a Starknet token address for the asset you want to swap.')
  }
  if (!symbol) {
    throw new Error('Enter the source token symbol.')
  }
  if (!Number.isInteger(payload.tokenDecimals) || payload.tokenDecimals < 0 || payload.tokenDecimals > 36) {
    throw new Error('Token decimals must be an integer between 0 and 36.')
  }
  if (payload.slippageBps < 1 || payload.slippageBps > 1000) {
    throw new Error('Slippage must be between 0.01% and 10%.')
  }

  const tokenIn: Token = {
    name: symbol,
    address: payload.tokenAddress as Address,
    decimals: payload.tokenDecimals,
    symbol,
  }
  const amountIn = Amount.parse(payload.amountInput, tokenIn)
  if (amountIn.isZero()) {
    throw new Error('Swap amount must be greater than zero.')
  }

  await prepareFinanceWalletForExecution(wallet)

  return wallet.swap(
    {
      tokenIn,
      tokenOut: STRK_TOKEN,
      amountIn,
      slippageBps: BigInt(payload.slippageBps),
      provider: 'ekubo',
    },
    {
      feeMode: getFeeMode(),
    },
  )
}
