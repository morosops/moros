import {
  Amount,
  ArgentXV050Preset,
  PrivySigner,
  type Address,
  type Token,
  Wallet,
  networks,
} from './starkzap-runtime'
import { RpcProvider, WalletAccount } from 'starknet'
import starknetCore, { type StarknetWindowObject } from '@starknet-io/get-starknet-core'
import { morosConfig } from './config'
import type {
  ConnectMorosPrivyWalletOptions,
  ConnectMorosWalletOptions,
  ExternalMorosWalletOption,
  MorosNetwork,
  OpenBaccaratRoundPayload,
  OpenDiceRoundPayload,
  OpenRouletteSpinPayload,
  SupportedWalletMode,
} from './starkzap-types'
import type { MorosExecuteCall, MorosWallet } from './wallet-types'

type BrowserStarknetWallet = StarknetWindowObject

const STRK_TOKEN: Token = {
  name: 'STRK',
  address: morosConfig.strkToken as Address,
  decimals: 18,
  symbol: 'STRK',
}
const UINT128_MASK = (1n << 128n) - 1n

function resolveNetwork(value: string): MorosNetwork {
  if (value === 'mainnet' || value === 'devnet') {
    return value
  }
  return 'sepolia'
}

export type StarkzapRuntimePlan = {
  network: MorosNetwork
  walletModes: Array<'external' | 'privy' | 'signer'>
  liveWalletModes: SupportedWalletMode[]
  modules: string[]
  note: string
}

const walletReadyCache = new Map<string, Promise<void>>()
let providerInstance: RpcProvider | null = null

function getNetworkPreset() {
  return networks[resolveNetwork(morosConfig.network)]
}

function getExplorerBaseUrl() {
  return getNetworkPreset().explorerUrl
}

function getProvider() {
  if (!providerInstance) {
    providerInstance = new RpcProvider({
      nodeUrl: getNetworkPreset().rpcUrl,
    })
  }

  return providerInstance
}

function getWalletConfig(options?: {
  paymasterHeaders?: Record<string, string>
  paymasterFetch?: typeof fetch
}) {
  const explorerBaseUrl = getExplorerBaseUrl()
  return {
    rpcUrl: getNetworkPreset().rpcUrl,
    chainId: getNetworkPreset().chainId,
    ...(morosConfig.paymasterUrl
      ? {
          paymaster: {
            nodeUrl: morosConfig.paymasterUrl,
            ...(options?.paymasterHeaders ? { headers: options.paymasterHeaders } : {}),
            ...(options?.paymasterFetch ? { baseFetch: options.paymasterFetch } : {}),
          },
        }
      : {}),
    ...(explorerBaseUrl
      ? {
          explorer: {
            baseUrl: explorerBaseUrl,
          },
        }
      : {}),
  }
}

function getStrkToken(): Token {
  return STRK_TOKEN
}

function getFeeMode(): 'user_pays' | 'sponsored' {
  if (morosConfig.paymasterUrl) {
    return 'sponsored'
  }
  return 'user_pays'
}

function isUndeployedAccountError(error: unknown) {
  return error instanceof Error && error.message.includes('Account is not deployed.')
}

async function ensureMorosWalletReady(
  wallet: MorosWallet,
  onProgress?: (event: { step: 'CONNECTED' | 'CHECK_DEPLOYED' | 'DEPLOYING' | 'FAILED' | 'READY' }) => void,
) {
  const cached = walletReadyCache.get(wallet.address)
  if (cached) {
    return cached
  }

  const pending = wallet
    .ensureReady({
      deploy: 'if_needed',
      ...(onProgress ? { onProgress } : {}),
    })
    .finally(() => {
      walletReadyCache.delete(wallet.address)
    })

  walletReadyCache.set(wallet.address, pending)
  return pending
}

export async function prepareMorosWalletForExecution(wallet: MorosWallet) {
  await ensureMorosWalletReady(wallet)
}

async function executeWithReadyRetry<T>(wallet: MorosWallet, run: () => Promise<T>) {
  try {
    return await run()
  } catch (error) {
    if (!isUndeployedAccountError(error)) {
      throw error
    }

    await ensureMorosWalletReady(wallet)
    return run()
  }
}

