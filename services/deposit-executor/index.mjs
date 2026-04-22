import http from 'node:http'
import { URL } from 'node:url'
import { JsonRpcProvider, keccak256, toQuantity, Wallet as EthersWallet } from 'ethers'
import {
  Amount,
  ConnectedSolanaWallet,
  EkuboSwapProvider,
  ExternalChain,
  SolanaNetwork,
  StarkSigner,
  Wallet,
  networks,
} from 'starkzap'
import { RpcProvider } from 'starknet'

const HOST = process.env.MOROS_DEPOSIT_EXECUTOR_HOST ?? process.env.HOST ?? '127.0.0.1'
const PORT = Number.parseInt(process.env.MOROS_DEPOSIT_EXECUTOR_PORT ?? process.env.PORT ?? '18085', 10)

const ROUTER_URL = process.env.MOROS_DEPOSIT_ROUTER_URL ?? 'http://127.0.0.1:8084'
const EXECUTOR_TOKEN = process.env.MOROS_DEPOSIT_EXECUTOR_TOKEN
const MASTER_SECRET = process.env.MOROS_DEPOSIT_MASTER_SECRET
const BANKROLL_VAULT = process.env.MOROS_BANKROLL_VAULT_ADDRESS
const STRK_TOKEN =
  process.env.MOROS_STRK_TOKEN_ADDRESS ??
  '0x04718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d'
const STARKNET_PRIVATE_KEY = process.env.MOROS_DEPOSIT_ROUTE_STARKNET_PRIVATE_KEY
const STARKNET_ACCOUNT_ADDRESS = process.env.MOROS_DEPOSIT_ROUTE_STARKNET_ACCOUNT_ADDRESS
const STARKNET_CHAIN = resolveNetwork(
  process.env.MOROS_DEPOSIT_ROUTE_STARKNET_CHAIN ?? process.env.MOROS_STARKNET_CHAIN,
)
const STARKNET_RPC_URL =
  process.env.MOROS_DEPOSIT_ROUTE_STARKNET_RPC_URL ??
  process.env.MOROS_STARKNET_RPC_URL ??
  networks[STARKNET_CHAIN].rpcUrl
const HOUSE_STARKNET_ACCOUNT_ADDRESS = process.env.MOROS_STARKNET_ACCOUNT_ADDRESS
const SOURCE_RPC_URLS = parseJsonEnv('MOROS_DEPOSIT_RPC_URLS', {})
const LAYER_ZERO_API_KEY = process.env.MOROS_DEPOSIT_LAYER_ZERO_API_KEY
const ROUTE_TIMEOUT_MS = parseIntegerEnv('MOROS_DEPOSIT_ROUTE_TIMEOUT_MS', 20 * 60 * 1000)
const ROUTE_POLL_INTERVAL_MS = parseIntegerEnv('MOROS_DEPOSIT_ROUTE_POLL_INTERVAL_MS', 15_000)
const SWAP_SLIPPAGE_BPS = BigInt(parseIntegerEnv('MOROS_DEPOSIT_SWAP_SLIPPAGE_BPS', 100))
const EVM_GAS_BUFFER_BPS = BigInt(parseIntegerEnv('MOROS_DEPOSIT_EVM_GAS_BUFFER_BPS', 2000))
const EVM_GAS_MIN_MARGIN_WEI = BigInt(
  process.env.MOROS_DEPOSIT_EVM_GAS_MIN_MARGIN_WEI ?? '100000000000000',
)
const EVM_GAS_SPONSOR_PRIVATE_KEY = process.env.MOROS_DEPOSIT_EVM_GAS_SPONSOR_PRIVATE_KEY
const STARKNET_FEE_BUFFER_RAW = BigInt(
  process.env.MOROS_DEPOSIT_STARKNET_FEE_BUFFER_RAW ?? '100000000000000000',
)
const STARKNET_DEPLOY_BUFFER_RAW = BigInt(
  process.env.MOROS_DEPOSIT_STARKNET_DEPLOY_BUFFER_RAW ?? '100000000000000000',
)
const SOLANA_NATIVE_FEE_BUFFER_LAMPORTS = BigInt(
  process.env.MOROS_DEPOSIT_SOLANA_NATIVE_FEE_BUFFER_LAMPORTS ?? '6000000',
)
const SOLANA_FEE_MARGIN_LAMPORTS = BigInt(
  process.env.MOROS_DEPOSIT_SOLANA_FEE_MARGIN_LAMPORTS ?? '250000',
)
const DEBUG_ROUTE_ERRORS = process.env.MOROS_DEPOSIT_EXECUTOR_DEBUG_ERRORS === '1'
const STARKNET_CURVE_ORDER = BigInt(
  '0x0800000000000011000000000000000000000000000000000000000000000001',
)

const activeJobs = new Map()
let routeWalletPromise = null
let sdkPromise = null
let ethereumBridgeTokenCachePromise = null
let solanaBridgeTokenCachePromise = null
let solanaWeb3Promise = null

function resolveNetwork(value) {
  if (value === 'mainnet' || value === 'devnet') {
    return value
  }
  return 'sepolia'
}

function parseIntegerEnv(key, fallback) {
  const value = Number.parseInt(process.env[key] ?? '', 10)
  return Number.isFinite(value) && value > 0 ? value : fallback
}

function parseJsonEnv(key, fallback) {
  const raw = process.env[key]
  if (!raw) {
    return fallback
  }
  try {
    return JSON.parse(raw)
  } catch {
    return fallback
  }
}

function keccakDigest(parts) {
  const input = Buffer.concat(
    parts.map((part) => {
      if (part instanceof Uint8Array) {
        return Buffer.from(part)
      }
      return Buffer.from(String(part), 'utf8')
    }),
  )
  return Buffer.from(keccak256(input).slice(2), 'hex')
}

function sendJson(response, statusCode, payload) {
  response.writeHead(statusCode, {
    'access-control-allow-headers': 'content-type, x-moros-executor-token',
    'access-control-allow-methods': 'GET, POST, OPTIONS',
    'access-control-allow-origin': '*',
    'content-type': 'application/json; charset=utf-8',
  })
  response.end(JSON.stringify(payload))
}

function isRouteJobPath(pathname) {
  return pathname === '/v1/route-jobs' || pathname === '/route-jobs' || pathname === '/'
}

async function readJson(request) {
  const chunks = []
  for await (const chunk of request) {
    chunks.push(chunk)
  }
  if (chunks.length === 0) {
    return {}
  }
  const raw = Buffer.concat(chunks).toString('utf8')
  return raw ? JSON.parse(raw) : {}
}

