import { formatStrk, parseOptionalStrkInput } from './format'

export type StrategyKind =
  | 'martingale'
  | 'delayed_martingale'
  | 'paroli'
  | 'dalembert'
  | 'strat'
  | 'martingale_ish'

export type StrategyProfile = {
  id: string
  name: string
  kind: StrategyKind
  stepBps: number
  delayRounds: number
  maxBets: number
  stopProfit: string
  stopLoss: string
  conditions: ConditionBlock[]
}

export type StrategyDraft = Omit<StrategyProfile, 'id' | 'conditions'>
export type StrategyEditorStep = 'start' | 'builder'
export type ConditionType = 'bet' | 'profit'
export type ConditionTriggerMode = 'every' | 'after'
export type ConditionTriggerContext = 'bets' | 'wins' | 'losses'
export type ConditionProfitComparator = 'gt' | 'gte' | 'lt' | 'lte'
export type ConditionAction =
  | 'increase_bet_amount'
  | 'decrease_bet_amount'
  | 'increase_win_chance'
  | 'decrease_win_chance'
  | 'add_to_bet_amount'
  | 'subtract_from_bet_amount'
  | 'add_to_win_chance'
  | 'subtract_from_win_chance'
  | 'set_bet_amount'
  | 'set_win_chance'
  | 'switch_over_under'
  | 'reset_bet_amount'
  | 'reset_win_chance'
  | 'stop_autobet'

export type ConditionBlock = {
  id: string
  type: ConditionType
  triggerMode: ConditionTriggerMode
  triggerCount: number
  triggerContext: ConditionTriggerContext
  profitComparator: ConditionProfitComparator
  profitValue: string
  action: ConditionAction
  actionValue: string
  collapsed: boolean
}

export function createConditionBlock(id: string = crypto.randomUUID(), overrides: Partial<ConditionBlock> = {}): ConditionBlock {
  return {
    id,
    type: 'bet',
    triggerMode: 'every',
    triggerCount: 1,
    triggerContext: 'bets',
    profitComparator: 'gt',
    profitValue: '0.00000000',
    action: 'add_to_bet_amount',
    actionValue: '0.00000000',
    collapsed: false,
    ...overrides,
  }
}

export function cloneConditionBlocks(blocks: ConditionBlock[]) {
  return blocks.map((block, index) =>
    createConditionBlock(`condition-${index + 1}-${crypto.randomUUID()}`, {
      ...block,
      collapsed: block.collapsed,
    }),
  )
}

export const defaultConditionBlocks: ConditionBlock[] = [
  createConditionBlock('condition-1'),
  createConditionBlock('condition-2', {
    type: 'profit',
    profitComparator: 'lte',
    profitValue: '0.10000000',
    action: 'decrease_bet_amount',
    actionValue: '0.10000000',
    collapsed: true,
  }),
]

export const defaultStrategies: StrategyProfile[] = [
  {
    id: 'martingale',
    name: 'Martingale',
    kind: 'martingale',
    stepBps: 10000,
    delayRounds: 0,
    maxBets: 24,
    stopProfit: '25',
    stopLoss: '50',
    conditions: cloneConditionBlocks(defaultConditionBlocks),
  },
  {
    id: 'delayed-martingale',
    name: 'Delayed Martingale',
    kind: 'delayed_martingale',
    stepBps: 10000,
    delayRounds: 2,
    maxBets: 24,
    stopProfit: '25',
    stopLoss: '50',
    conditions: cloneConditionBlocks(defaultConditionBlocks),
  },
  {
    id: 'paroli',
    name: 'Paroli',
    kind: 'paroli',
    stepBps: 10000,
    delayRounds: 0,
    maxBets: 20,
    stopProfit: '20',
    stopLoss: '40',
    conditions: cloneConditionBlocks(defaultConditionBlocks),
  },
  {
    id: 'dalembert',
    name: "D'Alembert",
    kind: 'dalembert',
    stepBps: 10000,
    delayRounds: 0,
    maxBets: 20,
    stopProfit: '20',
    stopLoss: '40',
    conditions: cloneConditionBlocks(defaultConditionBlocks),
  },
  {
    id: 'strat',
    name: 'Strat',
    kind: 'strat',
    stepBps: 5000,
    delayRounds: 0,
    maxBets: 18,
    stopProfit: '16',
    stopLoss: '32',
    conditions: cloneConditionBlocks(defaultConditionBlocks),
  },
  {
    id: 'martingale-ish',
    name: 'Martingale-ish',
    kind: 'martingale_ish',
    stepBps: 5000,
    delayRounds: 0,
    maxBets: 18,
    stopProfit: '16',
    stopLoss: '32',
    conditions: cloneConditionBlocks(defaultConditionBlocks),
  },
]

