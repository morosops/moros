const test = require('node:test')
const assert = require('node:assert/strict')
const { InvalidAuthTokenError } = require('@privy-io/node')
const { selector } = require('starknet')

function loadBridgeWithEnv(env = {}) {
  const target = require.resolve('./index.js')
  const previous = {
    MOROS_OPERATOR_USER_IDS: process.env.MOROS_OPERATOR_USER_IDS,
    MOROS_OPERATOR_EMAILS: process.env.MOROS_OPERATOR_EMAILS,
    MOROS_OPERATOR_WALLET_ADDRESSES: process.env.MOROS_OPERATOR_WALLET_ADDRESSES,
    MOROS_DEPOSIT_ROUTER_URL: process.env.MOROS_DEPOSIT_ROUTER_URL,
    MOROS_COORDINATOR_URL: process.env.MOROS_COORDINATOR_URL,
    MOROS_ADMIN_TOKEN: process.env.MOROS_ADMIN_TOKEN,
    MOROS_NETWORK: process.env.MOROS_NETWORK,
    MOROS_PAYMASTER_UPSTREAM_URL: process.env.MOROS_PAYMASTER_UPSTREAM_URL,
    MOROS_PAYMASTER_API_KEY: process.env.MOROS_PAYMASTER_API_KEY,
    MOROS_PAYMASTER_RATE_LIMIT_PER_MINUTE: process.env.MOROS_PAYMASTER_RATE_LIMIT_PER_MINUTE,
    MOROS_PAYMASTER_ALLOWED_ACCOUNT_CLASS_HASHES: process.env.MOROS_PAYMASTER_ALLOWED_ACCOUNT_CLASS_HASHES,
    MOROS_BANKROLL_VAULT_ADDRESS: process.env.MOROS_BANKROLL_VAULT_ADDRESS,
    MOROS_SESSION_REGISTRY_ADDRESS: process.env.MOROS_SESSION_REGISTRY_ADDRESS,
    MOROS_STRK_TOKEN_ADDRESS: process.env.MOROS_STRK_TOKEN_ADDRESS,
    MOROS_DICE_TABLE_ADDRESS: process.env.MOROS_DICE_TABLE_ADDRESS,
    MOROS_ROULETTE_TABLE_ADDRESS: process.env.MOROS_ROULETTE_TABLE_ADDRESS,
    MOROS_BACCARAT_TABLE_ADDRESS: process.env.MOROS_BACCARAT_TABLE_ADDRESS,
    MOROS_BLACKJACK_TABLE_ADDRESS: process.env.MOROS_BLACKJACK_TABLE_ADDRESS,
    PRIVY_APP_ID: process.env.PRIVY_APP_ID,
    PRIVY_APP_SECRET: process.env.PRIVY_APP_SECRET,
  }

  for (const [key, value] of Object.entries(env)) {
    if (value === undefined) {
      delete process.env[key]
    } else {
      process.env[key] = value
    }
  }

  delete require.cache[target]
  const bridge = require('./index.js')

  function restore() {
    delete require.cache[target]
    for (const [key, value] of Object.entries(previous)) {
      if (value === undefined) {
        delete process.env[key]
      } else {
        process.env[key] = value
      }
    }
  }

  return {
    bridge,
    restore,
  }
}

test('parseCsvSet trims and normalizes values', () => {
  const { bridge, restore } = loadBridgeWithEnv()
  try {
    const values = bridge.parseCsvSet(' Ops@Example.com, ,alpha ', (item) => item.trim().toLowerCase())
    assert.deepEqual([...values], ['ops@example.com', 'alpha'])
  } finally {
    restore()
  }
})

test('extractOperatorIdentity collects normalized emails and wallets', () => {
  const { bridge, restore } = loadBridgeWithEnv()
  try {
    const identity = bridge.extractOperatorIdentity({
      id: 'did:privy:user_123',
      linked_accounts: [
        { type: 'email', address: 'Ops@Example.com' },
        { type: 'wallet', address: '0xABCDEF' },
        { type: 'wallet', address: '0xabcdef' },
        { type: 'google_oauth', email: 'ops@example.com' },
      ],
    })

    assert.equal(identity.userId, 'did:privy:user_123')
    assert.deepEqual(identity.emails, ['ops@example.com'])
    assert.deepEqual(identity.wallets, ['0xabcdef'])
  } finally {
    restore()
  }
})