function requireConfig() {
  if (!EXECUTOR_TOKEN) {
    const error = new Error('MOROS_DEPOSIT_EXECUTOR_TOKEN is not configured.')
    error.statusCode = 503
    throw error
  }
  if (!MASTER_SECRET) {
    const error = new Error('MOROS_DEPOSIT_MASTER_SECRET is not configured.')
    error.statusCode = 503
    throw error
  }
  if (!BANKROLL_VAULT) {
    const error = new Error('MOROS_BANKROLL_VAULT_ADDRESS is not configured.')
    error.statusCode = 503
    throw error
  }
  if (!STARKNET_PRIVATE_KEY || !STARKNET_ACCOUNT_ADDRESS) {
    const error = new Error('Dedicated route Starknet wallet credentials are not configured.')
    error.statusCode = 503
    throw error
  }
}

function assertExecutorToken(request) {
  requireConfig()
  const provided = request.headers['x-moros-executor-token']
  if (provided !== EXECUTOR_TOKEN) {
    const error = new Error('invalid x-moros-executor-token')
    error.statusCode = 401
    throw error
  }
}

function normalizeEvmAddress(value) {
  const normalized = String(value ?? '').trim().toLowerCase()
  return normalized.startsWith('0x') ? normalized : `0x${normalized}`
}

function normalizeStarknetAddress(value) {
  const normalized = String(value ?? '').trim().toLowerCase()
  if (!normalized) {
    return '0x0'
  }
  const prefixed = normalized.startsWith('0x') ? normalized : `0x${normalized}`
  try {
    return `0x${BigInt(prefixed).toString(16)}`
  } catch {
    return prefixed
  }
}

function normalizeSolanaAddress(value) {
  return String(value ?? '').trim()
}

function normalizeQuantity(value) {
  if (value == null) {
    return undefined
  }
  if (typeof value === 'string' && value.startsWith('0x')) {
    return value
  }
  return toQuantity(BigInt(value))
}

function serializeEvmTransactionRequest(tx) {
  return {
    ...(tx.to ? { to: String(tx.to) } : {}),
    ...(tx.from ? { from: String(tx.from) } : {}),
    ...(tx.data ? { data: String(tx.data) } : {}),
    ...(tx.value != null ? { value: normalizeQuantity(tx.value) } : {}),
    ...(tx.nonce != null ? { nonce: normalizeQuantity(tx.nonce) } : {}),
    ...(tx.gasLimit != null ? { gas_limit: normalizeQuantity(tx.gasLimit) } : {}),
    ...(tx.gasPrice != null ? { gas_price: normalizeQuantity(tx.gasPrice) } : {}),
    ...(tx.maxFeePerGas != null ? { max_fee_per_gas: normalizeQuantity(tx.maxFeePerGas) } : {}),
    ...(tx.maxPriorityFeePerGas != null
      ? { max_priority_fee_per_gas: normalizeQuantity(tx.maxPriorityFeePerGas) }
      : {}),
    ...(tx.chainId != null ? { chain_id: normalizeQuantity(tx.chainId) } : {}),
    ...(tx.type != null ? { type: Number(tx.type) } : {}),
  }
}

function isDuplicateStarknetDeploySubmission(error) {
  const message = error instanceof Error ? error.message : String(error ?? '')
  return (
    message.toLowerCase().includes('same hash already exists in the mempool') ||
    message.toLowerCase().includes('duplicate transaction')
  )
}

async function ensureStarknetWalletReady(wallet, options) {
  try {
    await wallet.ensureReady(options)
    return
  } catch (error) {
    if (!isDuplicateStarknetDeploySubmission(error)) {
      throw error
    }
  }

  const deadline = Date.now() + ROUTE_TIMEOUT_MS
  while (Date.now() < deadline) {
    if (await wallet.isDeployed()) {
      return
    }
    await sleep(ROUTE_POLL_INTERVAL_MS)
  }

  throw new Error(`Timed out waiting for pending Starknet account deployment at ${wallet.address}.`)
}

function isStarknetExecutionAddress(value) {
  const normalized = String(value ?? '').trim()
  return /^0x[0-9a-f]+$/i.test(normalized)
}

function splitUint256(value) {
  const amount = BigInt(value)
  const mask = (1n << 128n) - 1n
  return {
    low: (amount & mask).toString(),
    high: (amount >> 128n).toString(),
  }
}

function deriveEvmPrivateKey(masterSecret, destinationAccountKey, chainKey) {
  for (let nonce = 0; nonce < 1024; nonce += 1) {
    const nonceBuffer = Buffer.alloc(4)
    nonceBuffer.writeUInt32BE(nonce, 0)
    const digest = keccakDigest([
      masterSecret,
      Buffer.from([0]),
      String(destinationAccountKey).toLowerCase(),
      Buffer.from([0]),
      chainKey,
      Buffer.from([0]),
      nonceBuffer,
    ])
    const candidate = `0x${digest.toString('hex')}`
    try {
      const wallet = new EthersWallet(candidate)
      return {
        privateKey: candidate,
        address: wallet.address.toLowerCase(),
      }
    } catch {
      // Continue until we get a valid secp256k1 key.
    }
  }

  throw new Error('Failed to derive a valid EVM custody key.')
}

function deriveStarknetPrivateKey(masterSecret, destinationAccountKey, chainKey) {
  const digest = keccakDigest([
    masterSecret,
    Buffer.from([0]),
    String(destinationAccountKey).toLowerCase(),
    Buffer.from([0]),
    chainKey,
  ])
  let scalar = BigInt(`0x${digest.toString('hex')}`) % STARKNET_CURVE_ORDER
  if (scalar === 0n) {
    scalar = 1n
  }
  return `0x${scalar.toString(16)}`
}

function deriveSolanaSeed(masterSecret, destinationAccountKey, chainKey) {
  return keccakDigest([
    masterSecret,
    Buffer.from([0]),
    String(destinationAccountKey).toLowerCase(),
    Buffer.from([0]),
    chainKey,
  ]).subarray(0, 32)
}

function resolveSolanaSourceChainKey() {
  return STARKNET_CHAIN === 'mainnet' ? 'solana-mainnet' : 'solana-testnet'
}

function defaultSolanaRpcUrl(chainKey) {
  return chainKey === 'solana-mainnet'
    ? 'https://api.mainnet-beta.solana.com'
    : 'https://api.testnet.solana.com'
}

function resolveSolanaRpcUrl(chainKey = resolveSolanaSourceChainKey()) {
  return SOURCE_RPC_URLS[chainKey] ?? defaultSolanaRpcUrl(chainKey)
}

async function getSdk() {
  if (!sdkPromise) {
    sdkPromise = import('starkzap').then(({ StarkZap }) => {
      return new StarkZap({
        network: STARKNET_CHAIN,
        bridging: {
          ...(SOURCE_RPC_URLS['ethereum-mainnet']
            ? { ethereumRpcUrl: SOURCE_RPC_URLS['ethereum-mainnet'] }
            : {}),
          ...(resolveSolanaRpcUrl() ? { solanaRpcUrl: resolveSolanaRpcUrl() } : {}),
          ...(LAYER_ZERO_API_KEY ? { layerZeroApiKey: LAYER_ZERO_API_KEY } : {}),
        },
      })
    })
  }

  return sdkPromise
}