export function strategyLabel(kind: StrategyKind) {
  switch (kind) {
    case 'martingale':
      return 'Martingale'
    case 'delayed_martingale':
      return 'Delayed Martingale'
    case 'paroli':
      return 'Paroli'
    case 'dalembert':
      return "D'Alembert"
    case 'strat':
      return 'Strat'
    case 'martingale_ish':
      return 'Martingale-ish'
  }
}

export function strategyDescription(strategy: StrategyProfile) {
  switch (strategy.kind) {
    case 'martingale':
      return 'Reset on win, double after a loss.'
    case 'delayed_martingale':
      return `Wait ${strategy.delayRounds} losses before increasing.`
    case 'paroli':
      return 'Press wins, reset after losses.'
    case 'dalembert':
      return 'Add one unit on loss, remove one on win.'
    case 'strat':
      return 'Aggressive hybrid that leans into streak reversals.'
    case 'martingale_ish':
      return 'Controlled progression with a lighter loss increase.'
  }
}

export function conditionActionLabel(action: ConditionAction) {
  switch (action) {
    case 'increase_bet_amount':
      return 'Increase bet amount'
    case 'decrease_bet_amount':
      return 'Decrease bet amount'
    case 'increase_win_chance':
      return 'Increase win chance'
    case 'decrease_win_chance':
      return 'Decrease win chance'
    case 'add_to_bet_amount':
      return 'Add to bet amount'
    case 'subtract_from_bet_amount':
      return 'Subtract from bet amount'
    case 'add_to_win_chance':
      return 'Add to win chance'
    case 'subtract_from_win_chance':
      return 'Subtract from win chance'
    case 'set_bet_amount':
      return 'Set bet amount'
    case 'set_win_chance':
      return 'Set win chance'
    case 'switch_over_under':
      return 'Switch over/under'
    case 'reset_bet_amount':
      return 'Reset bet amount'
    case 'reset_win_chance':
      return 'Reset win chance'
    case 'stop_autobet':
    default:
      return 'Stop autobet'
  }
}

export function conditionTriggerContextLabel(value: ConditionTriggerContext) {
  switch (value) {
    case 'bets':
      return 'Bets'
    case 'wins':
      return 'Wins'
    case 'losses':
    default:
      return 'Losses'
  }
}

export function conditionProfitComparatorLabel(value: ConditionProfitComparator) {
  switch (value) {
    case 'gt':
      return 'Greater than'
    case 'gte':
      return 'Greater than or equal to'
    case 'lt':
      return 'Lower than'
    case 'lte':
    default:
      return 'Lower than or equal to'
  }
}

export function conditionActionValueKind(action: ConditionAction) {
  switch (action) {
    case 'increase_bet_amount':
    case 'decrease_bet_amount':
    case 'add_to_bet_amount':
    case 'subtract_from_bet_amount':
    case 'set_bet_amount':
      return 'strk' as const
    case 'increase_win_chance':
    case 'decrease_win_chance':
    case 'add_to_win_chance':
    case 'subtract_from_win_chance':
    case 'set_win_chance':
      return 'percent' as const
    case 'switch_over_under':
    case 'reset_bet_amount':
    case 'reset_win_chance':
    case 'stop_autobet':
    default:
      return 'none' as const
  }
}

export function conditionActionNeedsValue(action: ConditionAction) {
  return conditionActionValueKind(action) !== 'none'
}

export function conditionActionValueUnit(action: ConditionAction) {
  switch (conditionActionValueKind(action)) {
    case 'strk':
      return 'STRK'
    case 'percent':
      return '%'
    default:
      return undefined
  }
}

export function normalizeConditionValue(value: string, kind: 'strk' | 'percent' = 'strk') {
  const normalized = value.trim()
  if (!normalized) {
    return kind === 'percent' ? '0.00' : '0.00000000'
  }
  const parsed = Number.parseFloat(normalized)
  if (!Number.isFinite(parsed) || parsed < 0) {
    return kind === 'percent' ? '0.00' : '0.00000000'
  }
  return kind === 'percent' ? parsed.toFixed(2) : parsed.toFixed(8)
}

