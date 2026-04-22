// @vitest-environment jsdom
import { fireEvent, render, screen } from '@testing-library/react'
import { renderToString } from 'react-dom/server'
import { describe, expect, it, vi } from 'vitest'
import {
  MOROS_DICE_ROLL_DOMAIN,
  MOROS_SERVER_SEED_DOMAIN,
  computePoseidonOnElements,
  feltToModulo,
} from '../lib/poseidon'
import { DicePage, nextWagerForAutoMode, stopReason, verifyDiceProof } from './DicePage'

vi.mock('../hooks/useMorosWallet', () => ({
  useMorosWallet: () => ({
    address: '0xabc',
    connect: vi.fn(),
    error: undefined,
    openDiceRound: vi.fn(),
    status: 'connected',
  }),
}))

vi.mock('../lib/api', () => ({
  createDiceCommitment: vi.fn(async () => ({ commitment: { commitment_id: 1 } })),
  fetchCoordinatorTableState: vi.fn(async () => ({
    state: {
      table: {
        max_wager: (100n * 10n ** 18n).toString(),
      },
      player_balance: '0',
      fully_covered_max_wager: (100n * 10n ** 18n).toString(),
      house_available: (1000n * 10n ** 18n).toString(),
    },
  })),
  settleDiceCommitment: vi.fn(),
}))

vi.mock('../components/DiceProbabilitySlider', () => ({
  DiceProbabilitySlider: () => <div className="dice-probability-slider">slider</div>,
}))

vi.mock('../components/GameUtilityBar', () => ({
  GameUtilityBar: ({ fairnessStatus }: { fairnessStatus?: { label?: string } }) => (
    <div>
      <span>Fairness</span>
      <span>{fairnessStatus?.label ?? 'Awaiting next settled round'}</span>
    </div>
  ),
}))

describe('DicePage', () => {
  it('renders the dice desk, custom slider, and fairness rail', () => {
    const html = renderToString(<DicePage />)

    expect(html).toContain('Manual')
    expect(html).toContain('Auto')
    expect(html).toContain('Advanced')
    expect(html).toContain('Bet Amount')
    expect(html).toContain('Profit on Win')
    expect(html).toContain('Roll Over')
    expect(html).toContain('Win Chance')
    expect(html).toContain('dice-probability-slider')
    expect(html).toContain('Fairness')
    expect(html).toContain('Bet')
  })

  it('reproduces a locally generated proof from revealed seed data', async () => {
    const serverSeed = '0x1'
    const clientSeed = '0x2'
    const player = '0xabc'
    const roundId = 17
    const serverSeedHash = await computePoseidonOnElements([MOROS_SERVER_SEED_DOMAIN, serverSeed])
    const mixed = await computePoseidonOnElements([
      MOROS_DICE_ROLL_DOMAIN,
      serverSeed,
      clientSeed,
      player,
      roundId.toString(),
    ])
    const rollBps = feltToModulo(mixed, 10000n)
    const round = {
      round_id: roundId,
      table_id: 1,
      player,
      wager: '0',
      status: 'settled',
      transcript_root: '0x0',
      commitment_id: 0,
      server_seed_hash: serverSeedHash,
      client_seed: clientSeed,
      target_bps: 4950,
      roll_over: true,
      roll_bps: rollBps,
      chance_bps: 4950,
      multiplier_bps: 20000,
      payout: '0',
      win: rollBps > 4950,
    }
    const proof = await verifyDiceProof(round, serverSeed)

    expect(proof).toEqual({
      seedHashMatches: true,
      rollMatches: true,
      rollBps,
    })
  })

  it('adjusts auto-bet wager according to win/loss controls', () => {
    expect(nextWagerForAutoMode(100n, 100n, true, false, 'reset', '0', 'reset', '0')).toBe(100n)
    expect(nextWagerForAutoMode(100n, 100n, true, true, 'increase', '50', 'reset', '0')).toBe(150n)
    expect(nextWagerForAutoMode(100n, 100n, false, true, 'reset', '0', 'increase', '100')).toBe(200n)
  })

  it('derives stop reasons for max-bet, profit, and loss caps', () => {
    const strategy = {
      id: 'martingale',
      name: 'Martingale',
      kind: 'martingale' as const,
      stepBps: 10000,
      delayRounds: 0,
      maxBets: 3,
      stopProfit: '2',
      stopLoss: '4',
      conditions: [],
    }

    expect(stopReason(strategy, 3, 0n, 0n)).toContain('3 bets')
    expect(stopReason(strategy, 1, 2n * 10n ** 18n, 0n)).toContain('session profit')
    expect(stopReason(strategy, 1, 0n, 4n * 10n ** 18n)).toContain('session loss')
  })


  it('creates a strategy through the advanced bet modal', () => {
    const { container } = render(<DicePage />)

    const advancedTab = Array.from(container.querySelectorAll('[role="tab"]')).find(
      (node) => node.getAttribute('aria-label') === 'Advanced' || node.textContent?.trim() === 'Advanced',
    )
    expect(advancedTab).toBeTruthy()
    fireEvent.click(advancedTab as Element)

    const createButton = Array.from(container.querySelectorAll('button')).find((node) => node.textContent?.trim() === 'Create Strategy')
    expect(createButton).toBeTruthy()
    fireEvent.click(createButton as Element)

    const dialog = container.querySelector('[role="dialog"][aria-label="Advanced Bet"]') as HTMLDivElement | null
    expect(dialog).toBeTruthy()

    const nameInput = dialog?.querySelector('input[type="text"]') as HTMLInputElement | null
    expect(nameInput).toBeTruthy()
    fireEvent.change(nameInput as HTMLInputElement, {
      target: { value: 'Ladder' },
    })

    const getStartedButton = Array.from(dialog?.querySelectorAll('button') ?? []).find((node) => node.textContent?.trim() === 'Get Started')
    expect(getStartedButton).toBeTruthy()
    fireEvent.click(getStartedButton as Element)

    expect(dialog?.textContent).toContain('Ladder')
    expect(dialog?.textContent).toContain('Condition 1')

    const addConditionButton = Array.from(dialog?.querySelectorAll('button') ?? []).find((node) => node.textContent?.trim() === 'Add Condition Block')
    expect(addConditionButton).toBeTruthy()
    fireEvent.click(addConditionButton as Element)

    expect(dialog?.textContent).toContain('Condition 2')

    const saveButton = Array.from(dialog?.querySelectorAll('button') ?? []).find((node) => node.textContent?.trim() === 'Save Strategy')
    expect(saveButton).toBeTruthy()
    fireEvent.click(saveButton as Element)

    const strategySelect = container.querySelector('select') as HTMLSelectElement | null
    expect(strategySelect).toBeTruthy()
    expect(Array.from((strategySelect as HTMLSelectElement).options).some((option) => option.text === 'Ladder')).toBe(true)
  }, 10000)
})