test('getOperatorMatches honors user, email, and wallet allowlists', () => {
  const { bridge, restore } = loadBridgeWithEnv({
    MOROS_OPERATOR_USER_IDS: 'did:privy:user_123',
    MOROS_OPERATOR_EMAILS: 'ops@example.com',
    MOROS_OPERATOR_WALLET_ADDRESSES: '0xabcdef',
  })

  try {
    const matches = bridge.getOperatorMatches({
      userId: 'did:privy:user_123',
      emails: ['ops@example.com'],
      wallets: ['0xabcdef'],
    })

    assert.deepEqual(matches, [
      'user:did:privy:user_123',
      'email:ops@example.com',
      'wallet:0xabcdef',
    ])
  } finally {
    restore()
  }
})

test('proxyDepositRouterJson forwards the admin token and query params', async () => {
  const { bridge, restore } = loadBridgeWithEnv({
    MOROS_DEPOSIT_ROUTER_URL: 'http://127.0.0.1:8084',
    MOROS_ADMIN_TOKEN: 'admin-token',
    MOROS_OPERATOR_USER_IDS: 'did:privy:user_123',
    PRIVY_APP_ID: 'privy-app',
    PRIVY_APP_SECRET: 'privy-secret',
  })

  const originalFetch = global.fetch
  let requestUrl
  let requestInit
  global.fetch = async (url, init) => {
    requestUrl = String(url)
    requestInit = init
    return new Response(JSON.stringify([{ job_id: 'job-1' }]), {
      headers: { 'content-type': 'application/json' },
      status: 200,
    })
  }

  try {
    const result = await bridge.proxyDepositRouterJson('/v1/deposits/route-jobs', {
      query: { limit: 25, status: 'queued' },
    })

    assert.equal(requestUrl, 'http://127.0.0.1:8084/v1/deposits/route-jobs?limit=25&status=queued')
    assert.equal(requestInit.method, 'GET')
    assert.equal(requestInit.headers['x-moros-admin-token'], 'admin-token')
    assert.deepEqual(result.payload, [{ job_id: 'job-1' }])
    assert.equal(result.statusCode, 200)
  } finally {
    global.fetch = originalFetch
    restore()
  }
})

test('proxyCoordinatorAdminJson forwards the admin token and JSON body', async () => {
  const { bridge, restore } = loadBridgeWithEnv({
    MOROS_COORDINATOR_URL: 'http://127.0.0.1:8081',
    MOROS_ADMIN_TOKEN: 'admin-token',
    PRIVY_APP_ID: 'privy-app',
    PRIVY_APP_SECRET: 'privy-secret',
  })

  const originalFetch = global.fetch
  let requestUrl
  let requestInit
  global.fetch = async (url, init) => {
    requestUrl = String(url)
    requestInit = init
    return new Response(JSON.stringify({ user_id: 'user-1' }), {
      headers: { 'content-type': 'application/json' },
      status: 200,
    })
  }

  try {
    const result = await bridge.proxyCoordinatorAdminJson('/v1/accounts/resolve/verified', {
      method: 'POST',
      body: {
        auth_provider: 'privy',
        auth_subject: 'did:privy:user_123',
      },
    })

    assert.equal(requestUrl, 'http://127.0.0.1:8081/v1/accounts/resolve/verified')
    assert.equal(requestInit.method, 'POST')
    assert.equal(requestInit.headers['x-moros-admin-token'], 'admin-token')
    assert.equal(requestInit.headers['content-type'], 'application/json')
    assert.deepEqual(JSON.parse(requestInit.body), {
      auth_provider: 'privy',
      auth_subject: 'did:privy:user_123',
    })
    assert.deepEqual(result.payload, { user_id: 'user-1' })
    assert.equal(result.statusCode, 200)
  } finally {
    global.fetch = originalFetch
    restore()
  }
})