async function getEthereumBridgeTokens() {
  if (!ethereumBridgeTokenCachePromise) {
    ethereumBridgeTokenCachePromise = getSdk().then((sdk) =>
      sdk.getBridgingTokens(ExternalChain.ETHEREUM),
    )
  }
  return ethereumBridgeTokenCachePromise
}

async function getSolanaBridgeTokens() {
  if (!solanaBridgeTokenCachePromise) {
    solanaBridgeTokenCachePromise = getSdk().then((sdk) =>
      sdk.getBridgingTokens(ExternalChain.SOLANA),
    )
  }
  return solanaBridgeTokenCachePromise
}

async function getSolanaWeb3() {
  if (!solanaWeb3Promise) {
    solanaWeb3Promise = import('@solana/web3.js')
  }
  return solanaWeb3Promise
}

async function getRouteWallet() {
  if (!routeWalletPromise) {
    routeWalletPromise = (async () => {
      const provider = new RpcProvider({ nodeUrl: STARKNET_RPC_URL })
      const wallet = await Wallet.create({
        account: {
          signer: new StarkSigner(STARKNET_PRIVATE_KEY),
        },
        accountAddress: STARKNET_ACCOUNT_ADDRESS,
        provider,
        config: {
          rpcUrl: STARKNET_RPC_URL,
          chainId: networks[STARKNET_CHAIN].chainId,
          ...(networks[STARKNET_CHAIN].explorerUrl
            ? {
                explorer: {
                  baseUrl: networks[STARKNET_CHAIN].explorerUrl,
                },
              }
            : {}),
          bridging: {
            ...(SOURCE_RPC_URLS['ethereum-mainnet']
              ? { ethereumRpcUrl: SOURCE_RPC_URLS['ethereum-mainnet'] }
              : {}),
            ...(resolveSolanaRpcUrl() ? { solanaRpcUrl: resolveSolanaRpcUrl() } : {}),
            ...(LAYER_ZERO_API_KEY ? { layerZeroApiKey: LAYER_ZERO_API_KEY } : {}),
          },
        },
        feeMode: 'user_pays',
      })
      wallet.registerSwapProvider(new EkuboSwapProvider(), true)
      await ensureStarknetWalletReady(wallet, {
        deploy: 'if_needed',
        feeMode: 'user_pays',
      })
      return wallet
    })()
  }
  return routeWalletPromise
}

async function createDerivedStarknetSourceWallet(privateKey) {
  const provider = new RpcProvider({ nodeUrl: STARKNET_RPC_URL })
  return Wallet.create({
    account: {
      signer: new StarkSigner(privateKey),
    },
    provider,
    config: {
      rpcUrl: STARKNET_RPC_URL,
      chainId: networks[STARKNET_CHAIN].chainId,
      ...(networks[STARKNET_CHAIN].explorerUrl
        ? {
            explorer: {
              baseUrl: networks[STARKNET_CHAIN].explorerUrl,
            },
          }
        : {}),
      bridging: {
        ...(SOURCE_RPC_URLS['ethereum-mainnet']
          ? { ethereumRpcUrl: SOURCE_RPC_URLS['ethereum-mainnet'] }
          : {}),
        ...(resolveSolanaRpcUrl() ? { solanaRpcUrl: resolveSolanaRpcUrl() } : {}),
        ...(LAYER_ZERO_API_KEY ? { layerZeroApiKey: LAYER_ZERO_API_KEY } : {}),
      },
    },
    feeMode: 'user_pays',
  })
}

function findBridgeToken(tokens, payload) {
  const assetAddress = normalizeEvmAddress(payload.source.asset_address)
  const assetId = String(payload.source.asset_id).toLowerCase()
  const assetSymbol = String(payload.source.asset_symbol ?? '').toUpperCase()

  return (
    tokens.find((token) => normalizeEvmAddress(token.address) === assetAddress) ??
    tokens.find((token) => token.id.toLowerCase() === assetId) ??
    tokens.find((token) => token.symbol.toUpperCase() === assetSymbol)
  )
}

function findSolanaBridgeToken(tokens, payload) {
  const assetAddress = normalizeSolanaAddress(payload.source.asset_address)
  const assetId = String(payload.source.asset_id).toLowerCase()
  const assetSymbol = String(payload.source.asset_symbol ?? '').toUpperCase()

  return (
    tokens.find((token) => normalizeSolanaAddress(token.address) === assetAddress) ??
    tokens.find((token) => token.id.toLowerCase() === assetId) ??
    tokens.find((token) => token.symbol.toUpperCase() === assetSymbol)
  )
}

async function deriveSolanaKeypair(masterSecret, destinationAccountKey, chainKey) {
  const solanaWeb3 = await getSolanaWeb3()
  return solanaWeb3.Keypair.fromSeed(
    deriveSolanaSeed(masterSecret, destinationAccountKey, chainKey),
  )
}

function resolveSolanaNetwork(chainKey) {
  return chainKey === 'solana-mainnet' ? SolanaNetwork.MAINNET : SolanaNetwork.TESTNET
}

function createDeterministicSolanaProvider(connection, keypair) {
  return {
    async signAndSendTransaction(transaction) {
      const requiredSigners = readSolanaRequiredSigners(transaction)
      const signerAddress = keypair?.publicKey?.toBase58?.() ?? keypair?.publicKey?.toString?.()
      if (!signerAddress) {
        throw new Error('Derived Solana signer is missing a public key.')
      }
      if (requiredSigners.length > 0 && !requiredSigners.includes(signerAddress)) {
        throw new Error(
          `Solana bridge transaction does not require the derived deposit signer ${signerAddress}; required signers: ${requiredSigners.join(', ')}`,
        )
      }
      if (typeof transaction?.sign === 'function') {
        if (transaction.constructor?.name === 'VersionedTransaction') {
          transaction.sign([keypair])
        } else if (typeof transaction.partialSign === 'function') {
          transaction.partialSign(keypair)
        } else {
          transaction.sign(keypair)
        }
      } else if (typeof transaction?.partialSign === 'function') {
        transaction.partialSign(keypair)
      } else {
        throw new Error('Unsupported Solana transaction type.')
      }

      const signature = await connection.sendRawTransaction(transaction.serialize(), {
        skipPreflight: false,
        preflightCommitment: 'confirmed',
      })
      await connection.confirmTransaction(signature, 'confirmed')
      return signature
    },
  }
}

