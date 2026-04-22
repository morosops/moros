const http = require('node:http')
const { URL } = require('node:url')
const { InvalidAuthTokenError, PrivyClient } = require('@privy-io/node')
const { selector } = require('starknet')

const PORT = Number.parseInt(process.env.MOROS_PRIVY_BRIDGE_PORT ?? process.env.PORT ?? '18084', 10)
const HOST = process.env.MOROS_PRIVY_BRIDGE_HOST ?? '127.0.0.1'

const PRIVY_APP_ID = process.env.PRIVY_APP_ID
const PRIVY_APP_SECRET = process.env.PRIVY_APP_SECRET
const PRIVY_JWT_VERIFICATION_KEY = process.env.PRIVY_JWT_VERIFICATION_KEY
const USER_CACHE_TTL_MS = Number.parseInt(process.env.MOROS_PRIVY_BRIDGE_USER_CACHE_TTL_MS ?? '30000', 10)
const DEPOSIT_ROUTER_URL = process.env.MOROS_DEPOSIT_ROUTER_URL ?? 'http://127.0.0.1:8084'
const COORDINATOR_URL = process.env.MOROS_COORDINATOR_URL ?? 'http://127.0.0.1:8081'
const ADMIN_TOKEN = process.env.MOROS_ADMIN_TOKEN
const NETWORK = process.env.MOROS_NETWORK === 'sepolia' ? 'sepolia' : 'mainnet'
const PAYMASTER_UPSTREAM_URL =
  process.env.MOROS_PAYMASTER_UPSTREAM_URL ??
  (NETWORK === 'sepolia' ? 'https://sepolia.paymaster.avnu.fi' : 'https://starknet.paymaster.avnu.fi')
const PAYMASTER_API_KEY = process.env.MOROS_PAYMASTER_API_KEY
const PAYMASTER_RATE_LIMIT_PER_MINUTE = Number.parseInt(
  process.env.MOROS_PAYMASTER_RATE_LIMIT_PER_MINUTE ?? '60',
  10,
)
const PAYMASTER_ALLOWED_ACCOUNT_CLASS_HASHES = parseCsvSet(
  process.env.MOROS_PAYMASTER_ALLOWED_ACCOUNT_CLASS_HASHES ??
    // Argent X v0.5 account class used by the current Privy Starknet wallet preset.
    '0x73414441639dcd11d1846f287650a00c60c416b9d3ba45d31c651672125b2c2',
  normalizeFelt,
)
const BANKROLL_VAULT_ADDRESS = process.env.MOROS_BANKROLL_VAULT_ADDRESS
const SESSION_REGISTRY_ADDRESS = process.env.MOROS_SESSION_REGISTRY_ADDRESS
const STRK_TOKEN_ADDRESS = process.env.MOROS_STRK_TOKEN_ADDRESS
const BLACKJACK_TABLE_ADDRESS = process.env.MOROS_BLACKJACK_TABLE_ADDRESS
const DICE_TABLE_ADDRESS = process.env.MOROS_DICE_TABLE_ADDRESS
const ROULETTE_TABLE_ADDRESS = process.env.MOROS_ROULETTE_TABLE_ADDRESS
const BACCARAT_TABLE_ADDRESS = process.env.MOROS_BACCARAT_TABLE_ADDRESS
const OPERATOR_USER_IDS = parseCsvSet(process.env.MOROS_OPERATOR_USER_IDS)
const OPERATOR_EMAILS = parseCsvSet(process.env.MOROS_OPERATOR_EMAILS, normalizeEmail)
const OPERATOR_WALLET_ADDRESSES = parseCsvSet(process.env.MOROS_OPERATOR_WALLET_ADDRESSES, normalizeAddress)
const resolvedUserCache = new Map()
const paymasterRateLimits = new Map()

const MOROS_WALLET_METADATA_KEYS = {
  walletId: 'moros_starknet_wallet_id',
  walletAddress: 'moros_starknet_wallet_address',
  publicKey: 'moros_starknet_public_key',
}

const privy = PRIVY_APP_ID && PRIVY_APP_SECRET
  ? new PrivyClient({
      appId: PRIVY_APP_ID,
      appSecret: PRIVY_APP_SECRET,
      ...(PRIVY_JWT_VERIFICATION_KEY ? { jwtVerificationKey: PRIVY_JWT_VERIFICATION_KEY } : {}),
    })
  : null

function parseCsvSet(value, normalize = (item) => item.trim()) {
  if (typeof value !== 'string' || !value.trim()) {
    return new Set()
  }

  return new Set(
    value
      .split(',')
      .map((item) => normalize(item))
      .filter(Boolean),
  )
}

function normalizeAddress(value) {
  if (typeof value !== 'string') {
    return ''
  }

  return value.trim().toLowerCase()
}

function normalizeFelt(value) {
  if (typeof value !== 'string' && typeof value !== 'number' && typeof value !== 'bigint') {
    return ''
  }

  try {
    return `0x${BigInt(value).toString(16)}`
  } catch {
    return normalizeAddress(String(value))
  }
}

function feltEquals(left, right) {
  const normalizedLeft = normalizeFelt(left)
  const normalizedRight = normalizeFelt(right)
  return Boolean(normalizedLeft && normalizedRight && normalizedLeft === normalizedRight)
}

function normalizeEmail(value) {
  if (typeof value !== 'string') {
    return ''
  }

  return value.trim().toLowerCase()
}

