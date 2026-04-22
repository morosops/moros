import { renderToString } from 'react-dom/server'
import { describe, expect, it, vi } from 'vitest'
import { RoulettePage } from './RoulettePage'

vi.mock('../hooks/useMorosWallet', () => ({
  useMorosWallet: () => ({
    address: '0xabc',
    connect: vi.fn(),
    error: undefined,
    openRouletteSpin: vi.fn(),
    status: 'connected',
  }),
}))

vi.mock('../lib/api', () => ({
  createRouletteCommitment: vi.fn(),
  fetchCoordinatorTableState: vi.fn(),
  settleRouletteCommitment: vi.fn(),
}))

describe('RoulettePage', () => {
  it('renders roulette betting controls and wheel surface', () => {
    const html = renderToString(<RoulettePage />)

    expect(html).toContain('roulette-wheel-board')
    expect(html).toContain('roulette-wheel-board__svg')
    expect(html).toContain('roulette-wheel-board__ball-layer')
    expect(html).toContain('Red')
    expect(html).toContain('Black')
    expect(html).toContain('Chip Value')
    expect(html).toContain('Total Bet')
    expect(html).toContain('Bet')
  })
})
