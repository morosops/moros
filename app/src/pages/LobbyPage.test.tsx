import { MemoryRouter } from 'react-router-dom'
import { renderToString } from 'react-dom/server'
import { describe, expect, it, vi } from 'vitest'
import { LobbyPage } from './LobbyPage'

vi.mock('../components/MorosAuthProvider', () => ({
  useMorosAuthRuntime: () => ({
    ready: false,
    authenticated: false,
  }),
}))

vi.mock('../hooks/useMorosWallet', () => ({
  useMorosWallet: () => ({
    address: undefined,
    balanceFormatted: undefined,
    connect: vi.fn(),
    disconnect: vi.fn(),
    error: undefined,
    status: 'idle',
  }),
}))

vi.mock('../lib/api', () => ({
  fetchBetFeed: vi.fn().mockResolvedValue({
    my_bets: [],
    all_bets: [],
    high_rollers: [],
    race_leaderboard: [],
  }),
  fetchCoordinatorTableState: vi.fn().mockResolvedValue({
    state: {
      table: {
        table_id: 1,
        table_contract: '0x1',
        game_kind: 'dice',
        status: 'active',
        min_wager: '1000000000000000000',
        max_wager: '100000000000000000000',
      },
      house_available: '0',
      house_locked: '0',
      recommended_house_bankroll: '0',
      fully_covered_max_wager: '0',
    },
    live_players: 0,
  }),
}))

describe('LobbyPage', () => {
  it('renders real game cards and empty bet sections without mock activity rows', () => {
    const html = renderToString(
      <MemoryRouter>
        <LobbyPage />
      </MemoryRouter>,
    )

    expect(html).toContain('Casino')
    expect(html).toContain('Dice')
    expect(html).toContain('Roulette')
    expect(html).toContain('Baccarat')
    expect(html).toContain('Blackjack')
    expect(html).toContain('lobby-game-card__live')
    expect(html).toContain('My Bets')
    expect(html).toContain('All Bets')
    expect(html).toContain('High Rollers')
    expect(html).toContain('Race Leaderboard')
    expect(html).toContain('Leaderboard')
    expect(html).not.toContain('386 playing')
  })
})
