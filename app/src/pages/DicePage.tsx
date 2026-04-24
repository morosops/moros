import { useEffect, useMemo, useRef, useState, type CSSProperties } from 'react'
import {
  createDiceCommitment,
  settleDiceCommitment,
  type DiceRoundView,
  type SettleDiceCommitmentResponse,
} from '../lib/api'
import { DiceProbabilitySlider } from '../components/DiceProbabilitySlider'
import { GameUtilityBar } from '../components/GameUtilityBar'
import { OriginalsFairnessStepper, type OriginalsFairnessStage } from '../components/OriginalsFairnessStepper'
import { DiceHistoryPanel } from '../components/dice/DiceHistoryPanel'
import { DiceStrategyEditor } from '../components/dice/DiceStrategyEditor'
import { deriveMorosAccountState, resolveMorosPrimaryActionLabel } from '../lib/account-state'
import { morosGameBySlug } from '../lib/game-config'
import {
  MOROS_DICE_ROLL_DOMAIN,
  MOROS_SERVER_SEED_DOMAIN,
  computePoseidonOnElements,
  feltToModulo,
} from '../lib/poseidon'
import {
  formatPercentBps as formatPercent,
  formatStrk,
  formatWagerInput,
  parseStrkInput,
} from '../lib/format'
import { resolveEffectiveMorosBalanceWei } from '../lib/game-balance'
import { randomClientSeed, sameFelt } from '../lib/random'
import {
  type AutoAdjustMode,
  type ConditionBlock,
  type StrategyEditorStep,
  type StrategyProfile,
  cloneConditionBlocks,
  conditionActionValueKind,
  createConditionBlock,
  defaultConditionBlocks,
  defaultStrategies,
  makeDraft,
  nextWagerForAutoMode,
  nextWagerForStrategy,
  normalizeConditionValue,
  stopReason,
} from '../lib/dice-strategy'
import { useMorosWallet } from '../hooks/useMorosWallet'
import { clampChance, quotedDicePayout, useDiceGame } from '../hooks/useDiceGame'
import { useOriginalsCommitment } from '../hooks/useOriginalsCommitment'
import { useTableState } from '../hooks/useTableState'
import { useAccountStore } from '../store/account'
import { useGameStore } from '../store/game'
import { useToastStore } from '../store/toast'
import { useWalletStore } from '../store/wallet'

export { nextWagerForAutoMode, stopReason } from '../lib/dice-strategy'

const diceTableId = morosGameBySlug('dice')?.tableId ?? 1
const advancedModeIconStyle = {
  '--icon-url': 'url(/icons/right-up.svg)',
} as CSSProperties
const TABLE_STATE_FRESH_MS = 5_000

type PlayMode = 'manual' | 'auto' | 'advanced'

export async function verifyDiceProof(round?: DiceRoundView, serverSeed?: string) {
  if (!round || !serverSeed) {
    return undefined
  }

  const serverSeedHash = await computePoseidonOnElements([MOROS_SERVER_SEED_DOMAIN, serverSeed])
  const mixed = await computePoseidonOnElements([
    MOROS_DICE_ROLL_DOMAIN,
    serverSeed,
    round.client_seed,
    round.player,
    round.round_id.toString(),
  ])
  const rollBps = feltToModulo(mixed, 10000n)

  return {
    seedHashMatches: sameFelt(serverSeedHash, round.server_seed_hash),
    rollMatches: rollBps === round.roll_bps,
    rollBps,
  }
}

async function buildLocalDiceRound(params: {
  player: string
  tableId: number
  roundId: number
  clientSeed: string
  targetBps: number
  rollOver: boolean
  chanceBps: number
  multiplierBps: number
}): Promise<SettleDiceCommitmentResponse> {
  const serverSeed = randomClientSeed()
  const serverSeedHash = await computePoseidonOnElements([MOROS_SERVER_SEED_DOMAIN, serverSeed])
  const mixed = await computePoseidonOnElements([
    MOROS_DICE_ROLL_DOMAIN,
    serverSeed,
    params.clientSeed,
    params.player,
    params.roundId.toString(),
  ])
  const rollBps = feltToModulo(mixed, 10000n)
  const win = params.rollOver ? rollBps > params.targetBps : rollBps < params.targetBps

  return {
    tx_hash: '0x0',
    server_seed: serverSeed,
    round: {
      round_id: params.roundId,
      table_id: params.tableId,
      player: params.player,
      wager: '0',
      status: 'settled',
      transcript_root: serverSeedHash,
      commitment_id: 0,
      server_seed_hash: serverSeedHash,
      client_seed: params.clientSeed,
      target_bps: params.targetBps,
      roll_over: params.rollOver,
      roll_bps: rollBps,
      chance_bps: params.chanceBps,
      multiplier_bps: params.multiplierBps,
      payout: '0',
      win,
    },
  }
}

function formatConfiguredStop(value: string) {
  const normalized = value.trim()
  if (!normalized || Number.parseFloat(normalized || '0') === 0) {
    return 'Off'
  }

  try {
    return formatStrk(parseStrkInput(normalized))
  } catch {
    return 'Invalid'
  }
}

function parseStopInput(value: string) {
  const normalized = value.trim()
  if (!normalized) {
    return undefined
  }

  const parsed = Number.parseFloat(normalized)
  if (!Number.isFinite(parsed) || parsed === 0) {
    return undefined
  }

  return BigInt(parseStrkInput(normalized))
}

function adjustIntegerInput(current: string, delta: number, min = 0, max = 500) {
  const parsed = Number.parseInt(current, 10)
  const base = Number.isFinite(parsed) ? parsed : min
  return String(Math.min(max, Math.max(min, base + delta)))
}