function sendJson(response, statusCode, payload) {
  response.writeHead(statusCode, {
    'access-control-allow-headers': 'content-type, authorization, x-moros-admin-token',
    'access-control-allow-methods': 'GET, POST, OPTIONS',
    'access-control-allow-origin': '*',
    'cache-control': 'no-store',
    'content-type': 'application/json; charset=utf-8',
  })
  response.end(JSON.stringify(payload))
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

function ensureConfigured() {
  if (!privy) {
    const error = new Error('Privy bridge is not configured.')
    error.statusCode = 503
    throw error
  }
}

function ensureOperatorConfigured() {
  ensureConfigured()

  if (!ADMIN_TOKEN || !DEPOSIT_ROUTER_URL) {
    const error = new Error('Operator bridge is not configured.')
    error.statusCode = 503
    throw error
  }

  if (OPERATOR_USER_IDS.size === 0 && OPERATOR_EMAILS.size === 0 && OPERATOR_WALLET_ADDRESSES.size === 0) {
    const error = new Error('Operator allowlist is empty.')
    error.statusCode = 503
    throw error
  }
}

function ensureCoordinatorBridgeConfigured() {
  ensureConfigured()

  if (!ADMIN_TOKEN || !COORDINATOR_URL) {
    const error = new Error('Coordinator bridge is not configured.')
    error.statusCode = 503
    throw error
  }
}

function getCachedUser(idToken) {
  const cached = resolvedUserCache.get(idToken)
  if (!cached) {
    return undefined
  }

  if (cached.expiresAt <= Date.now()) {
    resolvedUserCache.delete(idToken)
    return undefined
  }

  return cached.user
}

function cacheResolvedUser(idToken, user) {
  if (!idToken || !Number.isFinite(USER_CACHE_TTL_MS) || USER_CACHE_TTL_MS <= 0) {
    return user
  }

  resolvedUserCache.set(idToken, {
    expiresAt: Date.now() + USER_CACHE_TTL_MS,
    user,
  })
  return user
}

function requireString(value, fieldName) {
  if (typeof value !== 'string' || !value.trim()) {
    const error = new Error(`${fieldName} is required.`)
    error.statusCode = 400
    throw error
  }

  return value.trim()
}

function requireAuthToken(value) {
  return requireString(value, 'auth_token')
}

async function resolveFreshUser(token) {
  ensureConfigured()
  const cachedUser = getCachedUser(token)
  if (cachedUser) {
    return cachedUser
  }

  let accessTokenError
  try {
    const verified = await privy.utils().auth().verifyAccessToken(token)
    const freshUser = await privy.users()._get(verified.user_id)
    return cacheResolvedUser(token, freshUser)
  } catch (error) {
    accessTokenError = error
    if (!isPrivyAuthVerificationError(error)) {
      throw error
    }
  }

  try {
    const tokenUser = await privy.users().get({ id_token: token })
    const freshUser = await privy.users()._get(tokenUser.id)
    return cacheResolvedUser(token, freshUser)
  } catch (error) {
    if (isPrivyAuthVerificationError(error)) {
      const authError = new Error('Failed to verify authentication token')
      authError.statusCode = 401
      throw authError
    }
    throw accessTokenError ?? error
  }
}

function isPrivyAuthVerificationError(error) {
  if (error instanceof InvalidAuthTokenError) {
    return true
  }

  const message = error instanceof Error ? error.message : String(error)
  const normalized = message.trim().toLowerCase()
  return (
    normalized.includes('failed to verify authentication token') ||
    normalized.includes('authentication token is invalid') ||
    normalized.includes('authentication token expired') ||
    normalized.includes('invalid identity token') ||
    normalized.includes('unable to parse identity token')
  )
}

function isPrivyJwtDataError(error) {
  const message = error instanceof Error ? error.message : String(error)
  const normalized = message.trim().toLowerCase()
  return (
    normalized.includes('invalid jwt token provided') ||
    normalized.includes('"code":"invalid_data"') ||
    normalized.includes('invalid_data')
  )
}

function decodeJwtPart(part) {
  if (typeof part !== 'string' || !part) {
    return undefined
  }

  try {
    const padded = part.replace(/-/g, '+').replace(/_/g, '/').padEnd(part.length + ((4 - (part.length % 4 || 4)) % 4), '=')
    return JSON.parse(Buffer.from(padded, 'base64').toString('utf8'))
  } catch {
    return undefined
  }
}

function summarizeJwt(token) {
  if (typeof token !== 'string' || !token.trim()) {
    return { present: false }
  }

  const [headerPart, payloadPart] = token.split('.')
  const header = decodeJwtPart(headerPart)
  const payload = decodeJwtPart(payloadPart)
  const exp = typeof payload?.exp === 'number' ? payload.exp : undefined
  const iat = typeof payload?.iat === 'number' ? payload.iat : undefined

  return {
    present: true,
    segments: token.split('.').length,
    alg: typeof header?.alg === 'string' ? header.alg : undefined,
    kid: typeof header?.kid === 'string' ? header.kid.slice(-8) : undefined,
    iss: typeof payload?.iss === 'string' ? payload.iss : undefined,
    aud: typeof payload?.aud === 'string' ? payload.aud : Array.isArray(payload?.aud) ? payload.aud.join(',') : undefined,
    subSuffix: typeof payload?.sub === 'string' ? payload.sub.slice(-10) : undefined,
    exp,
    iat,
    expired: typeof exp === 'number' ? exp * 1000 <= Date.now() : undefined,
  }
}

function logPrivySignAttempt(event, details) {
  console.info('privy-bridge sign debug', {
    event,
    ...details,
  })
}

async function summarizePrivyAccessVerification(token) {
  if (!token) {
    return { checked: false }
  }

  try {
    const verified = await privy.utils().auth().verifyAccessToken(token)
    return {
      checked: true,
      ok: true,
      userIdSuffix: typeof verified?.user_id === 'string' ? verified.user_id.slice(-10) : undefined,
    }
  } catch (error) {
    return {
      checked: true,
      ok: false,
      error: error instanceof Error ? error.message : String(error),
    }
  }
}

function extractOperatorIdentity(user) {
  const emails = []
  const wallets = []

  for (const linkedAccount of user?.linked_accounts ?? []) {
    if (linkedAccount?.type === 'email' && typeof linkedAccount.address === 'string') {
      emails.push(normalizeEmail(linkedAccount.address))
      continue
    }

    if (
      (linkedAccount?.type === 'wallet' || linkedAccount?.type === 'smart_wallet') &&
      typeof linkedAccount.address === 'string'
    ) {
      wallets.push(normalizeAddress(linkedAccount.address))
      continue
    }

    if (typeof linkedAccount?.email === 'string') {
      emails.push(normalizeEmail(linkedAccount.email))
    }
  }

  return {
    userId: typeof user?.id === 'string' ? user.id : '',
    emails: [...new Set(emails.filter(Boolean))],
    wallets: [...new Set(wallets.filter(Boolean))],
  }
}

function getOperatorMatches(identity) {
  const matches = []

  if (identity.userId && OPERATOR_USER_IDS.has(identity.userId)) {
    matches.push(`user:${identity.userId}`)
  }

  for (const email of identity.emails) {
    if (OPERATOR_EMAILS.has(email)) {
      matches.push(`email:${email}`)
    }
  }

  for (const wallet of identity.wallets) {
    if (OPERATOR_WALLET_ADDRESSES.has(wallet)) {
      matches.push(`wallet:${wallet}`)
    }
  }

  return matches
}

function buildHealthPayload() {
  const allowlistConfigured =
    OPERATOR_USER_IDS.size > 0 || OPERATOR_EMAILS.size > 0 || OPERATOR_WALLET_ADDRESSES.size > 0

  return {
    service: 'moros-privy-bridge',
    status: privy ? 'ready' : 'misconfigured',
    privy: privy ? 'ready' : 'misconfigured',
    paymaster: PAYMASTER_UPSTREAM_URL ? 'ready' : 'misconfigured',
    operator:
      privy && ADMIN_TOKEN && DEPOSIT_ROUTER_URL && allowlistConfigured ? 'ready' : 'misconfigured',
  }
}

function requireAdminToken(request) {
  const provided = request.headers['x-moros-admin-token']
  if (provided !== ADMIN_TOKEN) {
    const error = new Error(
      provided ? 'invalid x-moros-admin-token' : 'missing x-moros-admin-token',
    )
    error.statusCode = 401
    throw error
  }
}

function requireBearerToken(request) {
  const authorization = request.headers.authorization
  if (!authorization?.startsWith('Bearer ')) {
    const error = new Error('Missing bearer token.')
    error.statusCode = 401
    throw error
  }

  return requireString(authorization.slice('Bearer '.length), 'bearer token')
}

async function requireAuthenticatedUser(request) {
  ensureConfigured()
  const idToken = requireBearerToken(request)
  const user = await resolveFreshUser(idToken)
  return {
    idToken,
    user,
  }
}

async function requireOperator(request) {
  ensureOperatorConfigured()
  const idToken = requireBearerToken(request)
  const user = await resolveFreshUser(idToken)
  const identity = extractOperatorIdentity(user)
  const matches = getOperatorMatches(identity)

  if (matches.length === 0) {
    const error = new Error('Not authorized for operator access.')
    error.statusCode = 403
    throw error
  }

  return {
    identity,
    matches,
    user,
  }
}

async function proxyPublicJson(pathname, options = {}) {
  ensureConfigured()
  if (!DEPOSIT_ROUTER_URL) {
    const error = new Error('Deposit router is not configured.')
    error.statusCode = 503
    throw error
  }
  if (options.adminToken === true && !ADMIN_TOKEN) {
    const error = new Error('Privy deposit bridge is not configured.')
    error.statusCode = 503
    throw error
  }

  const url = new URL(pathname, DEPOSIT_ROUTER_URL)
  if (options.query) {
    for (const [key, value] of Object.entries(options.query)) {
      if (value === undefined || value === null || value === '') {
        continue
      }
      url.searchParams.set(key, String(value))
    }
  }

  const headers = {}
  if (options.adminToken === true) {
    headers['x-moros-admin-token'] = ADMIN_TOKEN
  }
  if (options.body !== undefined) {
    headers['content-type'] = 'application/json'
  }

  const upstream = await fetch(url, {
    method: options.method ?? 'GET',
    headers,
    body: options.body !== undefined ? JSON.stringify(options.body) : undefined,
  })

  const rawBody = await upstream.text()
  let payload = {}
  if (rawBody) {
    try {
      payload = JSON.parse(rawBody)
    } catch {
      payload = { error: rawBody }
    }
  }

  return {
    payload,
    statusCode: upstream.status,
  }
}

async function proxyDepositRouterJson(pathname, options = {}) {
  ensureOperatorConfigured()
  const url = new URL(pathname, DEPOSIT_ROUTER_URL)
  if (options.query) {
    for (const [key, value] of Object.entries(options.query)) {
      if (value === undefined || value === null || value === '') {
        continue
      }
      url.searchParams.set(key, String(value))
    }
  }

  const headers = {
    'x-moros-admin-token': ADMIN_TOKEN,
  }

  if (options.body !== undefined) {
    headers['content-type'] = 'application/json'
  }

  const upstream = await fetch(url, {
    method: options.method ?? 'GET',
    headers,
    body: options.body !== undefined ? JSON.stringify(options.body) : undefined,
  })

  const rawBody = await upstream.text()
  let payload = {}
  if (rawBody) {
    try {
      payload = JSON.parse(rawBody)
    } catch {
      payload = { error: rawBody }
    }
  }

  return {
    payload,
    statusCode: upstream.status,
  }
}

async function proxyCoordinatorAdminJson(pathname, options = {}) {
  ensureCoordinatorBridgeConfigured()
  const url = new URL(pathname, COORDINATOR_URL)
  if (options.query) {
    for (const [key, value] of Object.entries(options.query)) {
      if (value === undefined || value === null || value === '') {
        continue
      }
      url.searchParams.set(key, String(value))
    }
  }

  const headers = {
    'x-moros-admin-token': ADMIN_TOKEN,
  }

  if (options.body !== undefined) {
    headers['content-type'] = 'application/json'
  }

  const upstream = await fetch(url, {
    method: options.method ?? 'GET',
    headers,
    body: options.body !== undefined ? JSON.stringify(options.body) : undefined,
  })

  const rawBody = await upstream.text()
  let payload = {}
  if (rawBody) {
    try {
      payload = JSON.parse(rawBody)
    } catch {
      payload = { error: rawBody }
    }
  }

  return {
    payload,
    statusCode: upstream.status,
  }
}

function selectorFor(entrypoint) {
  return normalizeFelt(selector.getSelectorFromName(entrypoint))
}

function buildAllowedPaymasterTargets() {
  const targets = new Map()

  function add(address, entrypoints) {
    const normalized = normalizeFelt(address)
    if (!normalized) {
      return
    }
    targets.set(normalized, new Set(entrypoints.map(selectorFor)))
  }

  add(STRK_TOKEN_ADDRESS, ['approve'])
  add(BANKROLL_VAULT_ADDRESS, [
    'deposit_public',
    'withdraw_public',
    'withdraw_from_vault',
    'move_to_vault',
    'move_to_gambling',
  ])
  add(SESSION_REGISTRY_ADDRESS, ['register_session_key'])
  add(DICE_TABLE_ADDRESS, ['open_round'])
  add(ROULETTE_TABLE_ADDRESS, ['open_spin'])
  add(BACCARAT_TABLE_ADDRESS, ['open_round'])
  add(BLACKJACK_TABLE_ADDRESS, [
    'open_hand_verified',
    'submit_hit',
    'submit_hit_verified',
    'submit_stand',
    'submit_double',
    'submit_double_verified',
    'submit_split',
    'submit_split_verified',
    'submit_take_insurance',
    'submit_decline_insurance',
    'submit_surrender',
    'void_expired_hand',
  ])

  return targets
}

function requirePaymasterConfigured() {
  ensureConfigured()
  if (!PAYMASTER_UPSTREAM_URL) {
    const error = new Error('Moros paymaster proxy is not configured.')
    error.statusCode = 503
    throw error
  }
}

function rateLimitPaymaster(request) {
  if (!Number.isFinite(PAYMASTER_RATE_LIMIT_PER_MINUTE) || PAYMASTER_RATE_LIMIT_PER_MINUTE <= 0) {
    return
  }

  const forwarded = String(request.headers['x-forwarded-for'] ?? '')
    .split(',')[0]
    .trim()
  const key = forwarded || request.socket.remoteAddress || 'unknown'
  const now = Date.now()
  const windowMs = 60_000
  const current = paymasterRateLimits.get(key)
  if (!current || current.resetAt <= now) {
    paymasterRateLimits.set(key, { count: 1, resetAt: now + windowMs })
    return
  }

  current.count += 1
  if (current.count > PAYMASTER_RATE_LIMIT_PER_MINUTE) {
    const error = new Error('Paymaster rate limit exceeded.')
    error.statusCode = 429
    throw error
  }
}

function getTransactionUserAddress(transaction) {
  return (
    transaction?.invoke?.user_address ??
    transaction?.invoke?.userAddress ??
    transaction?.invoke?.user ??
    undefined
  )
}

function getTransactionDeployment(transaction) {
  return transaction?.deployment
}

function getDeploymentAddress(deployment) {
  return deployment?.address ?? deployment?.contract_address ?? deployment?.contractAddress
}

function getDeploymentClassHash(deployment) {
  return deployment?.class_hash ?? deployment?.classHash
}

function getCallTarget(call) {
  return call?.to ?? call?.To ?? call?.contractAddress ?? call?.ContractAddress
}

function getCallSelector(call) {
  return call?.selector ?? call?.Selector
}

function getCallCalldata(call) {
  const calldata = call?.calldata ?? call?.Calldata ?? []
  return Array.isArray(calldata) ? calldata : []
}

function getInvokeCalls(transaction) {
  if (Array.isArray(transaction?.invoke?.calls)) {
    return transaction.invoke.calls
  }

  const typedData = transaction?.invoke?.typed_data ?? transaction?.invoke?.typedData
  const message = typedData?.message
  if (Array.isArray(message?.calls)) {
    return message.calls
  }
  if (Array.isArray(message?.Calls)) {
    return message.Calls
  }
  return []
}

function validatePaymasterDeployment(transaction, wallet) {
  const deployment = getTransactionDeployment(transaction)
  if (!deployment) {
    return
  }

  const deploymentAddress = getDeploymentAddress(deployment)
  if (!feltEquals(deploymentAddress, wallet.walletAddress)) {
    const error = new Error('Paymaster deployment address is not linked to this Moros user.')
    error.statusCode = 403
    throw error
  }

  const classHash = normalizeFelt(getDeploymentClassHash(deployment))
  if (
    PAYMASTER_ALLOWED_ACCOUNT_CLASS_HASHES.size > 0 &&
    (!classHash || !PAYMASTER_ALLOWED_ACCOUNT_CLASS_HASHES.has(classHash))
  ) {
    const error = new Error('Paymaster account class is not allowed.')
    error.statusCode = 403
    throw error
  }
}

function validateMorosPaymasterCall(call, wallet) {
  const target = normalizeFelt(getCallTarget(call))
  const callSelector = normalizeFelt(getCallSelector(call))
  const allowedTargets = buildAllowedPaymasterTargets()
  const allowedSelectors = allowedTargets.get(target)
  if (!allowedSelectors || !allowedSelectors.has(callSelector)) {
    const error = new Error('Paymaster call is not allowed.')
    error.statusCode = 403
    throw error
  }

  const calldata = getCallCalldata(call)
  if (target === normalizeFelt(STRK_TOKEN_ADDRESS)) {
    if (!feltEquals(callSelector, selectorFor('approve')) || !feltEquals(calldata[0], BANKROLL_VAULT_ADDRESS)) {
      const error = new Error('Paymaster only sponsors STRK approvals to the Moros vault.')
      error.statusCode = 403
      throw error
    }
    return
  }

  if (target === normalizeFelt(BANKROLL_VAULT_ADDRESS) && feltEquals(callSelector, selectorFor('deposit_public'))) {
    if (!feltEquals(calldata[0], wallet.walletAddress)) {
      const error = new Error('Paymaster deposit target must be the authenticated Moros wallet.')
      error.statusCode = 403
      throw error
    }
    return
  }

  if (target === normalizeFelt(SESSION_REGISTRY_ADDRESS)) {
    if (!feltEquals(calldata[0], wallet.walletAddress)) {
      const error = new Error('Paymaster session owner must be the authenticated Moros wallet.')
      error.statusCode = 403
      throw error
    }
    return
  }

  const gameTargets = new Set([
    normalizeFelt(DICE_TABLE_ADDRESS),
    normalizeFelt(ROULETTE_TABLE_ADDRESS),
    normalizeFelt(BACCARAT_TABLE_ADDRESS),
  ].filter(Boolean))
  if (gameTargets.has(target)) {
    if (!feltEquals(calldata[1], wallet.walletAddress) || !feltEquals(calldata[2], wallet.walletAddress)) {
      const error = new Error('Paymaster game call must open a round for the authenticated Moros wallet.')
      error.statusCode = 403
      throw error
    }
  }
}

function validatePaymasterRpcRequest(body, wallet) {
  if (body?.jsonrpc !== '2.0' || typeof body?.method !== 'string') {
    const error = new Error('Invalid paymaster JSON-RPC request.')
    error.statusCode = 400
    throw error
  }

  if (!['paymaster_isAvailable', 'paymaster_buildTransaction', 'paymaster_executeTransaction', 'paymaster_getSupportedTokens'].includes(body.method)) {
    const error = new Error('Paymaster method is not allowed.')
    error.statusCode = 403
    throw error
  }

  if (body.method === 'paymaster_isAvailable' || body.method === 'paymaster_getSupportedTokens') {
    return
  }

  const transaction = body?.params?.transaction
  if (!transaction || typeof transaction !== 'object') {
    const error = new Error('Paymaster transaction is required.')
    error.statusCode = 400
    throw error
  }

  validatePaymasterDeployment(transaction, wallet)

  const userAddress = getTransactionUserAddress(transaction)
  if (userAddress && !feltEquals(userAddress, wallet.walletAddress)) {
    const error = new Error('Paymaster transaction user is not linked to this Moros user.')
    error.statusCode = 403
    throw error
  }

  const calls = getInvokeCalls(transaction)
  if (transaction.type !== 'deploy' && calls.length === 0) {
    const error = new Error('Paymaster transaction calls are required.')
    error.statusCode = 400
    throw error
  }

  for (const call of calls) {
    validateMorosPaymasterCall(call, wallet)
  }
}

async function proxyPaymasterJson(body) {
  requirePaymasterConfigured()
  const headers = {
    'content-type': 'application/json',
  }
  if (PAYMASTER_API_KEY) {
    headers['x-paymaster-api-key'] = PAYMASTER_API_KEY
  }

  const upstream = await fetch(PAYMASTER_UPSTREAM_URL, {
    method: 'POST',
    headers,
    body: JSON.stringify(body),
  })
  const rawBody = await upstream.text()
  return {
    rawBody,
    statusCode: upstream.status,
    contentType: upstream.headers.get('content-type') ?? 'application/json; charset=utf-8',
  }
}

async function handlePaymaster(request, response) {
  let body = {}
  try {
    rateLimitPaymaster(request)
    const { idToken } = await requireAuthenticatedUser(request)
    body = await readJson(request)
    const requestedWalletId =
      typeof request.headers['x-moros-wallet-id'] === 'string'
        ? request.headers['x-moros-wallet-id'].trim()
        : undefined
    const requestedWalletAddress =
      typeof request.headers['x-moros-wallet-address'] === 'string'
        ? request.headers['x-moros-wallet-address'].trim()
        : undefined
    const wallet = await ensureStarknetWallet(
      idToken,
      requestedWalletId,
      requestedWalletAddress,
    )
    validatePaymasterRpcRequest(body, wallet)
    const result = await proxyPaymasterJson(body)
    response.writeHead(result.statusCode, {
      'access-control-allow-headers': 'content-type, authorization, x-moros-admin-token',
      'access-control-allow-methods': 'POST, OPTIONS',
      'access-control-allow-origin': '*',
      'cache-control': 'no-store',
      'content-type': result.contentType,
    })
    response.end(result.rawBody)
  } catch (error) {
    const statusCode =
      typeof error?.statusCode === 'number'
        ? error.statusCode
        : error instanceof SyntaxError
          ? 400
          : 500
    response.writeHead(200, {
      'access-control-allow-headers': 'content-type, authorization, x-moros-admin-token',
      'access-control-allow-methods': 'POST, OPTIONS',
      'access-control-allow-origin': '*',
      'cache-control': 'no-store',
      'content-type': 'application/json; charset=utf-8',
    })
    response.end(JSON.stringify({
      jsonrpc: '2.0',
      id: typeof body?.id === 'number' || typeof body?.id === 'string' ? body.id : 0,
      error: {
        code: statusCode === 403 ? 403 : statusCode === 429 ? 429 : -32603,
        message: error instanceof Error ? error.message : 'Paymaster request failed.',
      },
    }))
  }
}

function getWalletMetadata(user) {
  const keys = MOROS_WALLET_METADATA_KEYS
  const metadata = user.custom_metadata ?? {}
  const walletId = typeof metadata[keys.walletId] === 'string' ? metadata[keys.walletId] : undefined
  const walletAddress =
    typeof metadata[keys.walletAddress] === 'string' ? metadata[keys.walletAddress] : undefined
  const publicKey =
    typeof metadata[keys.publicKey] === 'string' ? metadata[keys.publicKey] : undefined

  if (!walletId || !walletAddress) {
    return undefined
  }

  return {
    walletId,
    walletAddress,
    publicKey,
  }
}

function getLinkedStarknetWallet(user) {
  for (const linkedAccount of user?.linked_accounts ?? []) {
    const type = typeof linkedAccount?.type === 'string' ? linkedAccount.type : undefined
    const chainType =
      typeof linkedAccount?.chain_type === 'string'
        ? linkedAccount.chain_type.trim().toLowerCase()
        : undefined
    const walletId = typeof linkedAccount?.id === 'string' ? linkedAccount.id.trim() : ''
    const walletAddress =
      typeof linkedAccount?.address === 'string' ? linkedAccount.address.trim() : ''
    const publicKey =
      typeof linkedAccount?.public_key === 'string' && linkedAccount.public_key.trim()
        ? linkedAccount.public_key.trim()
        : undefined

    if (
      (type === 'wallet' || type === 'smart_wallet') &&
      chainType === 'starknet' &&
      walletId &&
      walletAddress
    ) {
      return {
        walletId,
        walletAddress,
        publicKey,
      }
    }
  }

  return undefined
}

async function syncWalletMetadata(user, idToken, wallet) {
  const metadataKeys = MOROS_WALLET_METADATA_KEYS
  const currentMetadata = user.custom_metadata ?? {}
  const nextMetadata = {
    ...currentMetadata,
    [metadataKeys.walletId]: wallet.walletId,
    [metadataKeys.walletAddress]: wallet.walletAddress,
  }
  if (wallet.publicKey) {
    nextMetadata[metadataKeys.publicKey] = wallet.publicKey
  } else {
    delete nextMetadata[metadataKeys.publicKey]
  }

  const metadataChanged =
    currentMetadata[metadataKeys.walletId] !== nextMetadata[metadataKeys.walletId]
    || currentMetadata[metadataKeys.walletAddress] !== nextMetadata[metadataKeys.walletAddress]
    || currentMetadata[metadataKeys.publicKey] !== nextMetadata[metadataKeys.publicKey]

  if (metadataChanged) {
    await privy.users().setCustomMetadata(user.id, {
      custom_metadata: nextMetadata,
    })
  }

  cacheResolvedUser(idToken, {
    ...user,
    custom_metadata: nextMetadata,
  })
}

function buildMorosWalletCreateInput(userId) {
  return {
    chain_type: 'starknet',
    display_name: 'Moros Wallet',
    owner: { user_id: userId },
  }
}

async function ensureStarknetWallet(idToken, requestedWalletId, requestedWalletAddress) {
  const user = await resolveFreshUser(idToken)

  if (typeof requestedWalletId === 'string' && requestedWalletId.trim()) {
    const wallet = await privy.wallets().get(requestedWalletId.trim())
    const walletId = requireString(wallet.id, 'wallet id')
    const walletAddress = requireString(wallet.address, 'wallet address')
    const publicKey =
      typeof wallet.public_key === 'string' && wallet.public_key.trim()
        ? wallet.public_key.trim()
        : undefined
    if (wallet.chain_type !== 'starknet') {
      const error = new Error('Requested wallet is not a Starknet wallet.')
      error.statusCode = 403
      throw error
    }
    if (requestedWalletAddress && !feltEquals(walletAddress, requestedWalletAddress)) {
      const error = new Error('Requested wallet address does not match requested wallet id.')
      error.statusCode = 403
      throw error
    }
    await syncWalletMetadata(user, idToken, {
      walletId,
      walletAddress,
      publicKey,
    })
    return {
      walletId,
      walletAddress,
      publicKey,
      userId: user.id,
    }
  }

  const existing = getWalletMetadata(user)
  if (existing) {
    return {
      ...existing,
      userId: user.id,
    }
  }

  const linkedWallet = getLinkedStarknetWallet(user)
  if (linkedWallet) {
    await syncWalletMetadata(user, idToken, linkedWallet)
    return {
      ...linkedWallet,
      userId: user.id,
    }
  }

  const wallet = await privy.wallets().create(buildMorosWalletCreateInput(user.id))

  const walletId = requireString(wallet.id, 'wallet id')
  const walletAddress = requireString(wallet.address, 'wallet address')
  const publicKey =
    typeof wallet.public_key === 'string' && wallet.public_key.trim()
      ? wallet.public_key.trim()
      : undefined
  await syncWalletMetadata(user, idToken, {
    walletId,
    walletAddress,
    publicKey,
  })

  return {
    walletId,
    walletAddress,
    publicKey,
    userId: user.id,
  }
}

async function signMorosWallet(privyClient, authToken, walletId, hash) {
  ensureConfigured()
  return privyClient.wallets().rawSign(walletId, {
    authorization_context: {
      user_jwts: [authToken],
    },
    params: {
      hash,
    },
  })
}

async function handleEnsureWallet(request, response) {
  const body = await readJson(request)
  const idToken = requireAuthToken(body.auth_token ?? body.id_token)
  const wallet = await ensureStarknetWallet(idToken)
  sendJson(response, 200, {
    wallet_id: wallet.walletId,
    wallet_address: wallet.walletAddress,
    public_key: wallet.publicKey,
    user_id: wallet.userId,
  })
}

async function handleSign(request, response) {
  const body = await readJson(request)
  const idToken =
    typeof body.id_token === 'string' && body.id_token.trim()
      ? body.id_token.trim()
      : typeof body.identity_token === 'string' && body.identity_token.trim()
        ? body.identity_token.trim()
        : undefined
  const signingToken =
    typeof body.signing_token === 'string' && body.signing_token.trim()
      ? body.signing_token.trim()
      : requireAuthToken(body.auth_token ?? body.id_token)
  const lookupToken = idToken ?? signingToken
  const walletId = requireString(body.wallet_id, 'wallet_id')
  const hash = requireString(body.hash, 'hash')
  const wallet = await ensureStarknetWallet(lookupToken)

  if (wallet.walletId !== walletId) {
    const error = new Error('wallet_id is not linked to this Moros user.')
    error.statusCode = 403
    throw error
  }

  let signed
  logPrivySignAttempt('request', {
    hasIdentityToken: Boolean(idToken),
    hasSigningToken: Boolean(signingToken),
    sameToken: Boolean(idToken && signingToken && idToken === signingToken),
    identityToken: summarizeJwt(idToken),
    signingToken: summarizeJwt(signingToken),
    signingTokenAccessVerification: await summarizePrivyAccessVerification(signingToken),
    walletIdSuffix: walletId.slice(-8),
    hashSuffix: hash.slice(-8),
  })
  try {
    signed = await signMorosWallet(privy, signingToken, walletId, hash)
  } catch (error) {
    logPrivySignAttempt('primary_failed', {
      walletIdSuffix: walletId.slice(-8),
      error: error instanceof Error ? error.message : String(error),
    })
    if (signingToken !== idToken && isPrivyJwtDataError(error)) {
      try {
        signed = await signMorosWallet(privy, idToken, walletId, hash)
        logPrivySignAttempt('fallback_succeeded', {
          walletIdSuffix: walletId.slice(-8),
        })
      } catch (fallbackError) {
        logPrivySignAttempt('fallback_failed', {
          walletIdSuffix: walletId.slice(-8),
          error: fallbackError instanceof Error ? fallbackError.message : String(fallbackError),
        })
        throw fallbackError
      }
    } else {
      throw error
    }
  }

  sendJson(response, 200, {
    signature: signed.signature,
    wallet_id: wallet.walletId,
    wallet_address: wallet.walletAddress,
  })
}

async function handleCreateAuthenticatedDepositChannel(request, response) {
  const { user } = await requireAuthenticatedUser(request)
  const body = await readJson(request)
  const assetId = requireString(body.asset_id, 'asset_id')
  const chainKey = requireString(body.chain_key, 'chain_key')
  const linkedStarknetWallet = getWalletMetadata(user)

  await proxyCoordinatorAdminJson('/v1/accounts/resolve/verified', {
    method: 'POST',
    body: {
      wallet_address: linkedStarknetWallet?.walletAddress,
      auth_provider: 'privy',
      auth_subject: requireString(user?.id, 'user id'),
      linked_via: linkedStarknetWallet ? 'privy_wallet' : 'privy_auth',
      make_primary: false,
    },
  })

  const result = await proxyPublicJson('/v1/deposits', {
    method: 'POST',
    adminToken: true,
    body: {
      wallet_address: linkedStarknetWallet?.walletAddress,
      auth_provider: 'privy',
      auth_subject: requireString(user?.id, 'user id'),
      asset_id: assetId,
      chain_key: chainKey,
    },
  })

  sendJson(response, result.statusCode, result.payload)
}

async function handleResolveAuthenticatedAccount(request, response) {
  const { user } = await requireAuthenticatedUser(request)
  const body = await readJson(request)
  const result = await proxyCoordinatorAdminJson('/v1/accounts/resolve/verified', {
    method: 'POST',
    body: {
      wallet_address: typeof body.wallet_address === 'string' ? body.wallet_address : undefined,
      auth_provider: 'privy',
      auth_subject: requireString(user?.id, 'user id'),
      linked_via: typeof body.linked_via === 'string' ? body.linked_via : 'privy_bridge',
      make_primary: body.make_primary === true,
    },
  })

  sendJson(response, result.statusCode, result.payload)
}

async function handleOperatorSession(request, response) {
  const operator = await requireOperator(request)
  sendJson(response, 200, {
    user_id: operator.identity.userId,
    emails: operator.identity.emails,
    wallets: operator.identity.wallets,
    matches: operator.matches,
  })
}

async function handleOperatorRouteJobs(request, response, url) {
  await requireOperator(request)
  const result = await proxyDepositRouterJson('/v1/deposits/route-jobs', {
    query: {
      limit: url.searchParams.get('limit'),
      status: url.searchParams.get('status'),
    },
  })
  sendJson(response, result.statusCode, result.payload)
}

async function handleOperatorRetryRouteJob(request, response, jobId) {
  await requireOperator(request)
  const result = await proxyDepositRouterJson(
    `/v1/deposits/route-jobs/${encodeURIComponent(jobId)}/retry`,
    { method: 'POST' },
  )
  sendJson(response, result.statusCode, result.payload)
}

async function handleOperatorRiskFlags(request, response) {
  await requireOperator(request)
  const result = await proxyDepositRouterJson('/v1/deposits/risk-flags')
  sendJson(response, result.statusCode, result.payload)
}

async function handleOperatorResolveRiskFlag(request, response, flagId) {
  await requireOperator(request)
  const body = await readJson(request)
  const result = await proxyDepositRouterJson(
    `/v1/deposits/risk-flags/${encodeURIComponent(flagId)}/resolve`,
    {
      body,
      method: 'POST',
    },
  )
  sendJson(response, result.statusCode, result.payload)
}

async function handleOperatorRecoveries(request, response, url) {
  await requireOperator(request)
  const result = await proxyDepositRouterJson('/v1/deposits/recoveries', {
    query: {
      limit: url.searchParams.get('limit'),
      status: url.searchParams.get('status'),
    },
  })
  sendJson(response, result.statusCode, result.payload)
}

async function handleOperatorResolveRecovery(request, response, recoveryId) {
  await requireOperator(request)
  const body = await readJson(request)
  const result = await proxyDepositRouterJson(
    `/v1/deposits/recoveries/${encodeURIComponent(recoveryId)}/resolve`,
    {
      body,
      method: 'POST',
    },
  )
  sendJson(response, result.statusCode, result.payload)
}

const server = http.createServer(async (request, response) => {
  try {
    if (!request.url || !request.method) {
      sendJson(response, 404, { error: 'not found' })
      return
    }

    if (request.method === 'OPTIONS') {
      response.writeHead(204, {
        'access-control-allow-headers': 'content-type, authorization, x-moros-admin-token',
        'access-control-allow-methods': 'GET, POST, OPTIONS',
        'access-control-allow-origin': '*',
      })
      response.end()
      return
    }

    const url = new URL(request.url, `http://${request.headers.host ?? `${HOST}:${PORT}`}`)

    if (request.method === 'GET' && url.pathname === '/health') {
      sendJson(response, 200, buildHealthPayload())
      return
    }

    if (request.method === 'POST' && url.pathname === '/v1/auth/privy/starknet-wallet/ensure') {
      await handleEnsureWallet(request, response)
      return
    }

    if (request.method === 'POST' && url.pathname === '/v1/auth/privy/starknet-wallet/sign') {
      await handleSign(request, response)
      return
    }

    if (request.method === 'POST' && url.pathname === '/v1/paymaster') {
      await handlePaymaster(request, response)
      return
    }

    if (request.method === 'POST' && url.pathname === '/v1/deposits/channels') {
      await handleCreateAuthenticatedDepositChannel(request, response)
      return
    }

    if (request.method === 'POST' && url.pathname === '/v1/accounts/resolve') {
      await handleResolveAuthenticatedAccount(request, response)
      return
    }

    if (request.method === 'GET' && url.pathname === '/v1/operators/session') {
      await handleOperatorSession(request, response)
      return
    }

    if (request.method === 'GET' && url.pathname === '/v1/operators/deposits/route-jobs') {
      await handleOperatorRouteJobs(request, response, url)
      return
    }

    const routeJobRetryMatch = url.pathname.match(/^\/v1\/operators\/deposits\/route-jobs\/([^/]+)\/retry$/)
    if (request.method === 'POST' && routeJobRetryMatch) {
      await handleOperatorRetryRouteJob(request, response, routeJobRetryMatch[1])
      return
    }

    if (request.method === 'GET' && url.pathname === '/v1/operators/deposits/risk-flags') {
      await handleOperatorRiskFlags(request, response)
      return
    }

    const riskFlagResolveMatch = url.pathname.match(/^\/v1\/operators\/deposits\/risk-flags\/([^/]+)\/resolve$/)
    if (request.method === 'POST' && riskFlagResolveMatch) {
      await handleOperatorResolveRiskFlag(request, response, riskFlagResolveMatch[1])
      return
    }

    if (request.method === 'GET' && url.pathname === '/v1/operators/deposits/recoveries') {
      await handleOperatorRecoveries(request, response, url)
      return
    }

    const recoveryResolveMatch = url.pathname.match(/^\/v1\/operators\/deposits\/recoveries\/([^/]+)\/resolve$/)
    if (request.method === 'POST' && recoveryResolveMatch) {
      await handleOperatorResolveRecovery(request, response, recoveryResolveMatch[1])
      return
    }

    sendJson(response, 404, { error: 'not found' })
  } catch (error) {
    const method = request.method ?? 'UNKNOWN'
    const path = request.url ?? '/'
    console.error('privy-bridge request failed', {
      method,
      path,
      statusCode:
        typeof error?.statusCode === 'number'
          ? error.statusCode
          : error instanceof SyntaxError
            ? 400
            : 500,
      message: error instanceof Error ? error.message : 'Privy bridge request failed.',
    })
    const statusCode =
      typeof error?.statusCode === 'number'
        ? error.statusCode
        : error instanceof SyntaxError
          ? 400
          : 500
    sendJson(response, statusCode, {
      error: error instanceof Error ? error.message : 'Privy bridge request failed.',
    })
  }
})

if (require.main === module) {
  server.listen(PORT, HOST, () => {
    console.log(`moros-privy-bridge listening on http://${HOST}:${PORT}`)
  })
}

module.exports = {
  buildMorosWalletCreateInput,
  buildHealthPayload,
  cacheResolvedUser,
  extractOperatorIdentity,
  getCachedUser,
  getOperatorMatches,
  getWalletMetadata,
  parseCsvSet,
  proxyCoordinatorAdminJson,
  proxyPublicJson,
  proxyDepositRouterJson,
  proxyPaymasterJson,
  requireAuthenticatedUser,
  validatePaymasterRpcRequest,
  isPrivyAuthVerificationError,
  signMorosWallet,
  server,
  ensureStarknetWallet,
}