function resolveStarknetWalletIcon(icon: BrowserStarknetWallet['icon']) {
  if (typeof icon === 'string') {
    return icon
  }
  return icon.dark || icon.light
}

function createUnsupportedMethod(name: string) {
  return async () => {
    throw new Error(`${name} is not available for external wallets in this build.`)
  }
}

function balanceFromRaw(raw: bigint, token: Token) {
  const amount = Amount.fromRaw(raw, token)
  return {
    toFormatted: (compact?: boolean) => amount.toFormatted(compact),
    toUnit: () => amount.toUnit(),
  }
}

function normalizeSignature(signature: unknown) {
  if (Array.isArray(signature)) {
    return signature.map((value) => value.toString())
  }

  if (
    signature &&
    typeof signature === 'object' &&
    'r' in signature &&
    's' in signature
  ) {
    const { r, s } = signature as { r: { toString: () => string }; s: { toString: () => string } }
    return [r.toString(), s.toString()]
  }

  throw new Error('Wallet signature has an unexpected format.')
}

async function readTokenBalanceRaw(wallet: WalletAccount, token: Token) {
  const response = await wallet.callContract({
    contractAddress: token.address,
    entrypoint: 'balance_of',
    calldata: [wallet.address],
  })
  const [low = '0', high = '0'] = response
  return BigInt(low) + (BigInt(high) << 128n)
}

async function connectExternalWalletAccount(provider: BrowserStarknetWallet, silent = false) {
  const rpcProvider = getProvider()
  const connectedWallet = silent
    ? provider
    : ((await starknetCore.enable(provider)) as BrowserStarknetWallet)
  const walletAccount = silent
    ? await WalletAccount.connectSilent(
        rpcProvider,
        connectedWallet as Parameters<typeof WalletAccount.connectSilent>[1],
      )
    : await WalletAccount.connect(
        rpcProvider,
        connectedWallet as Parameters<typeof WalletAccount.connect>[1],
      )
  const chainId = await connectedWallet.request({
    type: 'wallet_requestChainId',
  }) as string

  const wallet: MorosWallet = {
    address: walletAccount.address,
    disconnect: async () => {
      await starknetCore.disconnect()
    },
    signMessage: async (typedData) => normalizeSignature(await walletAccount.signMessage(typedData as Parameters<typeof walletAccount.signMessage>[0])),
    ensureReady: async () => undefined,
    getChainId: () => chainId,
    balanceOf: async (token) => balanceFromRaw(await readTokenBalanceRaw(walletAccount, token as Token), token as Token),
    getDepositBalance: createUnsupportedMethod('Bridge balance checks') as MorosWallet['getDepositBalance'],
    getDepositFeeEstimate: createUnsupportedMethod('Bridge fee estimates') as MorosWallet['getDepositFeeEstimate'],
    deposit: createUnsupportedMethod('Bridge deposits') as MorosWallet['deposit'],
    swap: createUnsupportedMethod('Swaps') as MorosWallet['swap'],
    execute: async (calls, _options) => {
      const result = await walletAccount.execute(calls as Parameters<typeof walletAccount.execute>[0])
      const hash = 'transaction_hash' in result ? result.transaction_hash : (result as { transaction_hash: string }).transaction_hash
      return {
        hash,
        ...(getExplorerBaseUrl()
          ? {
              explorerUrl: `${getExplorerBaseUrl()}/tx/${hash}`,
            }
          : {}),
        wait: async () => rpcProvider.waitForTransaction(hash),
      }
    },
  }

  return wallet
}

export async function listAvailableMorosExternalWallets() {
  const wallets = (await starknetCore.getAvailableWallets()) as BrowserStarknetWallet[]
  return wallets.map((provider) => ({
    id: provider.id,
    name: provider.name,
    icon: resolveStarknetWalletIcon(provider.icon),
    provider,
  })) satisfies ExternalMorosWalletOption[]
}