function readSolanaRequiredSigners(transaction) {
  if (Array.isArray(transaction?.signatures)) {
    return transaction.signatures
      .map((signature) => signature?.publicKey?.toBase58?.() ?? signature?.publicKey?.toString?.())
      .filter(Boolean)
  }
  const staticKeys = transaction?.message?.staticAccountKeys
  const requiredCount = transaction?.message?.header?.numRequiredSignatures
  if (Array.isArray(staticKeys) && Number.isInteger(requiredCount)) {
    return staticKeys
      .slice(0, requiredCount)
      .map((publicKey) => publicKey?.toBase58?.() ?? publicKey?.toString?.())
      .filter(Boolean)
  }
  return []
}

async function createEthereumBridge(bridgeToken, signer, routeWallet, ethereumRpcUrl) {
  const walletConfig = {
    provider: new JsonRpcProvider(ethereumRpcUrl),
    signer,
  }

  if (bridgeToken.id === 'lords') {
    const { LordsBridge } = await import(
      'starkzap/dist/src/bridge/ethereum/lords/LordsBridge.js'
    )
    return new LordsBridge(bridgeToken, walletConfig, routeWallet)
  }

  switch (bridgeToken.protocol) {
    case 'canonical': {
      const { CanonicalEthereumBridge } = await import(
        'starkzap/dist/src/bridge/ethereum/canonical/CanonicalEthereumBridge.js'
      )
      return new CanonicalEthereumBridge(bridgeToken, walletConfig, routeWallet)
    }
    case 'cctp': {
      const { CCTPBridge } = await import(
        'starkzap/dist/src/bridge/ethereum/cctp/CCTPBridge.js'
      )
      return new CCTPBridge(bridgeToken, walletConfig, routeWallet)
    }
    case 'oft':
    case 'oft-migrated': {
      const { OftBridge } = await import('starkzap/dist/src/bridge/ethereum/oft/OftBridge.js')
      if (!LAYER_ZERO_API_KEY) {
        throw new Error('MOROS_DEPOSIT_LAYER_ZERO_API_KEY is required for OFT routes.')
      }
      return new OftBridge(bridgeToken, walletConfig, routeWallet, LAYER_ZERO_API_KEY)
    }
    default:
      throw new Error(`Unsupported Ethereum bridge protocol "${bridgeToken.protocol}".`)
  }
}

function makeStarknetToken(params) {
  return {
    name: params.name,
    address: params.address,
    decimals: params.decimals,
    symbol: params.symbol,
  }
}

async function readBalanceRaw(wallet, token) {
  const balance = await wallet.balanceOf(token)
  return balance.toBase()
}

async function waitForEthereumReceipt(provider, hash) {
  const deadline = Date.now() + ROUTE_TIMEOUT_MS
  while (Date.now() < deadline) {
    const receipt = await provider.getTransactionReceipt(hash)
    if (receipt) {
      if (receipt.status !== 1n && receipt.status !== 1) {
        throw new Error(`Ethereum bridge transaction ${hash} reverted.`)
      }
      return receipt
    }
    await sleep(ROUTE_POLL_INTERVAL_MS)
  }
  throw new Error(`Timed out waiting for Ethereum transaction ${hash}.`)
}

async function waitForSolanaSignature(connection, signature) {
  const deadline = Date.now() + ROUTE_TIMEOUT_MS
  while (Date.now() < deadline) {
    const response = await connection.getSignatureStatuses([signature], {
      searchTransactionHistory: true,
    })
    const status = response?.value?.[0]
    if (status) {
      if (status.err) {
        throw new Error(`Solana bridge transaction ${signature} failed.`)
      }
      if (status.confirmationStatus === 'confirmed' || status.confirmationStatus === 'finalized') {
        return status
      }
    }
    await sleep(ROUTE_POLL_INTERVAL_MS)
  }
  throw new Error(`Timed out waiting for Solana transaction ${signature}.`)
}

async function waitForBalanceIncrease(wallet, token, startBalance, minimumIncrease) {
  const deadline = Date.now() + ROUTE_TIMEOUT_MS
  while (Date.now() < deadline) {
    const current = await readBalanceRaw(wallet, token)
    const delta = current - startBalance
    if (delta >= minimumIncrease) {
      return {
        current,
        delta,
      }
    }
    await sleep(ROUTE_POLL_INTERVAL_MS)
  }
  throw new Error(`Timed out waiting for Starknet ${token.symbol} bridge arrival.`)
}

async function transferStarknetToken(wallet, tokenAddress, recipient, amountRaw) {
  const uint256Amount = splitUint256(amountRaw)
  return wallet.execute(
    [
      {
        contractAddress: tokenAddress,
        entrypoint: 'transfer',
        calldata: [recipient, uint256Amount.low, uint256Amount.high],
      },
    ],
    {
      feeMode: 'user_pays',
    },
  )
}

async function readStarknetTokenBalanceRaw(tokenAddress, holderAddress) {
  const provider = new RpcProvider({ nodeUrl: STARKNET_RPC_URL })
  const response = await provider.callContract({
    contractAddress: tokenAddress,
    entrypoint: 'balanceOf',
    calldata: [holderAddress],
  })
  const [low = '0x0', high = '0x0'] = response ?? []
  return BigInt(low) + (BigInt(high) << 128n)
}

async function waitForStarknetTokenBalance(tokenAddress, holderAddress, minimumBalanceRaw) {
  const deadline = Date.now() + ROUTE_TIMEOUT_MS
  while (Date.now() < deadline) {
    const current = await readStarknetTokenBalanceRaw(tokenAddress, holderAddress)
    if (current >= minimumBalanceRaw) {
      return current
    }
    await sleep(ROUTE_POLL_INTERVAL_MS)
  }

  throw new Error(
    `Timed out waiting for Starknet token balance at ${holderAddress} to reach ${minimumBalanceRaw.toString()}.`,
  )
}

function getRequiredStarknetSourceBalanceRaw(
  amountRaw,
  sourceAssetAddress,
  extraReserveRaw = 0n,
) {
  const normalizedSourceAsset = normalizeStarknetAddress(sourceAssetAddress)
  const transferPrincipal =
    normalizedSourceAsset === normalizeStarknetAddress(STRK_TOKEN) ? amountRaw : 0n
  return transferPrincipal + STARKNET_FEE_BUFFER_RAW + BigInt(extraReserveRaw)
}