test('proxyPublicJson can forward authenticated user deposit requests with service auth', async () => {
  const { bridge, restore } = loadBridgeWithEnv({
    MOROS_DEPOSIT_ROUTER_URL: 'http://127.0.0.1:8084',
    MOROS_ADMIN_TOKEN: 'admin-token',
    PRIVY_APP_ID: 'privy-app',
    PRIVY_APP_SECRET: 'privy-secret',
  })

  const originalFetch = global.fetch
  let requestUrl
  let requestInit
  global.fetch = async (url, init) => {
    requestUrl = String(url)
    requestInit = init
    return new Response(JSON.stringify({ ok: true }), {
      headers: { 'content-type': 'application/json' },
      status: 200,
    })
  }

  try {
    const result = await bridge.proxyPublicJson('/v1/deposits', {
      method: 'POST',
      adminToken: true,
      body: {
        auth_provider: 'privy',
        auth_subject: 'did:privy:user_123',
        asset_id: 'usdc',
        chain_key: 'ethereum-mainnet',
      },
    })

    assert.equal(requestUrl, 'http://127.0.0.1:8084/v1/deposits')
    assert.equal(requestInit.method, 'POST')
    assert.equal(requestInit.headers['content-type'], 'application/json')
    assert.equal(requestInit.headers['x-moros-admin-token'], 'admin-token')
    assert.deepEqual(JSON.parse(requestInit.body), {
      auth_provider: 'privy',
      auth_subject: 'did:privy:user_123',
      asset_id: 'usdc',
      chain_key: 'ethereum-mainnet',
    })
    assert.deepEqual(result.payload, { ok: true })
    assert.equal(result.statusCode, 200)
  } finally {
    global.fetch = originalFetch
    restore()
  }
})

test('resolved user cache stores and expires entries', async () => {
  const { bridge, restore } = loadBridgeWithEnv()

  try {
    const originalNow = Date.now
    let now = 1_000
    Date.now = () => now
    try {
      bridge.cacheResolvedUser('token-1', { id: 'did:privy:user_1' })
      assert.deepEqual(bridge.getCachedUser('token-1'), { id: 'did:privy:user_1' })

      now += 31_000
      assert.equal(bridge.getCachedUser('token-1'), undefined)
    } finally {
      Date.now = originalNow
    }
  } finally {
    restore()
  }
})

test('buildHealthPayload reports operator readiness separately', () => {
  const { bridge, restore } = loadBridgeWithEnv({
    MOROS_DEPOSIT_ROUTER_URL: 'http://127.0.0.1:8084',
    MOROS_ADMIN_TOKEN: 'admin-token',
    MOROS_OPERATOR_EMAILS: 'ops@example.com',
    PRIVY_APP_ID: 'privy-app',
    PRIVY_APP_SECRET: 'privy-secret',
  })

  try {
    assert.deepEqual(bridge.buildHealthPayload(), {
      service: 'moros-privy-bridge',
      status: 'ready',
      privy: 'ready',
      paymaster: 'ready',
      operator: 'ready',
    })
  } finally {
    restore()
  }
})

test('isPrivyAuthVerificationError recognizes invalid-token failures', () => {
  const { bridge, restore } = loadBridgeWithEnv({
    PRIVY_APP_ID: 'privy-app',
    PRIVY_APP_SECRET: 'privy-secret',
  })

  try {
    assert.equal(
      bridge.isPrivyAuthVerificationError(new InvalidAuthTokenError('Failed to verify authentication token')),
      true,
    )
    assert.equal(
      bridge.isPrivyAuthVerificationError(new Error('Authentication token is invalid')),
      true,
    )
    assert.equal(
      bridge.isPrivyAuthVerificationError(new Error('network timeout')),
      false,
    )
  } finally {
    restore()
  }
})

test('buildMorosWalletCreateInput provisions a Starknet Moros wallet owned by the user', () => {
  const { bridge, restore } = loadBridgeWithEnv({
    PRIVY_APP_ID: 'privy-app',
    PRIVY_APP_SECRET: 'privy-secret',
  })
  try {
    assert.deepEqual(
      bridge.buildMorosWalletCreateInput('did:privy:user_123'),
      {
        chain_type: 'starknet',
        display_name: 'Moros Wallet',
        owner: { user_id: 'did:privy:user_123' },
      },
    )
  } finally {
    restore()
  }
})

function call(to, entrypoint, calldata = []) {
  return {
    to,
    selector: selector.getSelectorFromName(entrypoint),
    calldata,
  }
}