export async function connectMorosExternalWallet(provider?: BrowserStarknetWallet) {
  let selected = provider
  if (!selected) {
    const lastConnected = (await starknetCore.getLastConnectedWallet()) as BrowserStarknetWallet | null | undefined
    if (lastConnected) {
      selected = lastConnected
    }
  }
  if (!selected) {
    const wallets = await listAvailableMorosExternalWallets()
    if (wallets.length === 1) {
      selected = wallets[0].provider
    }
  }
  if (!selected) {
    throw new Error('Choose a Starknet wallet from Login first.')
  }

  const wallet = await connectExternalWalletAccount(selected)
  return {
    wallet,
    strategy: 'external' as const,
  }
}

export async function reconnectMorosExternalWallet() {
  const lastConnected = (await starknetCore.getLastConnectedWallet()) as BrowserStarknetWallet | null | undefined
  if (!lastConnected) {
    return undefined
  }

  const wallet = await connectExternalWalletAccount(lastConnected, true)
  return {
    wallet,
    strategy: 'external' as const,
  }
}

export async function connectMorosPrivyWallet(
  options: ConnectMorosPrivyWalletOptions,
) {
  if (!morosConfig.privyBridgeUrl) {
    throw new Error('Moros Privy bridge is not configured.')
  }

  const privyWallet = options.walletLink
  if (!privyWallet) {
    throw new Error('Privy Starknet wallet is not ready. Refresh and sign in again.')
  }
  if (!options.signRawHash) {
    throw new Error('Privy Starknet signing is not ready. Refresh and sign in again.')
  }
  if (!privyWallet.public_key) {
    throw new Error('Privy Starknet wallet public key is not ready. Refresh and sign in again.')
  }
  const signer = new PrivySigner({
    walletId: privyWallet.wallet_id,
    publicKey: privyWallet.public_key,
    rawSign: async (_walletId, hash) => options.signRawHash!({
      address: privyWallet.wallet_address,
      hash: (hash.startsWith('0x') ? hash : `0x${hash}`) as `0x${string}`,
    }),
    buildBody: async ({ walletId, hash }) => {
      const signingToken = (await options.resolveSigningToken?.()) ?? options.signingToken ?? options.idToken
      return {
        auth_token: signingToken,
        id_token: options.idToken,
        signing_token: signingToken,
        wallet_id: walletId,
        hash,
      }
    },
  })
  const paymasterFetch: typeof fetch = async (input, init) => {
    const latestToken =
      (await options.resolvePaymasterToken?.()) ??
      (await options.resolveSigningToken?.()) ??
      options.signingToken ??
      options.idToken

    const headers = new Headers(init?.headers)
    if (latestToken) {
      headers.set('authorization', `Bearer ${latestToken}`)
    }
    headers.set('x-moros-wallet-id', privyWallet.wallet_id)
    headers.set('x-moros-wallet-address', privyWallet.wallet_address)

    return fetch(input, {
      ...init,
      headers,
    })
  }
  const wallet = await Wallet.create({
    account: {
      signer,
      accountClass: ArgentXV050Preset,
    },
    provider: getProvider(),
    config: getWalletConfig({
      paymasterHeaders: {
        'x-moros-wallet-id': privyWallet.wallet_id,
        'x-moros-wallet-address': privyWallet.wallet_address,
      },
      paymasterFetch,
    }),
    feeMode: getFeeMode(),
  })

  return {
    wallet: wallet as unknown as MorosWallet,
    strategy: 'privy' as const,
    walletLink: privyWallet,
  }
}

export function warmMorosWalletConnect() {
  void getProvider()
}

export async function signMorosMessage(wallet: MorosWallet, typedData: unknown) {
  return normalizeSignature(await wallet.signMessage(typedData))
}

export async function disconnectMorosWallet(wallet: MorosWallet) {
  walletReadyCache.delete(wallet.address)
  await wallet.disconnect()
}

export async function readStrkBalance(wallet: MorosWallet) {
  const balance = await wallet.balanceOf(getStrkToken())
  return {
    formatted: balance.toFormatted(),
    unit: balance.toUnit(),
  }
}