function adjustPercentInput(current: string, delta: number, min = 0, max = 1000) {
  const parsed = Number.parseFloat(current)
  const base = Number.isFinite(parsed) ? parsed : min
  const next = Math.min(max, Math.max(min, base + delta))
  return Number.isInteger(next) ? String(next) : next.toFixed(2).replace(/0+$/, '').replace(/\.$/, '')
}

export function DicePage() {
  const {
    address,
    balanceFormatted,
    connect,
    ensureGameplaySession,
    error: walletError,
    openDiceRound,
    pendingLabel,
    status: walletStatus,
  } = useMorosWallet()
  const accountUserId = useAccountStore((state) => state.userId)
  const accountWalletAddress = useAccountStore((state) => state.walletAddress)
  const accountState = deriveMorosAccountState({
    authReady: true,
    authenticated: false,
    accountUserId,
    accountWalletAddress,
    runtimeWalletAddress: address,
    walletStatus,
  })
  const { resolvedWalletAddress } = accountState
  const [playMode, setPlayMode] = useState<PlayMode>('manual')
  const [wager, setWager] = useState('0')
  const {
    chanceBps,
    chanceDisplay,
    multiplierBps,
    multiplierDisplay,
    rollOver,
    setChanceBps,
    stepMultiplier,
    targetBps,
    thresholdDisplay,
    toggleRollDirection,
    updateThreshold,
  } = useDiceGame()
  const round = useGameStore((state) => state.diceRound)
  const history = useGameStore((state) => state.diceHistory)
  const lastRoll = useGameStore((state) => state.diceLastRoll)
  const setDiceResult = useGameStore((state) => state.setDiceResult)
  const pushToast = useToastStore((state) => state.pushToast)
  const [activeCommitment, setActiveCommitment] = useState<number>()
  const [clientSeed, setClientSeed] = useState<string>()
  const [clientSeedDraft, setClientSeedDraft] = useState<string>()
  const [statusMessage, setStatusMessage] = useState<string>()
  const [isRolling, setIsRolling] = useState(false)
  const [betCount, setBetCount] = useState('0')
  const [autoAdvancedOpen, setAutoAdvancedOpen] = useState(false)
  const [theatreMode, setTheatreMode] = useState(false)
  const [onWinMode, setOnWinMode] = useState<AutoAdjustMode>('reset')
  const [onLossMode, setOnLossMode] = useState<AutoAdjustMode>('reset')
  const [onWinIncrease, setOnWinIncrease] = useState('0')
  const [onLossIncrease, setOnLossIncrease] = useState('0')
  const [stopOnProfit, setStopOnProfit] = useState('0')
  const [stopOnLoss, setStopOnLoss] = useState('0')
  const [strategies, setStrategies] = useState<StrategyProfile[]>(defaultStrategies)
  const [selectedStrategyId, setSelectedStrategyId] = useState(defaultStrategies[0].id)
  const [strategyEditorOpen, setStrategyEditorOpen] = useState(false)
  const [strategyEditorStep, setStrategyEditorStep] = useState<StrategyEditorStep>('start')
  const [editingStrategyId, setEditingStrategyId] = useState<string | null>(null)
  const [strategyNameInput, setStrategyNameInput] = useState('')
  const [strategyConditionDraft, setStrategyConditionDraft] = useState<ConditionBlock[]>([])
  const [strategySelectedCondition, setStrategySelectedCondition] = useState('')
  const [strategyEditorError, setStrategyEditorError] = useState<string>()
  const autoRunStopRef = useRef(false)
  const rollInFlightRef = useRef(false)
  const autoConfigRef = useRef({
    autoAdvancedOpen: false,
    onLossIncrease: '0',
    onLossMode: 'reset' as AutoAdjustMode,
    onWinIncrease: '0',
    onWinMode: 'reset' as AutoAdjustMode,
    stopOnLoss: '0',
    stopOnProfit: '0',
  })
  const [isAutoRunning, setIsAutoRunning] = useState(false)
  const [isAutoStopping, setIsAutoStopping] = useState(false)
  const [autoHasStartedRound, setAutoHasStartedRound] = useState(false)
  const [proof, setProof] = useState<Awaited<ReturnType<typeof verifyDiceProof>>>()
  const [transientFairnessPhase, setTransientFairnessPhase] = useState<OriginalsFairnessStage>()
  const isAdvancedMode = playMode === 'advanced'
  const isAutomatedMode = playMode === 'auto' || isAdvancedMode
  const { lastLoadedAt, refreshTableState, tableState } = useTableState(diceTableId, resolvedWalletAddress)
  const { commitmentReady, takeCommitment } = useOriginalsCommitment(createDiceCommitment, {
    enabled: accountState.signedIn,
  })

  const selectedStrategy = useMemo(
    () => strategies.find((strategy) => strategy.id === selectedStrategyId) ?? strategies[0],
    [selectedStrategyId, strategies],
  )
  const expectedPayout = useMemo(() => {
    try {
      return formatStrk(quotedDicePayout(parseStrkInput(wager), multiplierBps))
    } catch {
      return '0 STRK'
    }
  }, [multiplierBps, wager])
  const expectedProfit = useMemo(() => {
    try {
      const wagerWei = BigInt(parseStrkInput(wager))
      const payoutWei = BigInt(quotedDicePayout(wagerWei.toString(), multiplierBps))
      return formatStrk((payoutWei - wagerWei).toString())
    } catch {
      return '0 STRK'
    }
  }, [multiplierBps, wager])
  const liveBalance = tableState?.player_balance ? formatStrk(tableState.player_balance) : balanceFormatted ?? '0 STRK'
  const inlineWalletError = walletError && walletError !== statusMessage ? walletError : undefined
  const liveMaxBet = formatStrk(tableState?.table.max_wager ?? (100n * 10n ** 18n).toString())
  const walletBusy =
    walletStatus === 'connecting' || walletStatus === 'preparing' || walletStatus === 'funding' || walletStatus === 'confirming'
  const stopOnProfitDisplay = useMemo(() => formatConfiguredStop(stopOnProfit), [stopOnProfit])
  const stopOnLossDisplay = useMemo(() => formatConfiguredStop(stopOnLoss), [stopOnLoss])
  const manualActionLabel = isRolling
    ? 'Betting...'
    : resolveMorosPrimaryActionLabel({
      accountState,
      pendingLabel,
      readyLabel: 'Bet',
      walletBusy,
    })
  const autoActionReadyLabel = isAutoStopping
    ? 'Finishing Bet'
    : isAutoRunning
      ? 'Stop AutoBet'
      : isRolling
        ? 'Starting AutoBet'
        : 'Start AutoBet'
  const autoActionLabel = resolveMorosPrimaryActionLabel({
    accountState,
    pendingLabel,
    readyLabel: autoActionReadyLabel,
    walletBusy,
  })
  const fairnessPhase =
    transientFairnessPhase ?? (round?.status === 'settled' ? 'verify' : activeCommitment ? 'commit' : undefined)

  useEffect(() => {
    autoConfigRef.current = {
      autoAdvancedOpen,
      onLossIncrease,
      onLossMode,
      onWinIncrease,
      onWinMode,
      stopOnLoss,
      stopOnProfit,
    }
  }, [
    autoAdvancedOpen,
    onLossIncrease,
    onLossMode,
    onWinIncrease,
    onWinMode,
    stopOnLoss,
    stopOnProfit,
  ])

  useEffect(() => {
    let cancelled = false

    if (!round || !lastRoll?.server_seed) {
      setProof(undefined)
      return () => {
        cancelled = true
      }
    }

    void verifyDiceProof(round, lastRoll.server_seed)
      .then((nextProof) => {
        if (!cancelled) {
          setProof(nextProof)
          setTransientFairnessPhase(undefined)
        }
      })
      .catch(() => {
        if (!cancelled) {
          setProof(undefined)
          setTransientFairnessPhase(undefined)
        }
      })

    return () => {
      cancelled = true
    }
  }, [lastRoll?.server_seed, round])

  useEffect(() => () => {
    autoRunStopRef.current = true
  }, [])

  function adjustWager(next: 'half' | 'double') {
    try {
      const currentWei = BigInt(parseStrkInput(wager))
      const adjusted = next === 'half'
        ? currentWei > 1n ? currentWei / 2n : currentWei
        : currentWei * 2n
      setWager(formatWagerInput(adjusted))
    } catch {
      setWager(next === 'half' ? '1' : '20')
    }
  }

  function openCreateStrategy() {
    setEditingStrategyId(null)
    setStrategyEditorStep('start')
    setStrategyNameInput('')
    setStrategyConditionDraft([])
    setStrategySelectedCondition('')
    setStrategyEditorError(undefined)
    setStrategyEditorOpen(true)
  }

  function openEditStrategy(conditionId?: string) {
    if (!selectedStrategy) {
      return
    }

    const nextConditions = cloneConditionBlocks(
      selectedStrategy.conditions.length ? selectedStrategy.conditions : defaultConditionBlocks,
    )
    const sourceIndex = conditionId
      ? Math.max(0, selectedStrategy.conditions.findIndex((block) => block.id === conditionId))
      : 0
    const nextSelectedCondition = nextConditions[sourceIndex]?.id ?? nextConditions[0]?.id ?? ''

    setEditingStrategyId(selectedStrategy.id)
    setStrategyEditorStep('builder')
    setStrategyNameInput(selectedStrategy.name)
    setStrategyConditionDraft(nextConditions)
    setStrategySelectedCondition(nextSelectedCondition)
    setStrategyEditorError(undefined)
    setStrategyEditorOpen(true)
  }

  function closeStrategyEditor() {
    setStrategyEditorOpen(false)
    setStrategyEditorError(undefined)
  }

  function startStrategyBuilder() {
    if (!strategyNameInput.trim()) {
      setStrategyEditorError('Strategy name is required.')
      return
    }

    const nextConditions = strategyConditionDraft.length
      ? strategyConditionDraft
      : [createConditionBlock(`condition-1-${crypto.randomUUID()}`)]

    setStrategyConditionDraft(nextConditions)
    setStrategySelectedCondition(nextConditions[0]?.id ?? '')
    setStrategyEditorError(undefined)
    setStrategyEditorStep('builder')
  }

  function saveStrategy() {
    const normalizedName = strategyNameInput.trim()
    if (!normalizedName) {
      setStrategyEditorError('Strategy name is required.')
      return
    }
    if (strategyConditionDraft.length === 0) {
      setStrategyEditorError('Add at least one condition block.')
      return
    }

    const baseStrategy = strategies.find((strategy) => strategy.id === editingStrategyId) ?? selectedStrategy
    const baseDraft = makeDraft(baseStrategy)
    const profile: StrategyProfile = {
      id: editingStrategyId ?? crypto.randomUUID(),
      name: normalizedName,
      kind: baseDraft.kind,
      stepBps: Math.max(100, Math.min(20000, Math.round(baseDraft.stepBps))),
      delayRounds: Math.max(0, Math.min(10, Math.round(baseDraft.delayRounds))),
      maxBets: Math.max(1, Math.min(500, Math.round(baseDraft.maxBets))),
      stopProfit: baseDraft.stopProfit.trim(),
      stopLoss: baseDraft.stopLoss.trim(),
      conditions: strategyConditionDraft.map((block, index) => ({
        ...block,
        id: `condition-${index + 1}-${crypto.randomUUID()}`,
        profitValue: normalizeConditionValue(block.profitValue),
        actionValue: normalizeConditionValue(
          block.actionValue,
          conditionActionValueKind(block.action) === 'percent' ? 'percent' : 'strk',
        ),
      })),
    }

    setStrategies((current) => {
      if (editingStrategyId) {
        return current.map((strategy) => (strategy.id === editingStrategyId ? profile : strategy))
      }
      return [...current, profile]
    })
    setSelectedStrategyId(profile.id)
    setStrategyEditorOpen(false)
    setStrategyEditorStep('start')
    setStrategyEditorError(undefined)
    setStatusMessage(`${profile.name} saved.`)
    pushToast({ message: `${profile.name} saved.`, tone: 'success', title: 'Strategy updated' })
  }

  function deleteStrategy() {
    if (!selectedStrategy) {
      return
    }
    if (strategies.length === 1) {
      setStatusMessage('Keep at least one strategy profile available for auto-bet.')
      return
    }
    const nextStrategies = strategies.filter((strategy) => strategy.id !== selectedStrategy.id)
    setStrategies(nextStrategies)
    setSelectedStrategyId(nextStrategies[0].id)
    setStrategyEditorOpen(false)
    setStrategyEditorError(undefined)
    setStatusMessage(`${selectedStrategy.name} deleted.`)
    pushToast({ message: `${selectedStrategy.name} deleted.`, tone: 'success', title: 'Strategy deleted' })
  }

  function updateConditionBlock(conditionId: string, updates: Partial<ConditionBlock>) {
    setStrategyConditionDraft((current) =>
      current.map((block) => (block.id === conditionId ? { ...block, ...updates } : block)),
    )
  }

  function focusConditionBlock(conditionId: string) {
    setStrategySelectedCondition(conditionId)
    updateConditionBlock(conditionId, { collapsed: false })
  }

  function addConditionBlock() {
    const nextBlock = createConditionBlock(`condition-${strategyConditionDraft.length + 1}-${crypto.randomUUID()}`)
    setStrategyConditionDraft((current) => [...current, nextBlock])
    setStrategySelectedCondition(nextBlock.id)
  }

  function deleteConditionBlock(conditionId: string) {
    const nextBlocks = strategyConditionDraft.filter((block) => block.id !== conditionId)
    setStrategyConditionDraft(nextBlocks)
    setStrategySelectedCondition(nextBlocks[0]?.id ?? '')
  }

  function stepConditionCount(conditionId: string, delta: number) {
    const activeBlock = strategyConditionDraft.find((block) => block.id === conditionId)
    if (!activeBlock) {
      return
    }
    updateConditionBlock(conditionId, {
      triggerCount: Math.max(1, Math.min(999, activeBlock.triggerCount + delta)),
    })
  }

  async function executeSingleRound(
    playerAddress: string,
    wagerWei: string,
    roundIndex: number,
    totalRounds: number,
    bankrollBalanceWei: string,
  ) {
    if (BigInt(wagerWei) === 0n) {
      setProof(undefined)
      setTransientFairnessPhase('verify')
      const nextClientSeed = roundIndex === 0 && clientSeedDraft ? clientSeedDraft : randomClientSeed()
      const localResult = await buildLocalDiceRound({
        player: playerAddress,
        tableId: diceTableId,
        roundId: Number(Date.now() + roundIndex),
        clientSeed: nextClientSeed,
        targetBps,
        rollOver,
        chanceBps,
        multiplierBps,
      })
      setActiveCommitment(undefined)
      setClientSeed(nextClientSeed)
      if (roundIndex === 0) {
        setClientSeedDraft(undefined)
      }
      setDiceResult(localResult)
      return localResult
    }

    setProof(undefined)
    setActiveCommitment(undefined)
    const nextClientSeed = roundIndex === 0 && clientSeedDraft ? clientSeedDraft : randomClientSeed()
    const bankrollShortfall = BigInt(wagerWei) > BigInt(bankrollBalanceWei) ? BigInt(wagerWei) - BigInt(bankrollBalanceWei) : 0n
    if (bankrollShortfall > 0n) {
      throw new Error('Deposit STRK into your Moros balance before betting.')
    }
    setTransientFairnessPhase(undefined)
    setStatusMessage(
      totalRounds === 1
        ? 'Authorizing gameplay...'
        : `Round ${roundIndex}/${totalRounds}: authorizing gameplay...`,
    )
    await ensureGameplaySession()
    setTransientFairnessPhase('commit')
    setStatusMessage(
      totalRounds === 1
        ? 'Preparing round commitment...'
        : `Round ${roundIndex}/${totalRounds}: preparing commitment...`,
    )
    const commitment = await takeCommitment()
    setActiveCommitment(commitment.commitment.commitment_id)
    setTransientFairnessPhase('open')
    setClientSeed(nextClientSeed)
    if (roundIndex === 0) {
      setClientSeedDraft(undefined)
    }
    setStatusMessage(
      totalRounds === 1
        ? 'Seed hash committed. Opening the round...'
        : `Round ${roundIndex}/${totalRounds}: seed committed. Opening the round...`,
    )
    await openDiceRound({
      tableId: diceTableId,
      wagerWei,
      bankrollBalanceWei,
      targetBps,
      rollOver,
      clientSeed: nextClientSeed,
      commitmentId: commitment.commitment.commitment_id,
    })
    setTransientFairnessPhase('reveal')
    setStatusMessage(
      totalRounds === 1
        ? 'Round opened. Revealing server seed and settling on Starknet...'
        : `Round ${roundIndex}/${totalRounds}: revealing and settling on Starknet...`,
    )
    const result = await settleDiceCommitment(commitment.commitment.commitment_id)
    setTransientFairnessPhase('verify')
    setDiceResult(result)
    return result
  }

  async function handleRoll() {
    if (isAutomatedMode && isAutoRunning) {
      autoRunStopRef.current = true
      setStatusMessage('Stopping auto-bet...')
      setIsAutoStopping(true)
      return
    }

    if (rollInFlightRef.current) {
      return
    }

    rollInFlightRef.current = true
    setIsRolling(true)
    setStatusMessage(undefined)
    try {
      const parsedWagerWei = parseStrkInput(wager)
      const baseWagerWei = BigInt(parsedWagerWei)
      let playerAddress = address
      if (!playerAddress) {
        const connected = await connect()
        playerAddress = connected.address
      }
      if (!playerAddress) {
        throw new Error('Connect a wallet before rolling.')
      }

      setStatusMessage(
        baseWagerWei === 0n
          ? 'Starting zero-wager roll...'
          : commitmentReady
            ? 'Opening round...'
            : 'Preparing round...',
      )
      playerAddress =
        useAccountStore.getState().walletAddress ??
        useWalletStore.getState().address ??
        playerAddress
      if (isAutomatedMode) {
        autoRunStopRef.current = false
        setIsAutoRunning(false)
        setIsAutoStopping(false)
        setAutoHasStartedRound(false)
      }
      const canReuseTableState =
        Boolean(tableState)
        && typeof lastLoadedAt === 'number'
        && resolvedWalletAddress?.toLowerCase() === playerAddress.toLowerCase()
        && Date.now() - lastLoadedAt <= TABLE_STATE_FRESH_MS
      const liveTable = canReuseTableState
        ? { live_players: undefined, state: tableState! }
        : await refreshTableState(playerAddress)
      const tableMax = liveTable.state.table.max_wager ? BigInt(liveTable.state.table.max_wager) : undefined
      const effectiveBalanceWei = await resolveEffectiveMorosBalanceWei(
        playerAddress,
        liveTable.state.player_balance,
      )
      let playerBalance = BigInt(effectiveBalanceWei)
      const configuredRounds = Math.max(0, Number.parseInt(betCount, 10) || 0)
      const isInfiniteMode = playMode === 'auto' && configuredRounds === 0
      const roundsToRun = playMode === 'auto'
        ? isInfiniteMode
          ? Number.POSITIVE_INFINITY
          : Math.min(500, configuredRounds)
        : isAdvancedMode
          ? Number.POSITIVE_INFINITY
        : 1
      let currentWagerWei = baseWagerWei
      let sessionProfit = 0n
      let sessionLoss = 0n
      let autoRoundStarted = false
      let roundsPlayed = 0
      let winStreak = 0
      let lossStreak = 0

      let roundIndex = 0
      while (roundIndex < roundsToRun && !autoRunStopRef.current) {
        if (isAdvancedMode && selectedStrategy) {
          const reason = stopReason(selectedStrategy, roundsPlayed, sessionProfit, sessionLoss)
          if (reason) {
            setStatusMessage(reason)
            break
          }
        }

        roundIndex += 1
        if (playMode === 'auto') {
          const liveAutoConfig = autoConfigRef.current
          const stopProfitCap = parseStopInput(liveAutoConfig.stopOnProfit)
          const stopLossCap = parseStopInput(liveAutoConfig.stopOnLoss)
          if (stopProfitCap && sessionProfit >= stopProfitCap) {
            setStatusMessage(`Auto-bet locked ${formatStrk(stopProfitCap.toString())} in session profit.`)
            break
          }

          if (stopLossCap && sessionLoss >= stopLossCap) {
            setStatusMessage(`Auto-bet hit the ${formatStrk(stopLossCap.toString())} session loss limit.`)
            break
          }
        }

        if (tableMax && currentWagerWei > tableMax) {
          setStatusMessage(`Next wager exceeds the live table max of ${formatStrk(tableMax.toString())}.`)
          break
        }

        if (playerBalance && currentWagerWei > playerBalance) {
          setStatusMessage('Auto-bet stopped because the next wager exceeds your bankroll.')
          break
        }

        const result = await executeSingleRound(
          playerAddress,
          currentWagerWei.toString(),
          roundIndex,
          Number.isFinite(roundsToRun) ? roundsToRun : roundIndex,
          playerBalance.toString(),
        )
        if (isAutomatedMode && !autoRoundStarted) {
          setIsAutoRunning(true)
          setAutoHasStartedRound(true)
          autoRoundStarted = true
        }
        const delta = BigInt(result.round.payout) - BigInt(result.round.wager)
        playerBalance = playerBalance - currentWagerWei + BigInt(result.round.payout)

        if (delta >= 0n) {
          sessionProfit += delta
        } else {
          sessionLoss += -delta
        }

        roundsPlayed += 1
        if (isAdvancedMode && selectedStrategy) {
          if (result.round.win) {
            winStreak += 1
            lossStreak = 0
          } else {
            lossStreak += 1
            winStreak = 0
          }
          currentWagerWei = nextWagerForStrategy(
            selectedStrategy,
            currentWagerWei,
            baseWagerWei,
            result.round.win,
            winStreak,
            lossStreak,
          )
        } else if (playMode === 'auto') {
          const liveAutoConfig = autoConfigRef.current
          currentWagerWei = nextWagerForAutoMode(
            currentWagerWei,
            baseWagerWei,
            result.round.win,
            liveAutoConfig.autoAdvancedOpen,
            liveAutoConfig.onWinMode,
            liveAutoConfig.onWinIncrease,
            liveAutoConfig.onLossMode,
            liveAutoConfig.onLossIncrease,
          )
        }

        setStatusMessage(
          baseWagerWei === 0n
            ? `Practice roll ${result.round.round_id} complete.`
            : result.round.win
              ? `Dice round ${result.round.round_id} won and settled on Starknet.`
              : `Dice round ${result.round.round_id} lost and settled on Starknet.`,
        )

        if (isAutomatedMode && !autoRunStopRef.current && (isInfiniteMode || roundIndex < roundsToRun)) {
          await new Promise((resolve) => {
            window.setTimeout(resolve, baseWagerWei === 0n ? 850 : 780)
          })
        }
      }

      if (isAutomatedMode && autoRunStopRef.current) {
        setStatusMessage('Auto-bet stopped.')
      }

      if (baseWagerWei > 0n) {
        void refreshTableState(playerAddress)
      }
    } catch (error) {
      setTransientFairnessPhase(undefined)
      const message = error instanceof Error ? error.message : 'Failed to roll dice.'
      setStatusMessage(message)
    } finally {
      rollInFlightRef.current = false
      setIsRolling(false)
      setIsAutoRunning(false)
      setIsAutoStopping(false)
      setAutoHasStartedRound(false)
      autoRunStopRef.current = false
    }
  }

  const fairnessFields = [
    { label: 'Commitment id', value: activeCommitment ? `#${activeCommitment}` : 'Pending' },
    { label: 'Server seed hash', value: round?.server_seed_hash ?? 'Pending', sensitive: true },
    { label: 'Server seed', value: lastRoll?.server_seed ?? 'Revealed after settlement', sensitive: true },
    { label: 'Client seed', value: clientSeedDraft ?? clientSeed ?? 'Generated on roll', sensitive: true },
    { label: 'Nonce', value: round ? `#${round.round_id}` : 'Pending' },
  ]

  const fairnessSummary = proof
    ? proof.seedHashMatches && proof.rollMatches
      ? 'Verification passed. The committed hash and revealed server seed reproduce the recorded roll.'
      : 'Verification mismatch. Compare the committed hash, client seed, and settled roll before continuing.'
    : 'Each round commits a server seed hash before the relayer opens the wager. Reveal data appears after settlement.'

  const liveStats = [
    { label: 'Last roll', value: round ? (round.roll_bps / 100).toFixed(2) : '—' },
    { label: 'Win chance', value: formatPercent(chanceBps) },
    { label: 'Tracked rounds', value: String(history.length) },
    { label: 'Bankroll', value: liveBalance },
  ]

  const settingsStats = [
    { label: 'Mode', value: playMode === 'advanced' ? 'Advanced' : playMode === 'manual' ? 'Manual' : 'Auto' },
    { label: 'Table max', value: formatStrk(tableState?.table.max_wager) },
    { label: 'Covered max', value: formatStrk(tableState?.fully_covered_max_wager) },
  ]

  const pageClassName = `page page--dice${theatreMode ? ' page--dice-theatre page--theatre' : ''}`

  return (
    <section className={pageClassName}>
      <div className="dice-workspace">
        <aside className="dice-console">
          <div className="dice-controls-head">
            <div className="dice-segmented" role="tablist" aria-label="Dice mode">
              <button
                aria-selected={playMode === 'manual'}
                className={playMode === 'manual' ? 'dice-segmented__button dice-segmented__button--active' : 'dice-segmented__button'}
                onClick={() => setPlayMode('manual')}
                role="tab"
                type="button"
              >
                Manual
              </button>
              <button
                aria-selected={playMode === 'auto'}
                className={playMode === 'auto' ? 'dice-segmented__button dice-segmented__button--active' : 'dice-segmented__button'}
                onClick={() => setPlayMode('auto')}
                role="tab"
                type="button"
              >
                Auto
              </button>
              <button
                aria-selected={playMode === 'advanced'}
                aria-label="Advanced"
                className={playMode === 'advanced'
                  ? 'dice-segmented__button dice-segmented__button--active dice-segmented__button--icon'
                  : 'dice-segmented__button dice-segmented__button--icon'}
                onClick={() => setPlayMode('advanced')}
                role="tab"
                title="Advanced"
                type="button"
              >
                <span aria-hidden="true" className="dice-segmented__icon-mask" style={advancedModeIconStyle} />
              </button>
            </div>
          </div>

          <section className="dice-control-section">
            <div className="dice-control-section__head">
              <span>Bet Amount</span>
              <strong>Max {liveMaxBet}</strong>
            </div>
            <div className="dice-token-input-row">
              <label className="dice-token-input">
                <input
                  className="text-input text-input--large dice-token-input__field"
                  inputMode="decimal"
                  onChange={(event) => setWager(event.target.value)}
                  value={wager}
                />
                <span className="dice-token-input__token dice-token-input__token--label">STRK</span>
              </label>
              <button className="chip" onClick={() => adjustWager('half')} type="button">½</button>
              <button className="chip" onClick={() => adjustWager('double')} type="button">2×</button>
            </div>
          </section>

          {playMode === 'manual' ? (
            <>
              <button
                className="button button--wide dice-bet-button game-primary-action"
                disabled={isRolling || walletBusy}
                onClick={() => void handleRoll()}
                type="button"
              >
                {manualActionLabel}
              </button>

              <section className="dice-control-section">
                <div className="dice-control-section__head">
                  <span>Profit on Win</span>
                  <strong>{expectedProfit}</strong>
                </div>
                <label className="dice-token-input">
                  <input className="text-input text-input--large dice-token-input__field" readOnly value={expectedProfit.replace(' STRK', '')} />
                </label>
              </section>
            </>
          ) : playMode === 'auto' ? (
            <>
              <section className="dice-control-section">
                <div className="dice-control-section__head">
                  <span>Number of Bets</span>
                </div>
                <label className="dice-token-input dice-token-input--with-stepper">
                  <input
                    className="text-input text-input--large dice-token-input__field"
                    inputMode="numeric"
                    max="500"
                    min="0"
                    onChange={(event) => setBetCount(event.target.value)}
                    value={betCount}
                  />
                  <span className="dice-token-input__token dice-token-input__token--icon">∞</span>
                  <div className="dice-inline-stepper">
                    <button aria-label="Increase number of bets" onClick={() => setBetCount((current) => adjustIntegerInput(current, 1, 0, 500))} type="button">
                      ▲
                    </button>
                    <button aria-label="Decrease number of bets" onClick={() => setBetCount((current) => adjustIntegerInput(current, -1, 0, 500))} type="button">
                      ▼
                    </button>
                  </div>
                </label>
              </section>

              <section className="dice-control-section">
                <div className="dice-control-section__head dice-control-section__head--toggle">
                  <span>Advanced</span>
                  <label className="switch-control">
                    <input
                      checked={autoAdvancedOpen}
                      onChange={(event) => setAutoAdvancedOpen(event.target.checked)}
                      type="checkbox"
                    />
                    <span />
                  </label>
                </div>
              </section>

              {autoAdvancedOpen ? (
                <>
                  <section className="dice-control-section">
                    <div className="dice-auto-rule">
                      <span className="dice-auto-rule__label">On Win</span>
                      <div className="dice-auto-rule__controls">
                        <button
                          className={onWinMode === 'reset' ? 'chip chip--active' : 'chip'}
                          onClick={() => setOnWinMode('reset')}
                          type="button"
                        >
                          Reset
                        </button>
                        <button
                          className={onWinMode === 'increase' ? 'chip chip--active' : 'chip'}
                          onClick={() => setOnWinMode('increase')}
                          type="button"
                        >
                          Increase
                        </button>
                        <label className="dice-percent-input dice-percent-input--with-stepper">
                          <input
                            className="text-input text-input--large"
                            inputMode="decimal"
                            onChange={(event) => setOnWinIncrease(event.target.value)}
                            value={onWinIncrease}
                          />
                          <small>%</small>
                          <div className="dice-inline-stepper">
                            <button
                              aria-label="Increase on-win adjustment"
                              onClick={() => setOnWinIncrease((current) => adjustPercentInput(current, 1))}
                              type="button"
                            >
                              ▲
                            </button>
                            <button
                              aria-label="Decrease on-win adjustment"
                              onClick={() => setOnWinIncrease((current) => adjustPercentInput(current, -1))}
                              type="button"
                            >
                              ▼
                            </button>
                          </div>
                        </label>
                      </div>
                    </div>
                  </section>

                  <section className="dice-control-section">
                    <div className="dice-auto-rule">
                      <span className="dice-auto-rule__label">On Loss</span>
                      <div className="dice-auto-rule__controls">
                        <button
                          className={onLossMode === 'reset' ? 'chip chip--active' : 'chip'}
                          onClick={() => setOnLossMode('reset')}
                          type="button"
                        >
                          Reset
                        </button>
                        <button
                          className={onLossMode === 'increase' ? 'chip chip--active' : 'chip'}
                          onClick={() => setOnLossMode('increase')}
                          type="button"
                        >
                          Increase
                        </button>
                        <label className="dice-percent-input dice-percent-input--with-stepper">
                          <input
                            className="text-input text-input--large"
                            inputMode="decimal"
                            onChange={(event) => setOnLossIncrease(event.target.value)}
                            value={onLossIncrease}
                          />
                          <small>%</small>
                          <div className="dice-inline-stepper">
                            <button
                              aria-label="Increase on-loss adjustment"
                              onClick={() => setOnLossIncrease((current) => adjustPercentInput(current, 1))}
                              type="button"
                            >
                              ▲
                            </button>
                            <button
                              aria-label="Decrease on-loss adjustment"
                              onClick={() => setOnLossIncrease((current) => adjustPercentInput(current, -1))}
                              type="button"
                            >
                              ▼
                            </button>
                          </div>
                        </label>
                      </div>
                    </div>
                  </section>

                  <section className="dice-control-section">
                    <div className="dice-control-section__head">
                      <span>Stop on Profit</span>
                      <strong>{stopOnProfitDisplay}</strong>
                    </div>
                    <label className="dice-token-input">
                      <input
                        className="text-input text-input--large dice-token-input__field"
                        inputMode="decimal"
                        onChange={(event) => setStopOnProfit(event.target.value)}
                        value={stopOnProfit}
                      />
                      <span className="dice-token-input__token dice-token-input__token--label">STRK</span>
                    </label>
                  </section>

                  <section className="dice-control-section">
                    <div className="dice-control-section__head">
                      <span>Stop on Loss</span>
                      <strong>{stopOnLossDisplay}</strong>
                    </div>
                    <label className="dice-token-input">
                      <input
                        className="text-input text-input--large dice-token-input__field"
                        inputMode="decimal"
                        onChange={(event) => setStopOnLoss(event.target.value)}
                        value={stopOnLoss}
                      />
                      <span className="dice-token-input__token dice-token-input__token--label">STRK</span>
                    </label>
                  </section>
                </>
              ) : null}

              <button
                className="button button--wide dice-bet-button game-primary-action"
                disabled={walletBusy || (isRolling && !isAutoRunning && !isAutoStopping)}
                onClick={() => void handleRoll()}
                type="button"
              >
                {autoActionLabel}
              </button>
            </>
          ) : (
            <>
              <section className="dice-control-section">
                <label className="stack-field">
                  <span>Select Strategy</span>
                  <select
                    className="table-select"
                    onChange={(event) => setSelectedStrategyId(event.target.value)}
                    value={selectedStrategyId}
                  >
                    {strategies.map((strategy) => (
                      <option key={strategy.id} value={strategy.id}>
                        {strategy.name}
                      </option>
                    ))}
                  </select>
                </label>
              </section>

              <section className="dice-control-section">
                <div className="stack-field">
                  <span>Conditions</span>
                  <div className="dice-condition-group">
                    {selectedStrategy?.conditions.length ? (
                      selectedStrategy.conditions.map((block, index) => (
                        <button
                          aria-label={`Open condition ${index + 1}`}
                          className="chip"
                          key={block.id}
                          onClick={() => openEditStrategy(block.id)}
                          type="button"
                        >
                          {index + 1}
                        </button>
                      ))
                    ) : (
                      <span className="stack-note">No conditions configured.</span>
                    )}
                  </div>
                </div>
              </section>

              <section className="dice-control-section">
                <div className="dice-strategy-actions">
                  <button className="button button--secondary button--wide" onClick={openCreateStrategy} type="button">
                    Create Strategy
                  </button>
                  <button className="button button--secondary button--wide" onClick={() => openEditStrategy()} type="button">
                    Edit Strategy
                  </button>
                  <button className="button button--secondary button--wide" onClick={deleteStrategy} type="button">
                    Delete Strategy
                  </button>
                </div>
              </section>

              <button
                className="button button--wide dice-bet-button game-primary-action"
                disabled={walletBusy || (isRolling && !isAutoRunning && !isAutoStopping)}
                onClick={() => void handleRoll()}
                type="button"
              >
                {autoActionLabel}
              </button>
            </>
          )}

          {statusMessage ? <p aria-live="polite" className="stack-note">{statusMessage}</p> : null}
          {inlineWalletError ? <p className="stack-note stack-note--error" role="alert">{inlineWalletError}</p> : null}
        </aside>

        <div className="dice-stage-shell dice-stage-shell--structured">
          <div className="dice-stage-surface">
            <div className="dice-slider-stage">
              <DiceProbabilitySlider
                onChangeThresholdBps={updateThreshold}
                resultBps={round?.roll_bps}
                resultKey={round?.round_id}
                resultLabel={round ? (round.roll_bps / 100).toFixed(2) : undefined}
                resultWin={round?.win}
                rollOver={rollOver}
                thresholdBps={targetBps}
              />
            </div>

            <div className="dice-metric-row">
              <div className="dice-metric-box">
                <span>Multiplier</span>
                <div className="dice-metric-input">
                  <input
                    className="text-input"
                    inputMode="decimal"
                    onChange={(event) => {
                      const next = Number.parseFloat(event.target.value)
                      if (!Number.isFinite(next) || next <= 0) {
                        return
                      }
                      setChanceBps(clampChance(Math.round(9900 / next)))
                    }}
                    value={multiplierDisplay}
                  />
                  <div className="dice-metric-input__stepper">
                    <button aria-label="Increase multiplier" onClick={() => stepMultiplier(0.01)} type="button">▲</button>
                    <button aria-label="Decrease multiplier" onClick={() => stepMultiplier(-0.01)} type="button">▼</button>
                  </div>
                </div>
              </div>
              <div className="dice-metric-box">
                <span>{rollOver ? 'Roll Over' : 'Roll Under'}</span>
                <div className="dice-metric-input dice-metric-input--threshold">
                  <input
                    className="text-input"
                    inputMode="decimal"
                    onChange={(event) => {
                      const next = Number.parseFloat(event.target.value)
                      if (!Number.isFinite(next)) {
                        return
                      }
                      updateThreshold(Math.round(next * 100))
                    }}
                    value={thresholdDisplay}
                  />
                  <button aria-label="Switch roll direction" className="dice-direction-toggle" onClick={toggleRollDirection} type="button">
                    {rollOver ? '↻' : '↺'}
                  </button>
                </div>
              </div>
              <div className="dice-metric-box">
                <span>Win Chance</span>
                <label className="dice-metric-box__input">
                  <input
                    className="text-input"
                    inputMode="decimal"
                    onChange={(event) => {
                      const next = Number.parseFloat(event.target.value)
                      if (!Number.isFinite(next)) {
                        return
                      }
                      setChanceBps(clampChance(Math.round(next * 100)))
                    }}
                    value={chanceDisplay}
                  />
                  <small>%</small>
                </label>
              </div>
            </div>

          <OriginalsFairnessStepper
            committed={Boolean(activeCommitment || round?.server_seed_hash)}
            label="Dice commit-reveal verification progress"
            opened={Boolean(round)}
            phase={fairnessPhase}
            verified={Boolean(proof && proof.seedHashMatches && proof.rollMatches)}
            warning={Boolean(proof && (!proof.seedHashMatches || !proof.rollMatches))}
          />

            <div className="dice-stage-meta">
              <DiceHistoryPanel history={history} />
            </div>
          </div>

          <GameUtilityBar
            fairnessFields={fairnessFields}
            fairnessStatus={{
              label: proof ? (proof.seedHashMatches && proof.rollMatches ? 'Verification passed' : 'Verification mismatch') : 'Awaiting next settled round',
              tone: proof ? (proof.seedHashMatches && proof.rollMatches ? 'good' : 'warn') : 'neutral',
            }}
            fairnessSummary={fairnessSummary}
            liveStats={liveStats}
            onRegenerate={() => {
              const nextSeed = randomClientSeed()
              setClientSeedDraft(nextSeed)
              setStatusMessage('Queued a fresh client seed for the next dice round.')
            }}
            onToggleTheatre={setTheatreMode}
            regenerateLabel="New client seed"
            settingsStats={settingsStats}
            theatreMode={theatreMode}
          />
        </div>
      </div>

      {playMode === 'advanced' && strategyEditorOpen ? (
        <DiceStrategyEditor
          conditionDraft={strategyConditionDraft}
          error={strategyEditorError}
          name={strategyNameInput}
          onAddCondition={addConditionBlock}
          onClose={closeStrategyEditor}
          onDeleteCondition={deleteConditionBlock}
          onFocusCondition={focusConditionBlock}
          onNameChange={setStrategyNameInput}
          onSave={saveStrategy}
          onStart={startStrategyBuilder}
          onStepConditionCount={stepConditionCount}
          onUpdateCondition={updateConditionBlock}
          selectedConditionId={strategySelectedCondition}
          step={strategyEditorStep}
        />
      ) : null}
    </section>
  )
}