async function ensureSourceFeeBuffer(
  routeWallet,
  sourceWallet,
  amountRaw,
  sourceAssetAddress,
  extraReserveRaw = 0n,
) {
  const strkToken = makeStarknetToken({
    name: 'STRK',
    address: STRK_TOKEN,
    decimals: 18,
    symbol: 'STRK',
  })
  const currentStrkBalance = await readBalanceRaw(sourceWallet, strkToken)
  const requiredBalance = getRequiredStarknetSourceBalanceRaw(
    amountRaw,
    sourceAssetAddress,
    extraReserveRaw,
  )

  if (currentStrkBalance >= requiredBalance) {
    return null
  }

  const topUpAmount = requiredBalance - currentStrkBalance
  const topUpTx = await transferStarknetToken(routeWallet, STRK_TOKEN, sourceWallet.address, topUpAmount)
  await topUpTx.wait()
  await waitForStarknetTokenBalance(STRK_TOKEN, sourceWallet.address, requiredBalance)
  return {
    hash: topUpTx.hash,
    amountRaw: topUpAmount.toString(),
  }
}

async function submitBankrollCredit(job, routeWallet, creditedAmountRaw, stagePayload = {}) {
  const recipient = job.payload?.destination?.wallet_address
  if (!isStarknetExecutionAddress(recipient)) {
    throw new Error('Deposit route is missing a valid Starknet beneficiary wallet.')
  }

  const uint256Amount = splitUint256(creditedAmountRaw)
  const bankrollTx = await routeWallet.execute(
    [
      {
        contractAddress: STRK_TOKEN,
        entrypoint: 'approve',
        calldata: [BANKROLL_VAULT, uint256Amount.low, uint256Amount.high],
      },
      {
        contractAddress: BANKROLL_VAULT,
        entrypoint: 'deposit_public',
        calldata: [recipient, creditedAmountRaw.toString()],
      },
    ],
    {
      feeMode: 'user_pays',
    },
  )
  await callbackRouter(job.job_id, {
    status: 'processing',
    response: {
      stage: 'bankroll_credit_submitted',
      vault_entrypoint: 'deposit_public',
      credit_scope: 'gambling',
      bankroll_tx_hash: bankrollTx.hash,
      credited_amount_raw: creditedAmountRaw.toString(),
      ...stagePayload,
    },
  })
  await bankrollTx.wait()
  await callbackRouter(job.job_id, {
    status: 'completed',
    destination_tx_hash: bankrollTx.hash,
    response: {
      bankroll_tx_hash: bankrollTx.hash,
      credited_amount_raw: creditedAmountRaw.toString(),
      recipient,
      vault_entrypoint: 'deposit_public',
      credit_scope: 'gambling',
      destination_asset_symbol: 'STRK',
      ...stagePayload,
    },
  })
}

async function callbackRouter(jobId, payload) {
  if (!EXECUTOR_TOKEN) {
    return
  }

  const response = await fetch(`${ROUTER_URL}/v1/deposits/route-jobs/${jobId}/callback`, {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
      'x-moros-executor-token': EXECUTOR_TOKEN,
    },
    body: JSON.stringify(payload),
  })

  if (!response.ok) {
    throw new Error(`Router callback failed with ${response.status}: ${await response.text()}`)
  }
}

function amountToRaw(value) {
  return BigInt(value.toBase())
}

function addBufferedMargin(amountRaw, bufferBps, minimumMarginRaw = 0n) {
  const percentageMargin = (amountRaw * bufferBps) / 10_000n
  return amountRaw + (percentageMargin > minimumMarginRaw ? percentageMargin : minimumMarginRaw)
}

function evmSourceAssetUsesNativeGas(job) {
  const assetId = String(job.payload?.source?.asset_id ?? '').toLowerCase()
  const assetSymbol = String(job.payload?.source?.asset_symbol ?? '').toUpperCase()
  return assetId === 'eth' || assetSymbol === 'ETH'
}

async function estimateEthereumExecutionReserveWei(bridge) {
  const estimate = await bridge.getDepositFeeEstimate()
  const approvalFee = amountToRaw(estimate.approvalFee)
  const l1Fee = amountToRaw(estimate.l1Fee)
  const l2Fee = amountToRaw(estimate.l2Fee)
  return addBufferedMargin(approvalFee + l1Fee + l2Fee, EVM_GAS_BUFFER_BPS, EVM_GAS_MIN_MARGIN_WEI)
}

async function topUpEthereumGasIfNeeded(provider, recipient, requiredWei) {
  const currentBalance = await provider.getBalance(recipient)
  if (currentBalance >= requiredWei) {
    return null
  }
  if (!EVM_GAS_SPONSOR_PRIVATE_KEY) {
    throw new Error(
      'MOROS_DEPOSIT_EVM_GAS_SPONSOR_PRIVATE_KEY is required to route ERC-20 Ethereum deposits through deterministic route wallets.',
    )
  }

  const sponsorWallet = new EthersWallet(EVM_GAS_SPONSOR_PRIVATE_KEY, provider)
  const topUpAmount = requiredWei - currentBalance
  const tx = await sponsorWallet.sendTransaction({
    to: recipient,
    value: topUpAmount,
  })
  await tx.wait()
  return {
    hash: tx.hash,
    amountRaw: topUpAmount.toString(),
  }
}

function isSwapRouteUnavailable(error) {
  const message = error instanceof Error ? error.message : String(error ?? '')
  const normalized = message.toLowerCase()
  return (
    normalized.includes('insufficient liquidity') ||
    normalized.includes('returned no routes') ||
    normalized.includes('quote failed') ||
    normalized.includes('no routes') ||
    normalized.includes('no quote')
  )
}

async function swapToStrkWithFallback(routeWallet, tokenIn, amountRaw, startStrkBalance, stagePayload) {
  const strkToken = makeStarknetToken({
    name: 'STRK',
    address: STRK_TOKEN,
    decimals: 18,
    symbol: 'STRK',
  })

  let submittedProvider = null
  let swapTx = null
  for (const provider of ['ekubo', 'avnu']) {
    try {
      swapTx = await routeWallet.swap(
        {
          tokenIn,
          tokenOut: strkToken,
          amountIn: Amount.fromRaw(amountRaw, tokenIn.decimals, tokenIn.symbol),
          slippageBps: SWAP_SLIPPAGE_BPS,
          provider,
        },
        {
          feeMode: 'user_pays',
        },
      )
      submittedProvider = provider
      break
    } catch (error) {
      if (provider === 'avnu' || !isSwapRouteUnavailable(error)) {
        throw error
      }
    }
  }

  if (!swapTx || !submittedProvider) {
    throw new Error('No STRK swap provider could quote the routed asset.')
  }

  await callbackRouter(stagePayload.jobId, {
    status: 'processing',
    response: {
      stage: 'swap_submitted',
      swap_provider: submittedProvider,
      swap_tx_hash: swapTx.hash,
      ...stagePayload.payload,
    },
  })
  await swapTx.wait()
  const strkBalanceAfterSwap = await readBalanceRaw(routeWallet, strkToken)
  const creditedAmountRaw = strkBalanceAfterSwap - startStrkBalance
  if (creditedAmountRaw <= 0n) {
    throw new Error('Swap completed but no STRK increase was observed.')
  }
  return {
    provider: submittedProvider,
    hash: swapTx.hash,
    creditedAmountRaw,
  }
}