export async function fundMorosBankroll(wallet: MorosWallet, amountInput: string) {
  if (!morosConfig.bankrollVault) {
    throw new Error('VITE_MOROS_BANKROLL_VAULT_ADDRESS is not configured.')
  }

  const token = getStrkToken()
  const amount = Amount.parse(amountInput, token)
  if (amount.isZero()) {
    throw new Error('Funding amount must be greater than zero.')
  }

  await prepareMorosWalletForExecution(wallet)

  return executeWithReadyRetry(wallet, () =>
    wallet.execute(
      buildVaultFundingCalls(wallet, amount.toBase()),
      {
        feeMode: getFeeMode(),
      },
    ),
  )
}

export async function registerMorosGameplaySession(
  wallet: MorosWallet,
  params: {
    sessionKeyAddress: string
    maxWagerWei: string
    expiresAtUnix: number
  },
) {
  if (!morosConfig.sessionRegistry) {
    throw new Error('VITE_MOROS_SESSION_REGISTRY_ADDRESS is not configured.')
  }
  if (!params.sessionKeyAddress) {
    throw new Error('VITE_MOROS_GAMEPLAY_SESSION_KEY_ADDRESS is not configured.')
  }

  await prepareMorosWalletForExecution(wallet)

  return executeWithReadyRetry(wallet, () =>
    wallet.execute(
      [
        {
          contractAddress: morosConfig.sessionRegistry as Address,
          entrypoint: 'register_session_key',
          calldata: [
            wallet.address as Address,
            params.sessionKeyAddress as Address,
            params.maxWagerWei,
            params.expiresAtUnix.toString(),
          ],
        },
      ],
      {
        feeMode: getFeeMode(),
      },
    ),
  )
}

export async function withdrawMorosBankroll(
  wallet: MorosWallet,
  amountInput: string,
  sourceBalance: 'gambling' | 'vault' = 'vault',
  recipientAddress?: string,
) {
  if (!morosConfig.bankrollVault) {
    throw new Error('VITE_MOROS_BANKROLL_VAULT_ADDRESS is not configured.')
  }

  const token = getStrkToken()
  const amount = Amount.parse(amountInput, token)
  if (amount.isZero()) {
    throw new Error('Withdrawal amount must be greater than zero.')
  }

  await prepareMorosWalletForExecution(wallet)
  const recipient = (recipientAddress?.trim() || wallet.address) as Address
  const entrypoint = sourceBalance === 'gambling' ? 'withdraw_public' : 'withdraw_from_vault'

  return executeWithReadyRetry(wallet, () =>
    wallet.execute(
      [
        {
          contractAddress: morosConfig.bankrollVault as Address,
          entrypoint,
          calldata: [recipient, amount.toBase().toString()],
        },
      ],
      {
        feeMode: getFeeMode(),
      },
    ),
  )
}

function splitUint256(value: bigint) {
  return {
    low: (value & UINT128_MASK).toString(),
    high: (value >> 128n).toString(),
  }
}

function buildVaultFundingCalls(wallet: MorosWallet, amountBase: bigint): MorosExecuteCall[] {
  if (amountBase <= 0n) {
    return []
  }

  if (!morosConfig.bankrollVault) {
    throw new Error('VITE_MOROS_BANKROLL_VAULT_ADDRESS is not configured.')
  }

  const amount = splitUint256(amountBase)

  return [
    {
      contractAddress: STRK_TOKEN.address,
      entrypoint: 'approve',
      calldata: [morosConfig.bankrollVault as Address, amount.low, amount.high],
    },
    {
      contractAddress: morosConfig.bankrollVault as Address,
      entrypoint: 'deposit_public',
      calldata: [wallet.address as Address, amountBase.toString()],
    },
  ]
}

function getBankrollShortfall(requiredWei: string, bankrollBalanceWei?: string) {
  if (!bankrollBalanceWei) {
    return BigInt(requiredWei)
  }

  const required = BigInt(requiredWei)
  const bankrollBalance = BigInt(bankrollBalanceWei)
  if (required <= bankrollBalance) {
    return 0n
  }
  return required - bankrollBalance
}