export function conditionSummary(block: ConditionBlock) {
  const actionLabel = conditionActionLabel(block.action)
  const actionUnit = conditionActionValueUnit(block.action)
  const actionKind = conditionActionValueKind(block.action)
  const actionSuffix = conditionActionNeedsValue(block.action)
    ? ` ${normalizeConditionValue(block.actionValue, actionKind === 'percent' ? 'percent' : 'strk')}${actionUnit ? ` ${actionUnit}` : ''}`
    : ''
  const triggerSummary = block.type === 'bet'
    ? `${block.triggerMode === 'every' ? 'Every' : 'After'} ${block.triggerCount} ${conditionTriggerContextLabel(block.triggerContext)}`
    : `Profit ${conditionProfitComparatorLabel(block.profitComparator)} ${normalizeConditionValue(block.profitValue)} STRK`
  return `On ${triggerSummary} -> ${actionLabel}${actionSuffix}`
}

export function makeDraft(strategy?: StrategyProfile): StrategyDraft {
  if (!strategy) {
    return {
      name: 'New strategy',
      kind: 'martingale',
      stepBps: 5000,
      delayRounds: 0,
      maxBets: 20,
      stopProfit: '20',
      stopLoss: '40',
    }
  }

  return {
    name: strategy.name,
    kind: strategy.kind,
    stepBps: strategy.stepBps,
    delayRounds: strategy.delayRounds,
    maxBets: strategy.maxBets,
    stopProfit: strategy.stopProfit,
    stopLoss: strategy.stopLoss,
  }
}

export function increaseByStep(current: bigint, stepBps: number) {
  return current + (current * BigInt(stepBps)) / 10000n
}

export function nextWagerForStrategy(
  strategy: StrategyProfile,
  current: bigint,
  base: bigint,
  won: boolean,
  winStreak: number,
  lossStreak: number,
) {
  switch (strategy.kind) {
    case 'martingale':
      return won ? base : increaseByStep(current, Math.max(strategy.stepBps, 10000))
    case 'delayed_martingale':
      if (won) {
        return base
      }
      return lossStreak <= strategy.delayRounds ? current : increaseByStep(current, Math.max(strategy.stepBps, 10000))
    case 'paroli':
      return won ? increaseByStep(current, Math.max(strategy.stepBps, 10000)) : base
    case 'dalembert':
      return won ? (current > base ? current - base : base) : current + base
    case 'strat':
      return won ? base + (base * BigInt(Math.max(winStreak - 1, 0))) : current + base * 2n
    case 'martingale_ish':
      return won ? base : increaseByStep(current, Math.max(strategy.stepBps, 3500))
  }
}

export function parsePercentInput(value: string) {
  const normalized = value.trim()
  if (!normalized) {
    return 0
  }
  const parsed = Number.parseFloat(normalized)
  if (!Number.isFinite(parsed) || parsed < 0) {
    throw new Error('Percent adjustments must be 0 or greater.')
  }
  return Math.round(parsed * 100)
}

export type AutoAdjustMode = 'reset' | 'increase'

export function nextWagerForAutoMode(
  current: bigint,
  base: bigint,
  won: boolean,
  advancedEnabled: boolean,
  onWinMode: AutoAdjustMode,
  onWinPercent: string,
  onLossMode: AutoAdjustMode,
  onLossPercent: string,
) {
  if (!advancedEnabled) {
    return base
  }

  if (won) {
    if (onWinMode === 'reset') {
      return base
    }

    const onWinBps = parsePercentInput(onWinPercent)
    return onWinBps === 0 ? current : increaseByStep(current, onWinBps)
  }

  if (onLossMode === 'reset') {
    return base
  }

  const onLossBps = parsePercentInput(onLossPercent)
  return onLossBps === 0 ? current : increaseByStep(current, onLossBps)
}

export function stopReason(strategy: StrategyProfile, roundsPlayed: number, sessionProfit: bigint, sessionLoss: bigint) {
  if (roundsPlayed >= strategy.maxBets) {
    return `Auto-bet stopped after ${strategy.maxBets} bets.`
  }

  const stopProfit = parseOptionalStrkInput(strategy.stopProfit)
  if (stopProfit && sessionProfit >= stopProfit) {
    return `Auto-bet locked ${formatStrk(stopProfit)} in session profit.`
  }

  const stopLoss = parseOptionalStrkInput(strategy.stopLoss)
  if (stopLoss && sessionLoss >= stopLoss) {
    return `Auto-bet hit the ${formatStrk(stopLoss)} session loss limit.`
  }

  return undefined
}
