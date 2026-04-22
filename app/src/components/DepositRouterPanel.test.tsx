// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { DepositRouterPanel } from './DepositRouterPanel'

const apiMocks = vi.hoisted(() => ({
  createAuthenticatedDepositChannel: vi.fn(),
  fetchDepositSupportedAssets: vi.fn(),
  createDepositChannel: vi.fn(),
  fetchDepositStatus: vi.fn(),
}))

const qrMocks = vi.hoisted(() => ({
  toDataURL: vi.fn(),
}))

vi.mock('../lib/api', () => ({
  createAuthenticatedDepositChannel: apiMocks.createAuthenticatedDepositChannel,
  fetchDepositSupportedAssets: apiMocks.fetchDepositSupportedAssets,
  createDepositChannel: apiMocks.createDepositChannel,
  fetchDepositStatus: apiMocks.fetchDepositStatus,
}))

vi.mock('qrcode', () => ({
  toDataURL: qrMocks.toDataURL,
}))

describe('DepositRouterPanel', () => {
  afterEach(() => {
    cleanup()
  })

  beforeEach(() => {
    vi.clearAllMocks()
    apiMocks.fetchDepositSupportedAssets.mockResolvedValue([
      {
        id: 'usdc',
        chain_key: 'ethereum-mainnet',
        chain_family: 'evm',
        network: 'mainnet',
        chain_id: '1',
        asset_symbol: 'USDC',
        asset_address: '0xa0b8',
        asset_decimals: 6,
        route_kind: 'bridge_and_swap_to_strk',
        watch_mode: 'erc20_transfer',
        min_amount: '1000000',
        max_amount: '50000000000',
        confirmations_required: 12,
        status: 'enabled',
        metadata: {
          label: 'USDC on Ethereum',
        },
        created_at: '2026-04-14T00:00:00.000Z',
        updated_at: '2026-04-14T00:00:00.000Z',
      },
    ])
    apiMocks.createAuthenticatedDepositChannel.mockResolvedValue({
      channel: {
        channel_id: 'channel-1',
        wallet_address: '0x1234',
        username: 'flow',
        asset_id: 'usdc',
        chain_key: 'ethereum-mainnet',
        asset_symbol: 'USDC',
        deposit_address: '0xdeposit',
        qr_payload: 'ethereum:0xa0b8@1/transfer?address=0xdeposit',
        route_kind: 'bridge_and_swap_to_strk',
        status: 'active',
        watch_from_block: 100,
        last_scanned_block: 100,
        last_seen_at: null,
        created_at: '2026-04-14T00:00:00.000Z',
        updated_at: '2026-04-14T00:00:00.000Z',
      },
      asset: {
        id: 'usdc',
        chain_key: 'ethereum-mainnet',
        chain_family: 'evm',
        network: 'mainnet',
        chain_id: '1',
        asset_symbol: 'USDC',
        asset_address: '0xa0b8',
        asset_decimals: 6,
        route_kind: 'bridge_and_swap_to_strk',
        watch_mode: 'erc20_transfer',
        min_amount: '1000000',
        max_amount: '50000000000',
        confirmations_required: 12,
        status: 'enabled',
        metadata: {
          label: 'USDC on Ethereum',
        },
        created_at: '2026-04-14T00:00:00.000Z',
        updated_at: '2026-04-14T00:00:00.000Z',
      },
      status_url: '/v1/deposits/status/0xdeposit',
    })
    apiMocks.createDepositChannel.mockResolvedValue({
      channel: {
        channel_id: 'channel-1',
        wallet_address: '0x1234',
        username: 'flow',
        asset_id: 'usdc',
        chain_key: 'ethereum-mainnet',
        asset_symbol: 'USDC',
        deposit_address: '0xdeposit',
        qr_payload: 'ethereum:0xa0b8@1/transfer?address=0xdeposit',
        route_kind: 'bridge_and_swap_to_strk',
        status: 'active',
        watch_from_block: 100,
        last_scanned_block: 100,
        last_seen_at: null,
        created_at: '2026-04-14T00:00:00.000Z',
        updated_at: '2026-04-14T00:00:00.000Z',
      },
      asset: {
        id: 'usdc',
        chain_key: 'ethereum-mainnet',
        chain_family: 'evm',
        network: 'mainnet',
        chain_id: '1',
        asset_symbol: 'USDC',
        asset_address: '0xa0b8',
        asset_decimals: 6,
        route_kind: 'bridge_and_swap_to_strk',
        watch_mode: 'erc20_transfer',
        min_amount: '1000000',
        max_amount: '50000000000',
        confirmations_required: 12,
        status: 'enabled',
        metadata: {
          label: 'USDC on Ethereum',
        },
        created_at: '2026-04-14T00:00:00.000Z',
        updated_at: '2026-04-14T00:00:00.000Z',
      },
      status_url: '/v1/deposits/status/0xdeposit',
    })
    apiMocks.fetchDepositStatus.mockResolvedValue({
      channel: {
        channel_id: 'channel-1',
        wallet_address: '0x1234',
        username: 'flow',
        asset_id: 'usdc',
        chain_key: 'ethereum-mainnet',
        asset_symbol: 'USDC',
        deposit_address: '0xdeposit',
        qr_payload: 'ethereum:0xa0b8@1/transfer?address=0xdeposit',
        route_kind: 'bridge_and_swap_to_strk',
        status: 'active',
        watch_from_block: 100,
        last_scanned_block: 120,
        last_seen_at: '2026-04-14T00:10:00.000Z',
        created_at: '2026-04-14T00:00:00.000Z',
        updated_at: '2026-04-14T00:10:00.000Z',
      },
      transfers: [
        {
          transfer_id: 'transfer-1',
          channel_id: 'channel-1',
          wallet_address: '0x1234',
          username: 'flow',
          asset_id: 'usdc',
          chain_key: 'ethereum-mainnet',
          asset_symbol: 'USDC',
          deposit_address: '0xdeposit',
          sender_address: '0xfeed1234',
          tx_hash: '0xtx',
          block_number: 120,
          amount_raw: '1000000',
          amount_display: '1',
          confirmations: 4,
          required_confirmations: 12,
          status: 'DEPOSIT_DETECTED',
          risk_state: 'clear',
          credit_target: '0x1234',
          destination_tx_hash: null,
          detected_at: '2026-04-14T00:05:00.000Z',
          confirmed_at: null,
          completed_at: null,
          created_at: '2026-04-14T00:05:00.000Z',
          updated_at: '2026-04-14T00:10:00.000Z',
        },
      ],
      route_jobs: [],
      risk_flags: [],
      recoveries: [],
    })
    qrMocks.toDataURL.mockResolvedValue('data:image/png;base64,qr')
  })

  it('loads routes, issues a deposit address, and renders QR/status details', async () => {
    render(<DepositRouterPanel walletAddress="0x1234" />)

    await waitFor(() => {
      expect(apiMocks.fetchDepositSupportedAssets).toHaveBeenCalledTimes(1)
      expect(apiMocks.createDepositChannel).toHaveBeenCalledWith({
        wallet_address: '0x1234',
        asset_id: 'usdc',
        chain_key: 'ethereum-mainnet',
      })
      expect(apiMocks.fetchDepositStatus).toHaveBeenCalledWith('0xdeposit')
    })

    expect(await screen.findByDisplayValue('0xdeposit')).toBeTruthy()
    expect(await screen.findByAltText('Deposit QR code')).toBeTruthy()
    expect(screen.getByText('Deposit detected')).toBeTruthy()
    expect(screen.getByText(/4\/12 confirmations observed/i)).toBeTruthy()
    expect(screen.getByText(/Recent deposits/i)).toBeTruthy()
    expect(screen.getByText(/Send only USDC on Ethereum Mainnet/i)).toBeTruthy()
  })

  it('issues authenticated deposit channels through the privy bridge when no wallet is connected yet', async () => {
    render(<DepositRouterPanel idToken="privy-token" />)

    await waitFor(() => {
      expect(apiMocks.createAuthenticatedDepositChannel).toHaveBeenCalledWith('privy-token', {
        asset_id: 'usdc',
        chain_key: 'ethereum-mainnet',
      })
    })

    expect(apiMocks.createDepositChannel).not.toHaveBeenCalled()
    expect(await screen.findByDisplayValue('0xdeposit')).toBeTruthy()
  })

  it('prefers authenticated deposit channels when a Privy token exists alongside a wallet address', async () => {
    const resolveIdToken = vi.fn().mockResolvedValue('lazy-privy-token')

    render(<DepositRouterPanel resolveIdToken={resolveIdToken} walletAddress="0x1234" />)

    await waitFor(() => {
      expect(resolveIdToken).toHaveBeenCalled()
      expect(apiMocks.createAuthenticatedDepositChannel).toHaveBeenCalledWith('lazy-privy-token', {
        asset_id: 'usdc',
        chain_key: 'ethereum-mainnet',
      })
    })

    expect(apiMocks.createDepositChannel).not.toHaveBeenCalled()
  })

  it('resolves the authenticated deposit session lazily when the modal opens before a token is ready', async () => {
    const resolveIdToken = vi.fn().mockResolvedValue('lazy-privy-token')

    render(<DepositRouterPanel resolveIdToken={resolveIdToken} />)

    await waitFor(() => {
      expect(resolveIdToken).toHaveBeenCalled()
      expect(apiMocks.createAuthenticatedDepositChannel).toHaveBeenCalledWith('lazy-privy-token', {
        asset_id: 'usdc',
        chain_key: 'ethereum-mainnet',
      })
    })

    expect(await screen.findByDisplayValue('0xdeposit')).toBeTruthy()
  })

  it('surfaces a Privy configuration hint instead of spinning forever when no auth token is available', async () => {
    const resolveIdToken = vi.fn().mockResolvedValue(undefined)

    render(<DepositRouterPanel resolveIdToken={resolveIdToken} />)

    expect(
      await screen.findByText(/Privy did not return a Moros auth token/i, undefined, {
        timeout: 4000,
      }),
    ).toBeTruthy()
    expect(apiMocks.createAuthenticatedDepositChannel).not.toHaveBeenCalled()
  }, 5000)
})