async function estimateSolanaSpendableRaw(routeWallet, bridgeToken, externalWallet, amountRaw) {
  try {
    const estimate = await routeWallet.getDepositFeeEstimate(bridgeToken, externalWallet)
    const reserve =
      amountToRaw(estimate.localFee) +
      amountToRaw(estimate.interchainFee) +
      SOLANA_FEE_MARGIN_LAMPORTS
    return amountRaw > reserve ? amountRaw - reserve : 0n
  } catch {
    return amountRaw > SOLANA_NATIVE_FEE_BUFFER_LAMPORTS
      ? amountRaw - SOLANA_NATIVE_FEE_BUFFER_LAMPORTS
      : 0n
  }
}

async function processEthereumRouteJob(job, routeWallet) {
  const sourceChainKey = String(job.payload?.source?.chain_key ?? '')
  const ethereumRpcUrl = SOURCE_RPC_URLS[sourceChainKey]
  if (!ethereumRpcUrl) {
    throw new Error(`No Ethereum RPC URL configured for ${sourceChainKey}.`)
  }

  const bridgeTokens = await getEthereumBridgeTokens()
  const bridgeToken = findBridgeToken(bridgeTokens, job.payload)
  if (!bridgeToken) {
    throw new Error('Could not match the deposit asset to a StarkZap bridge token.')
  }

  const evmProvider = new JsonRpcProvider(ethereumRpcUrl)
  const derived = deriveEvmPrivateKey(
    MASTER_SECRET,
    job.payload?.destination?.user_id ?? job.payload?.destination?.wallet_address,
    sourceChainKey,
  )
  if (
    normalizeEvmAddress(job.payload?.source?.deposit_address) !==
    normalizeEvmAddress(derived.address)
  ) {
    throw new Error('Derived EVM custody wallet does not match the issued deposit address.')
  }
  const sourceWallet = new EthersWallet(derived.privateKey, evmProvider)
  const amountRaw = BigInt(job.payload?.source?.amount_raw ?? '0')
  if (amountRaw <= 0n) {
    throw new Error('Deposit amount must be greater than zero.')
  }

  const bridgedToken = makeStarknetToken({
    name: bridgeToken.name,
    address: bridgeToken.starknetAddress,
    decimals: bridgeToken.decimals,
    symbol: bridgeToken.symbol,
  })
  const strkToken = makeStarknetToken({
    name: 'STRK',
    address: STRK_TOKEN,
    decimals: 18,
    symbol: 'STRK',
  })

  const startBridgedBalance = await readBalanceRaw(routeWallet, bridgedToken)
  const startStrkBalance = await readBalanceRaw(routeWallet, strkToken)
  const bridge = await createEthereumBridge(bridgeToken, sourceWallet, routeWallet, ethereumRpcUrl)
  const executionReserveWei = await estimateEthereumExecutionReserveWei(bridge)
  let sourceAmountRaw = amountRaw
  let gasTopUp = null
  if (evmSourceAssetUsesNativeGas(job)) {
    if (amountRaw <= executionReserveWei) {
      throw new Error('Deposit amount is too small after reserving the Ethereum execution buffer.')
    }
    sourceAmountRaw -= executionReserveWei
  } else {
    gasTopUp = await topUpEthereumGasIfNeeded(
      evmProvider,
      normalizeEvmAddress(derived.address),
      executionReserveWei,
    )
  }

  const sourceAmount = Amount.fromRaw(sourceAmountRaw, bridgeToken.decimals, bridgeToken.symbol)

  const bridgeTx = await bridge.deposit(routeWallet.address, sourceAmount)
  await callbackRouter(job.job_id, {
    status: 'processing',
    response: {
      stage: 'bridge_submitted',
      bridge_tx_hash: bridgeTx.hash,
      source_asset: bridgeToken.symbol,
      execution_reserve_wei: executionReserveWei.toString(),
      effective_source_amount_raw: sourceAmountRaw.toString(),
      gas_topup_tx_hash: gasTopUp?.hash,
    },
  })

  await waitForEthereumReceipt(evmProvider, bridgeTx.hash)

  const bridged = await waitForBalanceIncrease(
    routeWallet,
    bridgedToken,
    startBridgedBalance,
    sourceAmountRaw,
  )

  let creditedAmountRaw = bridged.delta
  let swapTxHash
  let swapProvider

  if (normalizeEvmAddress(bridgedToken.address) !== normalizeEvmAddress(strkToken.address)) {
    const swapResult = await swapToStrkWithFallback(
      routeWallet,
      bridgedToken,
      creditedAmountRaw,
      startStrkBalance,
      {
        jobId: job.job_id,
        payload: {
          bridge_tx_hash: bridgeTx.hash,
          gas_topup_tx_hash: gasTopUp?.hash,
        },
      },
    )
    swapTxHash = swapResult.hash
    swapProvider = swapResult.provider
    creditedAmountRaw = swapResult.creditedAmountRaw
  } else {
    const currentStrkBalance = await readBalanceRaw(routeWallet, strkToken)
    creditedAmountRaw = currentStrkBalance - startStrkBalance
    if (creditedAmountRaw <= 0n) {
      creditedAmountRaw = bridged.delta
    }
  }

  await submitBankrollCredit(job, routeWallet, creditedAmountRaw, {
    bridge_tx_hash: bridgeTx.hash,
    swap_tx_hash: swapTxHash,
    swap_provider: swapProvider,
    gas_topup_tx_hash: gasTopUp?.hash,
    execution_reserve_wei: executionReserveWei.toString(),
    effective_source_amount_raw: sourceAmountRaw.toString(),
  })
}

