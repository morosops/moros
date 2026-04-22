import { renderToString } from 'react-dom/server'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { BlackjackPage } from './BlackjackPage'
import { useUiStore } from '../store/ui'

vi.mock('../hooks/useMorosWallet', () => ({
  useMorosWallet: () => ({
    address: '0xabc',
    connect: vi.fn(),
    ensureGameplaySession: vi.fn(),
    error: undefined,
    status: 'connected',
    walletStatus: 'connected',
  }),
}))

vi.mock('../lib/api', () => ({
  createCoordinatorHand: vi.fn(),
  fetchCoordinatorHandFairness: vi.fn(),
  fetchCoordinatorHandView: vi.fn(),
  fetchCoordinatorTableState: vi.fn(),
  relayHandAction: vi.fn(),
}))

describe('BlackjackPage', () => {
  beforeEach(() => {
    useUiStore.setState({
      selectedTable: 'blackjack-main-floor',
      sidebarCollapsed: false,
    })
  })

  it('renders the simplified blackjack table and grouped controls', () => {
    const html = renderToString(<BlackjackPage />)

    expect(html).toContain('Bet Amount')
    expect(html).toContain('BLACKJACK PAYS 3 TO 2')
    expect(html).toContain('INSURANCE PAYS 2 TO 1')
    expect(html).toContain('Hit')
    expect(html).toContain('Stand')
    expect(html).toContain('Bet')
  })

  it('does not expose local practice blackjack language', () => {
    const html = renderToString(<BlackjackPage />)

    expect(html.toLowerCase()).not.toContain('practice')
  })
})