export async function openMorosDiceRound(wallet: MorosWallet, payload: OpenDiceRoundPayload) {
  if (!morosConfig.diceTable) {
    throw new Error('VITE_MOROS_DICE_TABLE_ADDRESS is not configured.')
  }

  const fundingCalls = buildVaultFundingCalls(wallet, getBankrollShortfall(payload.wagerWei, payload.bankrollBalanceWei))

  await prepareMorosWalletForExecution(wallet)

  return executeWithReadyRetry(wallet, () =>
    wallet.execute(
      [
        ...fundingCalls,
        {
          contractAddress: morosConfig.diceTable as Address,
          entrypoint: 'open_round',
          calldata: [
            payload.tableId.toString(),
            wallet.address as Address,
            wallet.address as Address,
            payload.wagerWei,
            payload.targetBps.toString(),
            payload.rollOver ? '1' : '0',
            payload.clientSeed,
            payload.commitmentId.toString(),
          ],
        },
      ],
      {
        feeMode: getFeeMode(),
      },
    ),
  )
}

export async function openMorosRouletteSpin(wallet: MorosWallet, payload: OpenRouletteSpinPayload) {
  if (!morosConfig.rouletteTable) {
    throw new Error('VITE_MOROS_ROULETTE_TABLE_ADDRESS is not configured.')
  }
  if (payload.bets.length === 0 || payload.bets.length > 8) {
    throw new Error('Roulette supports 1-8 bets per spin.')
  }

  const fundingCalls = buildVaultFundingCalls(wallet, getBankrollShortfall(payload.totalWagerWei, payload.bankrollBalanceWei))
  const padded = Array.from({ length: 8 }, (_, index) => payload.bets[index] ?? {
    kind: 0,
    selection: 0,
    amountWei: '0',
  })

  await prepareMorosWalletForExecution(wallet)

  return executeWithReadyRetry(wallet, () =>
    wallet.execute(
      [
        ...fundingCalls,
        {
          contractAddress: morosConfig.rouletteTable as Address,
          entrypoint: 'open_spin',
          calldata: [
            payload.tableId.toString(),
            wallet.address as Address,
            wallet.address as Address,
            payload.totalWagerWei,
            payload.clientSeed,
            payload.commitmentId.toString(),
            payload.bets.length.toString(),
            ...padded.flatMap((bet) => [bet.kind.toString(), bet.selection.toString(), bet.amountWei]),
          ],
        },
      ],
      {
        feeMode: getFeeMode(),
      },
    ),
  )
}

export async function openMorosBaccaratRound(wallet: MorosWallet, payload: OpenBaccaratRoundPayload) {
  if (!morosConfig.baccaratTable) {
    throw new Error('VITE_MOROS_BACCARAT_TABLE_ADDRESS is not configured.')
  }

  const fundingCalls = buildVaultFundingCalls(wallet, getBankrollShortfall(payload.wagerWei, payload.bankrollBalanceWei))

  await prepareMorosWalletForExecution(wallet)

  return executeWithReadyRetry(wallet, () =>
    wallet.execute(
      [
        ...fundingCalls,
        {
          contractAddress: morosConfig.baccaratTable as Address,
          entrypoint: 'open_round',
          calldata: [
            payload.tableId.toString(),
            wallet.address as Address,
            wallet.address as Address,
            payload.wagerWei,
            payload.betSide.toString(),
            payload.clientSeed,
            payload.commitmentId.toString(),
          ],
        },
      ],
      {
        feeMode: getFeeMode(),
      },
    ),
  )
}

export function getStarkzapRail(): StarkzapRuntimePlan {
  return {
    network: resolveNetwork(morosConfig.network),
    walletModes: ['external', 'privy', 'signer'],
    liveWalletModes: ['external', 'privy'],
    modules: [
      'wallet onboarding',
      'balance reads',
      'erc20 approve + vault deposit',
      'vault withdraw',
      'Ekubo swap rail registered on the connected wallet',
      'Ethereum bridge funding rail',
      'transaction builder',
    ],
    note: 'Moros uses StarkZap for Privy-backed Moros wallets, external Starknet wallet execution, Ekubo swap routing, Ethereum bridge funding, and live STRK bankroll deposit and withdraw flows.',
  }
}