async function processSolanaRouteJob(job, routeWallet) {
  const sourceChainKey = String(job.payload?.source?.chain_key ?? '')
  const bridgeTokens = await getSolanaBridgeTokens()
  const bridgeToken = findSolanaBridgeToken(bridgeTokens, job.payload)
  if (!bridgeToken) {
    throw new Error('Could not match the Solana deposit asset to a StarkZap bridge token.')
  }

  const solanaWeb3 = await getSolanaWeb3()
  const connection = new solanaWeb3.Connection(resolveSolanaRpcUrl(sourceChainKey), 'confirmed')
  const keypair = await deriveSolanaKeypair(
    MASTER_SECRET,
    job.payload?.destination?.user_id ?? job.payload?.destination?.wallet_address,
    sourceChainKey,
  )
  const derivedAddress = keypair.publicKey.toBase58()
  if (
    normalizeSolanaAddress(job.payload?.source?.deposit_address) !==
    normalizeSolanaAddress(derivedAddress)
  ) {
    throw new Error('Derived Solana custody wallet does not match the issued deposit address.')
  }
  const externalProvider = createDeterministicSolanaProvider(connection, keypair)
  const sourceAddress = derivedAddress
  const externalWallet = await ConnectedSolanaWallet.from(
    {
      chain: ExternalChain.SOLANA,
      provider: externalProvider,
      address: sourceAddress,
      chainId: resolveSolanaNetwork(sourceChainKey),
    },
    routeWallet.getChainId(),
  )

  let amountRaw = BigInt(job.payload?.source?.amount_raw ?? '0')
  if (amountRaw <= 0n) {
    throw new Error('Deposit amount must be greater than zero.')
  }
  amountRaw = await estimateSolanaSpendableRaw(routeWallet, bridgeToken, externalWallet, amountRaw)
  if (amountRaw <= 0n) {
    throw new Error('Deposit amount is too small after reserving the Solana bridge execution fees.')
  }

  const bridgedToken = makeStarknetToken({
    name: bridgeToken.name,
    address: bridgeToken.starknetAddress,
    decimals: bridgeToken.decimals,
    symbol: bridgeToken.symbol,
  })
  const strkToken = makeStarknetToken({
    name: 'STRK',
    address: STRK_TOKEN,
    decimals: 18,
    symbol: 'STRK',
  })

  const startBridgedBalance = await readBalanceRaw(routeWallet, bridgedToken)
  const startStrkBalance = await readBalanceRaw(routeWallet, strkToken)
  const bridgeTx = await routeWallet.deposit(
    routeWallet.address,
    Amount.fromRaw(amountRaw, bridgeToken.decimals, bridgeToken.symbol),
    bridgeToken,
    externalWallet,
  )
  await callbackRouter(job.job_id, {
    status: 'processing',
    response: {
      stage: 'solana_bridge_submitted',
      bridge_tx_hash: bridgeTx.hash,
      source_asset: bridgeToken.symbol,
      effective_source_amount_raw: amountRaw.toString(),
    },
  })

  await waitForSolanaSignature(connection, bridgeTx.hash)

  const bridged = await waitForBalanceIncrease(routeWallet, bridgedToken, startBridgedBalance, 1n)
  let creditedAmountRaw = bridged.delta
  let swapTxHash
  let swapProvider

  if (normalizeStarknetAddress(bridgedToken.address) !== normalizeStarknetAddress(strkToken.address)) {
    const swapResult = await swapToStrkWithFallback(
      routeWallet,
      bridgedToken,
      creditedAmountRaw,
      startStrkBalance,
      {
        jobId: job.job_id,
        payload: {
          bridge_tx_hash: bridgeTx.hash,
        },
      },
    )
    swapTxHash = swapResult.hash
    swapProvider = swapResult.provider
    creditedAmountRaw = swapResult.creditedAmountRaw
  } else {
    const currentStrkBalance = await readBalanceRaw(routeWallet, strkToken)
    creditedAmountRaw = currentStrkBalance - startStrkBalance
    if (creditedAmountRaw <= 0n) {
      creditedAmountRaw = bridged.delta
    }
  }

  await submitBankrollCredit(job, routeWallet, creditedAmountRaw, {
    bridge_tx_hash: bridgeTx.hash,
    swap_tx_hash: swapTxHash,
    swap_provider: swapProvider,
    effective_source_amount_raw: amountRaw.toString(),
  })
}

async function processStarknetRouteJob(job, routeWallet) {
  const sourceChainKey = String(job.payload?.source?.chain_key ?? '')
  const sourceAssetAddress = normalizeStarknetAddress(job.payload?.source?.asset_address)
  const amountRaw = BigInt(job.payload?.source?.amount_raw ?? '0')
  if (amountRaw <= 0n) {
    throw new Error('Deposit amount must be greater than zero.')
  }

  const privateKey = deriveStarknetPrivateKey(
    MASTER_SECRET,
    job.payload?.destination?.user_id ?? job.payload?.destination?.wallet_address,
    sourceChainKey,
  )
  const sourceWallet = await createDerivedStarknetSourceWallet(privateKey)
  if (
    normalizeStarknetAddress(sourceWallet.address) !==
    normalizeStarknetAddress(job.payload?.source?.deposit_address)
  ) {
    throw new Error('Derived Starknet custody wallet does not match the issued deposit address.')
  }

  const sourceToken = makeStarknetToken({
    name: String(job.payload?.source?.asset_symbol ?? '').toUpperCase(),
    address: sourceAssetAddress,
    decimals: Number.parseInt(String(job.payload?.source?.asset_decimals ?? '18'), 10) || 18,
    symbol: String(job.payload?.source?.asset_symbol ?? '').toUpperCase(),
  })
  const strkToken = makeStarknetToken({
    name: 'STRK',
    address: STRK_TOKEN,
    decimals: 18,
    symbol: 'STRK',
  })

  const sourceWalletDeployed = await sourceWallet.isDeployed()
  const preDeployFeeTopUp = await ensureSourceFeeBuffer(
    routeWallet,
    sourceWallet,
    amountRaw,
    sourceAssetAddress,
    sourceWalletDeployed ? 0n : STARKNET_DEPLOY_BUFFER_RAW,
  )
  if (preDeployFeeTopUp) {
    await callbackRouter(job.job_id, {
      status: 'processing',
      response: {
        stage: 'starknet_fee_topup_submitted',
        topup_tx_hash: preDeployFeeTopUp.hash,
        topup_amount_raw: preDeployFeeTopUp.amountRaw,
        reserve_kind: sourceWalletDeployed ? 'fee_buffer' : 'deploy_and_fee_buffer',
      },
    })
  }

  await ensureStarknetWalletReady(sourceWallet, {
    deploy: 'if_needed',
    feeMode: 'user_pays',
  })

  const postDeployFeeTopUp = await ensureSourceFeeBuffer(
    routeWallet,
    sourceWallet,
    amountRaw,
    sourceAssetAddress,
  )
  if (postDeployFeeTopUp) {
    await callbackRouter(job.job_id, {
      status: 'processing',
      response: {
        stage: 'starknet_post_deploy_fee_topup_submitted',
        topup_tx_hash: postDeployFeeTopUp.hash,
        topup_amount_raw: postDeployFeeTopUp.amountRaw,
        reserve_kind: 'fee_buffer',
      },
    })
  }

  const startSourceAssetBalance = await readBalanceRaw(routeWallet, sourceToken)
  const startStrkBalance = await readBalanceRaw(routeWallet, strkToken)
  const sourceTransferTx = await transferStarknetToken(
    sourceWallet,
    sourceAssetAddress,
    routeWallet.address,
    amountRaw,
  )
  await callbackRouter(job.job_id, {
    status: 'processing',
    response: {
      stage: 'source_transfer_submitted',
      source_transfer_tx_hash: sourceTransferTx.hash,
      source_asset: sourceToken.symbol,
      topup_tx_hash: postDeployFeeTopUp?.hash ?? preDeployFeeTopUp?.hash,
    },
  })
  await sourceTransferTx.wait()

  const received = await waitForBalanceIncrease(routeWallet, sourceToken, startSourceAssetBalance, amountRaw)

  let creditedAmountRaw = received.delta
  let swapTxHash
  if (normalizeStarknetAddress(sourceToken.address) !== normalizeStarknetAddress(strkToken.address)) {
    const swapTx = await routeWallet.swap(
      {
        tokenIn: sourceToken,
        tokenOut: strkToken,
        amountIn: Amount.fromRaw(received.delta, sourceToken.decimals, sourceToken.symbol),
        slippageBps: SWAP_SLIPPAGE_BPS,
        provider: 'ekubo',
      },
      {
        feeMode: 'user_pays',
      },
    )
    swapTxHash = swapTx.hash
    await callbackRouter(job.job_id, {
      status: 'processing',
      response: {
        stage: 'swap_submitted',
        source_transfer_tx_hash: sourceTransferTx.hash,
        swap_tx_hash: swapTxHash,
        topup_tx_hash: feeTopUp?.hash,
      },
    })
    await swapTx.wait()
    const strkBalanceAfterSwap = await readBalanceRaw(routeWallet, strkToken)
    creditedAmountRaw = strkBalanceAfterSwap - startStrkBalance
    if (creditedAmountRaw <= 0n) {
      throw new Error('Swap completed but no STRK increase was observed.')
    }
  } else {
    const currentStrkBalance = await readBalanceRaw(routeWallet, strkToken)
    creditedAmountRaw = currentStrkBalance - startStrkBalance
    if (creditedAmountRaw <= 0n) {
      creditedAmountRaw = received.delta
    }
  }

  await submitBankrollCredit(job, routeWallet, creditedAmountRaw, {
    source_transfer_tx_hash: sourceTransferTx.hash,
    swap_tx_hash: swapTxHash,
    topup_tx_hash: postDeployFeeTopUp?.hash ?? preDeployFeeTopUp?.hash,
  })
}

