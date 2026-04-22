import { renderToString } from 'react-dom/server'
import { describe, expect, it, vi } from 'vitest'
import { BaccaratPage } from './BaccaratPage'

vi.mock('../hooks/useMorosWallet', () => ({
  useMorosWallet: () => ({
    address: '0xabc',
    connect: vi.fn(),
    error: undefined,
    openBaccaratRound: vi.fn(),
    status: 'connected',
  }),
}))

vi.mock('../lib/api', () => ({
  createBaccaratCommitment: vi.fn(),
  fetchCoordinatorTableState: vi.fn(),
  settleBaccaratCommitment: vi.fn(),
}))

describe('BaccaratPage', () => {
  it('renders baccarat betting controls and commit reveal surface', () => {
    const html = renderToString(<BaccaratPage />)

    expect(html).toContain('TIE PAYS 8 TO 1')
    expect(html).toContain('Chip Value')
    expect(html).toContain('Player')
    expect(html).toContain('Banker')
    expect(html).toContain('Tie')
    expect(html).toContain('Total Bet')
    expect(html).toContain('Bet')
  })
})
