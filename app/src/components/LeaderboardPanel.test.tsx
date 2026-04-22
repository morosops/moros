// @vitest-environment jsdom

import { cleanup, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { LeaderboardPanel } from './LeaderboardPanel'
import { useAccountStore } from '../store/account'

const apiMocks = vi.hoisted(() => ({
  fetchBetFeed: vi.fn(),
}))

const authMocks = vi.hoisted(() => ({
  useMorosAuthRuntime: vi.fn(() => ({
    ready: false,
    authenticated: false,
  })),
}))

vi.mock('../lib/api', () => ({
  fetchBetFeed: apiMocks.fetchBetFeed,
}))

vi.mock('./MorosAuthProvider', () => ({
  useMorosAuthRuntime: authMocks.useMorosAuthRuntime,
}))

vi.mock('../hooks/useMorosWallet', () => ({
  useMorosWallet: () => ({
    address: undefined,
  }),
}))

describe('LeaderboardPanel', () => {
  beforeEach(() => {
    useAccountStore.setState({
      userId: undefined,
      walletAddress: undefined,
      authMethod: undefined,
    })
    apiMocks.fetchBetFeed.mockResolvedValue({
      my_bets: [],
      all_bets: [],
      high_rollers: [],
      race_leaderboard: [],
    })
    authMocks.useMorosAuthRuntime.mockReturnValue({
      ready: false,
      authenticated: false,
    })
  })

  afterEach(() => {
    cleanup()
    vi.clearAllMocks()
    useAccountStore.setState({
      userId: undefined,
      walletAddress: undefined,
      authMethod: undefined,
    })
  })

  it('uses the canonical Moros account address even when no execution wallet is connected', async () => {
    useAccountStore.setState({
      userId: 'user-1',
      walletAddress: '0xfeedface',
      authMethod: 'google',
    })

    render(<LeaderboardPanel initialTab="my_bets" />)

    await waitFor(() => {
      expect(apiMocks.fetchBetFeed).toHaveBeenCalledWith({
        userId: 'user-1',
        walletAddress: '0xfeedface',
      })
    })

    expect(screen.getByText('No settled bets for this account yet.')).toBeTruthy()
    expect(screen.queryByText('Login to show your settled bets.')).toBeNull()
  })

  it('does not show a login prompt when a canonical Moros account exists without a connected wallet', async () => {
    useAccountStore.setState({
      userId: 'user-2',
      walletAddress: undefined,
      authMethod: 'google',
    })

    render(<LeaderboardPanel initialTab="my_bets" />)

    await waitFor(() => {
      expect(apiMocks.fetchBetFeed).toHaveBeenCalledWith({
        userId: 'user-2',
        walletAddress: undefined,
      })
    })

    expect(screen.getByText('No settled bets for this account yet.')).toBeTruthy()
    expect(screen.queryByText('Login to show your settled bets.')).toBeNull()
  })

  it('shows a loading state instead of a login prompt when auth is ready but canonical account sync is still pending', async () => {
    authMocks.useMorosAuthRuntime.mockReturnValue({
      ready: true,
      authenticated: true,
    })

    render(<LeaderboardPanel initialTab="my_bets" />)

    await waitFor(() => {
      expect(apiMocks.fetchBetFeed).toHaveBeenCalledWith({
        userId: undefined,
        walletAddress: undefined,
      })
    })

    expect(screen.getByText('Loading your settled bets...')).toBeTruthy()
    expect(screen.queryByText('Login to show your settled bets.')).toBeNull()
  })
})