async function processRouteJob(job) {
  requireConfig()
  const routeWallet = await getRouteWallet()
  const routeKind = String(job.payload?.route_kind ?? job.job_type ?? '')

  switch (routeKind) {
    case 'bridge_and_swap_to_strk':
      return processEthereumRouteJob(job, routeWallet)
    case 'solana_bridge_and_swap_to_strk':
      return processSolanaRouteJob(job, routeWallet)
    case 'starknet_swap_to_strk':
    case 'starknet_credit_to_strk':
      return processStarknetRouteJob(job, routeWallet)
    default:
      throw new Error(`Unsupported route kind "${routeKind}".`)
  }
}

async function handleRouteJob(request, response) {
  assertExecutorToken(request)
  const body = await readJson(request)
  const jobId = String(body.job_id ?? '')
  if (!jobId) {
    const error = new Error('job_id is required.')
    error.statusCode = 400
    throw error
  }
  if (!body.payload || typeof body.payload !== 'object') {
    const error = new Error('payload is required.')
    error.statusCode = 400
    throw error
  }

  if (!activeJobs.has(jobId)) {
    const pending = processRouteJob(body).catch(async (error) => {
      console.error('route job failed', error)
      const errorMessage =
        DEBUG_ROUTE_ERRORS && error instanceof Error && error.stack
          ? error.stack
          : error instanceof Error
            ? error.message
            : 'Route execution failed.'
      try {
        await callbackRouter(jobId, {
          status: 'failed',
          error: errorMessage,
          retryable: false,
        })
      } catch (callbackError) {
        console.error('router callback failed', callbackError)
      }
    }).finally(() => {
      activeJobs.delete(jobId)
    })
    activeJobs.set(jobId, pending)
  }

  sendJson(response, 202, {
    accepted: true,
    job_id: jobId,
    status: 'processing',
  })
}

function sleep(ms) {
  return new Promise((resolve) => {
    setTimeout(resolve, ms)
  })
}

const server = http.createServer(async (request, response) => {
  try {
    if (!request.url || !request.method) {
      sendJson(response, 404, { error: 'not found' })
      return
    }

    if (request.method === 'OPTIONS') {
      response.writeHead(204, {
        'access-control-allow-headers': 'content-type, x-moros-executor-token',
        'access-control-allow-methods': 'GET, POST, OPTIONS',
        'access-control-allow-origin': '*',
      })
      response.end()
      return
    }

    const url = new URL(request.url, `http://${request.headers.host ?? `${HOST}:${PORT}`}`)

    if (request.method === 'GET' && url.pathname === '/health') {
      const routeWalletConfigured = Boolean(STARKNET_PRIVATE_KEY && STARKNET_ACCOUNT_ADDRESS)
      const routeWalletDedicated =
        routeWalletConfigured &&
        normalizeStarknetAddress(STARKNET_ACCOUNT_ADDRESS) !==
          normalizeStarknetAddress(HOUSE_STARKNET_ACCOUNT_ADDRESS)
      sendJson(response, 200, {
        service: 'moros-deposit-executor',
        status:
          EXECUTOR_TOKEN && MASTER_SECRET && BANKROLL_VAULT && routeWalletDedicated
            ? 'ready'
            : 'misconfigured',
        route_wallet_configured: routeWalletConfigured,
        route_wallet_dedicated: routeWalletDedicated,
        route_wallet_address: STARKNET_ACCOUNT_ADDRESS ?? null,
        router_url: ROUTER_URL,
      })
      return
    }

    if (request.method === 'POST' && isRouteJobPath(url.pathname)) {
      await handleRouteJob(request, response)
      return
    }

    sendJson(response, 404, { error: 'not found' })
  } catch (error) {
    const statusCode =
      typeof error?.statusCode === 'number'
        ? error.statusCode
        : error instanceof SyntaxError
          ? 400
          : 500
    sendJson(response, statusCode, {
      error: error instanceof Error ? error.message : 'Deposit executor request failed.',
    })
  }
})

if (import.meta.url === `file://${process.argv[1]}`) {
  server.listen(PORT, HOST, () => {
    console.log(`moros-deposit-executor listening on http://${HOST}:${PORT}`)
  })
}

export {
  addBufferedMargin,
  createEthereumBridge,
  deriveEvmPrivateKey,
  deriveSolanaSeed,
  estimateSolanaSpendableRaw,
  getRequiredStarknetSourceBalanceRaw,
  isRouteJobPath,
  normalizeEvmAddress,
  server,
  serializeEvmTransactionRequest,
  splitUint256,
}