test('validatePaymasterRpcRequest allows authenticated Moros deployment and game calls only', () => {
  const walletAddress = '0x123'
  const { bridge, restore } = loadBridgeWithEnv({
    PRIVY_APP_ID: 'privy-app',
    PRIVY_APP_SECRET: 'privy-secret',
    MOROS_STRK_TOKEN_ADDRESS: '0xaaa',
    MOROS_BANKROLL_VAULT_ADDRESS: '0xbbb',
    MOROS_SESSION_REGISTRY_ADDRESS: '0xccc',
    MOROS_DICE_TABLE_ADDRESS: '0xddd',
    MOROS_ROULETTE_TABLE_ADDRESS: '0xeee',
    MOROS_BACCARAT_TABLE_ADDRESS: '0xfff',
  })
  try {
    assert.doesNotThrow(() => bridge.validatePaymasterRpcRequest({
      jsonrpc: '2.0',
      id: 1,
      method: 'paymaster_buildTransaction',
      params: {
        transaction: {
          type: 'deploy_and_invoke',
          deployment: {
            address: walletAddress,
            class_hash: '0x73414441639dcd11d1846f287650a00c60c416b9d3ba45d31c651672125b2c2',
          },
          invoke: {
            user_address: walletAddress,
            calls: [
              call('0xaaa', 'approve', ['0xbbb', '100', '0']),
              call('0xbbb', 'deposit_public', [walletAddress, '100']),
              call('0xccc', 'register_session_key', [walletAddress, '0x999', '100', '200']),
              call('0xddd', 'open_round', ['1', walletAddress, walletAddress, '0', '5000', '1', '7', '9']),
            ],
          },
        },
        parameters: { version: '0x1', fee_mode: { mode: 'sponsored' } },
      },
    }, { walletAddress }))
  } finally {
    restore()
  }
})

test('validatePaymasterRpcRequest rejects sponsorship for another wallet', () => {
  const { bridge, restore } = loadBridgeWithEnv({
    PRIVY_APP_ID: 'privy-app',
    PRIVY_APP_SECRET: 'privy-secret',
    MOROS_DICE_TABLE_ADDRESS: '0xddd',
  })
  try {
    assert.throws(
      () => bridge.validatePaymasterRpcRequest({
        jsonrpc: '2.0',
        id: 1,
        method: 'paymaster_buildTransaction',
        params: {
          transaction: {
            type: 'invoke',
            invoke: {
              user_address: '0x456',
              calls: [call('0xddd', 'open_round', ['1', '0x456', '0x456', '0', '5000', '1', '7', '9'])],
            },
          },
        },
      }, { walletAddress: '0x123' }),
      /not linked to this Moros user/,
    )
  } finally {
    restore()
  }
})

test('validatePaymasterRpcRequest rejects non-Moros calls', () => {
  const { bridge, restore } = loadBridgeWithEnv({
    PRIVY_APP_ID: 'privy-app',
    PRIVY_APP_SECRET: 'privy-secret',
    MOROS_STRK_TOKEN_ADDRESS: '0xaaa',
    MOROS_BANKROLL_VAULT_ADDRESS: '0xbbb',
  })
  try {
    assert.throws(
      () => bridge.validatePaymasterRpcRequest({
        jsonrpc: '2.0',
        id: 1,
        method: 'paymaster_buildTransaction',
        params: {
          transaction: {
            type: 'invoke',
            invoke: {
              user_address: '0x123',
              calls: [call('0xaaa', 'transfer', ['0x999', '100', '0'])],
            },
          },
        },
      }, { walletAddress: '0x123' }),
      /not allowed|only sponsors/,
    )
  } finally {
    restore()
  }
})

test('getWalletMetadata reads Moros Starknet wallet metadata', () => {
  const { bridge, restore } = loadBridgeWithEnv()
  try {
    const user = {
      custom_metadata: {
        moros_starknet_wallet_id: 'wallet-stark',
        moros_starknet_wallet_address: '0xabc',
        moros_starknet_public_key: '0xpub',
      },
    }

    assert.deepEqual(bridge.getWalletMetadata(user), {
      walletId: 'wallet-stark',
      walletAddress: '0xabc',
      publicKey: '0xpub',
    })
  } finally {
    restore()
  }
})

test('signMorosWallet uses Privy rawSign with user JWT authorization', async () => {
  const { bridge, restore } = loadBridgeWithEnv({
    PRIVY_APP_ID: 'privy-app',
    PRIVY_APP_SECRET: 'privy-secret',
  })

  const calls = []
  const fakePrivy = {
    wallets() {
      return {
        rawSign: async (walletId, payload) => {
          calls.push({ walletId, payload })
          return {
            encoding: 'hex',
            signature: '0xdeadbeef',
          }
        },
      }
    },
  }

  try {
    const result = await bridge.signMorosWallet(
      fakePrivy,
      'access-token-1',
      'wallet-123',
      '0xabc',
    )

    assert.deepEqual(result, {
      encoding: 'hex',
      signature: '0xdeadbeef',
    })
    assert.deepEqual(calls, [
      {
        walletId: 'wallet-123',
        payload: {
          authorization_context: {
            user_jwts: ['access-token-1'],
          },
          params: {
            hash: '0xabc',
          },
        },
      },
    ])
  } finally {
    restore()
  }
})
