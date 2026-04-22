import test from 'node:test'
import assert from 'node:assert/strict'
import {
  addBufferedMargin,
  deriveEvmPrivateKey,
  estimateSolanaSpendableRaw,
  getRequiredStarknetSourceBalanceRaw,
  isRouteJobPath,
  normalizeEvmAddress,
  serializeEvmTransactionRequest,
  splitUint256,
} from './index.mjs'

test('deriveEvmPrivateKey is deterministic for the same user-chain tuple', () => {
  const first = deriveEvmPrivateKey('moros-secret', '0x1234', 'ethereum-mainnet')
  const second = deriveEvmPrivateKey('moros-secret', '0x1234', 'ethereum-mainnet')
  const third = deriveEvmPrivateKey('moros-secret', '0xabcd', 'ethereum-mainnet')
  const differentChain = deriveEvmPrivateKey('moros-secret', '0x1234', 'base-mainnet')

  assert.equal(first.privateKey, second.privateKey)
  assert.equal(first.address, second.address)
  assert.notEqual(first.address, third.address)
  assert.notEqual(first.address, differentChain.address)
  assert.equal(first.address.length, 42)
})

test('splitUint256 preserves low and high limbs', () => {
  const value = (2n ** 200n) + 12345n
  const split = splitUint256(value)
  const reconstructed = BigInt(split.low) + (BigInt(split.high) << 128n)

  assert.equal(reconstructed, value)
})

test('normalizeEvmAddress lowercases and prefixes addresses', () => {
  assert.equal(normalizeEvmAddress('ABCDEF'), '0xabcdef')
  assert.equal(
    normalizeEvmAddress('0xAbCdEf1234567890'),
    '0xabcdef1234567890',
  )
})

test('serializeEvmTransactionRequest normalizes transaction quantities', () => {
  assert.deepEqual(
    serializeEvmTransactionRequest({
      to: '0x1234',
      value: 10n,
      gasLimit: 21_000n,
      maxFeePerGas: 2_000_000_000n,
      nonce: 7,
      chainId: 1,
      type: 2,
    }),
    {
      to: '0x1234',
      value: '0xa',
      gas_limit: '0x5208',
      max_fee_per_gas: '0x77359400',
      nonce: '0x7',
      chain_id: '0x1',
      type: 2,
    },
  )
})

test('addBufferedMargin applies the larger of percentage and floor buffer', () => {
  assert.equal(addBufferedMargin(1000n, 2000n, 50n), 1200n)
  assert.equal(addBufferedMargin(1000n, 100n, 50n), 1050n)
})

test('estimateSolanaSpendableRaw subtracts dynamic bridge fees when available', async () => {
  const routeWallet = {
    async getDepositFeeEstimate() {
      return {
        localFee: {
          toBase() {
            return 1000n
          },
        },
        interchainFee: {
          toBase() {
            return 2000n
          },
        },
      }
    },
  }

  const spendable = await estimateSolanaSpendableRaw(routeWallet, {}, {}, 1_000_000n)
  assert.equal(spendable, 747_000n)
})

test('estimateSolanaSpendableRaw falls back to the static safety buffer when fee estimation fails', async () => {
  const routeWallet = {
    async getDepositFeeEstimate() {
      throw new Error('quote unavailable')
    },
  }

  const spendable = await estimateSolanaSpendableRaw(routeWallet, {}, {}, 10_000_000n)
  assert.equal(spendable, 4_000_000n)
})

test('isRouteJobPath accepts the legacy and normalized executor endpoints', () => {
  assert.equal(isRouteJobPath('/v1/route-jobs'), true)
  assert.equal(isRouteJobPath('/route-jobs'), true)
  assert.equal(isRouteJobPath('/'), true)
  assert.equal(isRouteJobPath('/health'), false)
})

test('getRequiredStarknetSourceBalanceRaw reserves transfer principal plus fee and deploy buffers for STRK routes', () => {
  const required = getRequiredStarknetSourceBalanceRaw(
    7_986_026_966_445_694_661n,
    '0x04718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d',
    100_000_000_000_000_000n,
  )

  assert.equal(required, 8_186_026_966_445_694_661n)
})

test('getRequiredStarknetSourceBalanceRaw only requires STRK fee reserves for non-STRK routes', () => {
  const required = getRequiredStarknetSourceBalanceRaw(
    1_000_000n,
    '0x053c91253bc9682c04929ca02ed00b3e423f6710d2ee7e0d5ebb06f3ecf368a8',
    100_000_000_000_000_000n,
  )

  assert.equal(required, 200_000_000_000_000_000n)
})
