import { useEffect, useMemo, useRef, useState } from 'react'
import {
  createRouletteCommitment,
  settleRouletteCommitment,
  type RouletteSpinView,
  type SettleRouletteCommitmentResponse,
} from '../lib/api'
import { GameUtilityBar } from '../components/GameUtilityBar'
import { OriginalsFairnessStepper, type OriginalsFairnessStage } from '../components/OriginalsFairnessStepper'
import { deriveMorosAccountState, resolveMorosPrimaryActionLabel } from '../lib/account-state'
import { morosGameBySlug, ROULETTE_MAX_BET_SPOTS } from '../lib/game-config'
import {
  MOROS_ROULETTE_SPIN_DOMAIN,
  MOROS_SERVER_SEED_DOMAIN,
  computePoseidonOnElements,
  feltToModulo,
} from '../lib/poseidon'
import { formatStrk, formatWagerInput, parseStrkInputToWei } from '../lib/format'
import { randomClientSeed, sameFelt } from '../lib/random'
import { useMorosWallet } from '../hooks/useMorosWallet'
import { useOriginalsCommitment } from '../hooks/useOriginalsCommitment'
import { useTableState } from '../hooks/useTableState'
import { useAccountStore } from '../store/account'
import { useToastStore } from '../store/toast'
import { useWalletStore } from '../store/wallet'

const rouletteTableId = morosGameBySlug('roulette')?.tableId ?? 3
const rouletteWheelOrder = [0, 32, 15, 19, 4, 21, 2, 25, 17, 34, 6, 27, 13, 36, 11, 30, 8, 23, 10, 5, 24, 16, 33, 1, 20, 14, 31, 9, 22, 18, 29, 7, 28, 12, 35, 3, 26]
const rouletteSegmentAngle = 360 / rouletteWheelOrder.length
const redNumbers = new Set([1, 3, 5, 7, 9, 12, 14, 16, 18, 19, 21, 23, 25, 27, 30, 32, 34, 36])
const numberRows = [
  [3, 6, 9, 12, 15, 18, 21, 24, 27, 30, 33, 36],
  [2, 5, 8, 11, 14, 17, 20, 23, 26, 29, 32, 35],
  [1, 4, 7, 10, 13, 16, 19, 22, 25, 28, 31, 34],
]
const chipOptions = [
  { label: '1', value: '1' },
  { label: '10', value: '10' },
  { label: '100', value: '100' },
  { label: '1K', value: '1000' },
]
const rouletteWheelViewSize = 400
const rouletteWheelCenter = rouletteWheelViewSize / 2
const rouletteOuterRimRadius = 190
const rouletteNumberRingOuterRadius = 174
const rouletteNumberRingInnerRadius = 122
const rouletteSeparatorRingRadius = 117
const roulettePocketRingOuterRadius = 112
const roulettePocketRingInnerRadius = 76
const rouletteBallTrackRadius = 182
const TABLE_STATE_FRESH_MS = 5_000

type BetDraft = {
  key: string
  kind: number
  selection: number
  label: string
  amountWei: bigint
}

type OutsideBetDescriptor = {
  key: string
  kind: number
  selection: number
  label: string
  tone?: 'red' | 'black' | 'green'
}

function parseStrkInput(value: string) {
  return parseStrkInputToWei(value, { allowZero: false, label: 'STRK amount' })
}

async function verifyRouletteProof(spin?: RouletteSpinView, serverSeed?: string) {
  if (!spin || !serverSeed) {
    return undefined
  }

  const serverSeedHash = await computePoseidonOnElements([MOROS_SERVER_SEED_DOMAIN, serverSeed])
  const mixed = await computePoseidonOnElements([
    MOROS_ROULETTE_SPIN_DOMAIN,
    serverSeed,
    spin.client_seed,
    spin.player,
    spin.spin_id.toString(),
  ])
  const result = feltToModulo(mixed, 37n)

  return {
    seedHashMatches: sameFelt(serverSeedHash, spin.server_seed_hash),
    resultMatches: result === spin.result_number,
    result,
  }
}

function pocketTone(value: number) {
  if (value === 0) {
    return 'green'
  }
  return redNumbers.has(value) ? 'red' : 'black'
}

function betKey(kind: number, selection: number) {
  return `${kind}:${selection}`
}

function columnSelectionForRow(rowIndex: number) {
  return 3 - rowIndex
}

function spinMatchesBet(result: number, kind: number, selection: number) {
  switch (kind) {
    case 0:
      return result === selection
    case 1:
      return redNumbers.has(result)
    case 2:
      return result !== 0 && !redNumbers.has(result)
    case 3:
      return result !== 0 && result % 2 === 1
    case 4:
      return result !== 0 && result % 2 === 0
    case 5:
      return result >= 1 && result <= 18
    case 6:
      return result >= 19 && result <= 36
    case 7:
      return result >= (selection - 1) * 12 + 1 && result <= selection * 12
    case 8:
      return result !== 0 && ((result - 1) % 3) + 1 === selection
    case 9:
      return result !== 0 && Math.floor((result - 1) / 3) === selection
    case 10:
      if (selection >= 100 && selection <= 102) {
        return result === 0 || result === selection - 99
      }
      if (selection >= 40) {
        const start = selection - 40
        return result === start || result === start + 1
      }
      return result === selection || result === selection + 3
    case 11:
      return result === selection || result === selection + 1 || result === selection + 3 || result === selection + 4
    case 12: {
      const start = selection * 3 + 1
      return result >= start && result < start + 6
    }
    case 13:
      return result === 0 || result === 1 || result === 2 || result === 3
    default:
      return false
  }
}

function compactBetLabel(kind: number, selection: number) {
  switch (kind) {
    case 0:
      return String(selection)
    case 1:
      return 'Red'
    case 2:
      return 'Black'
    case 3:
      return 'Odd'
    case 4:
      return 'Even'
    case 5:
      return '1-18'
    case 6:
      return '19-36'
    case 7:
      return selection === 1 ? '1-12' : selection === 2 ? '13-24' : '25-36'
    case 8:
      return `2:1 · ${selection}`
    case 9: {
      const start = selection * 3 + 1
      return `${start}-${start + 2}`
    }
    case 10:
      if (selection >= 100 && selection <= 102) return `0/${selection - 99}`
      if (selection >= 40) return `${selection - 40}/${selection - 39}`
      return `${selection}/${selection + 3}`
    case 11:
      return `${selection}/${selection + 1}/${selection + 3}/${selection + 4}`
    case 12: {
      const start = selection * 3 + 1
      return `${start}-${start + 5}`
    }
    case 13:
      return '0/1/2/3'
    default:
      return 'Bet'
  }
}

function cx(...parts: Array<string | false | null | undefined>) {
  return parts.filter(Boolean).join(' ')
}

function playerOutcomeTone(payout?: string | null, wager?: string | null) {
  if (!payout || !wager) {
    return undefined
  }

  const payoutWei = BigInt(payout)
  const wagerWei = BigInt(wager)
  if (payoutWei > wagerWei) {
    return 'win'
  }
  if (payoutWei === wagerWei) {
    return 'push'
  }
  return 'loss'
}

function polarPoint(cx: number, cy: number, radius: number, angleDegrees: number) {
  const radians = (angleDegrees * Math.PI) / 180
  return {
    x: cx + Math.cos(radians) * radius,
    y: cy + Math.sin(radians) * radius,
  }
}

function ringSegmentPath(
  cx: number,
  cy: number,
  outerRadius: number,
  innerRadius: number,
  startAngle: number,
  endAngle: number,
) {
  const outerStart = polarPoint(cx, cy, outerRadius, startAngle)
  const outerEnd = polarPoint(cx, cy, outerRadius, endAngle)
  const innerEnd = polarPoint(cx, cy, innerRadius, endAngle)
  const innerStart = polarPoint(cx, cy, innerRadius, startAngle)
  const largeArc = endAngle - startAngle > 180 ? 1 : 0

  return [
    `M ${outerStart.x} ${outerStart.y}`,
    `A ${outerRadius} ${outerRadius} 0 ${largeArc} 1 ${outerEnd.x} ${outerEnd.y}`,
    `L ${innerEnd.x} ${innerEnd.y}`,
    `A ${innerRadius} ${innerRadius} 0 ${largeArc} 0 ${innerStart.x} ${innerStart.y}`,
    'Z',
  ].join(' ')
}

function rouletteSegmentFill(value: number) {
  if (value === 0) {
    return '#3c7757'
  }
  return redNumbers.has(value) ? '#973931' : '#1a1d25'
}

function roulettePocketFill(value: number) {
  if (value === 0) {
    return '#2d5840'
  }
  return redNumbers.has(value) ? '#973931' : '#1a1d25'
}

export function RoulettePage() {
  const { address, connect, error: walletError, openRouletteSpin, pendingLabel, status: walletStatus } = useMorosWallet()
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
  const [selectedChipIndex, setSelectedChipIndex] = useState(1)
  const [bets, setBets] = useState<BetDraft[]>([])
  const [spin, setSpin] = useState<RouletteSpinView>()
  const [lastSpin, setLastSpin] = useState<SettleRouletteCommitmentResponse>()
  const [activeCommitment, setActiveCommitment] = useState<number>()
  const [statusMessage, setStatusMessage] = useState<string>()
  const [isSpinning, setIsSpinning] = useState(false)
  const [wheelSettling, setWheelSettling] = useState(false)
  const [theatreMode, setTheatreMode] = useState(false)
  const [clientSeedDraft, setClientSeedDraft] = useState<string>()
  const [proof, setProof] = useState<Awaited<ReturnType<typeof verifyRouletteProof>>>()
  const [transientFairnessPhase, setTransientFairnessPhase] = useState<OriginalsFairnessStage>()
  const wheelFrameRef = useRef<number | null>(null)
  const wheelRotationRef = useRef(0)
  const ballAngleRef = useRef(-90)
  const spinInFlightRef = useRef(false)
  const [wheelRotation, setWheelRotation] = useState(0)
  const [ballAngle, setBallAngle] = useState(-90)
  const pushToast = useToastStore((state) => state.pushToast)
  const { lastLoadedAt, refreshTableState, tableState } = useTableState(rouletteTableId, resolvedWalletAddress)
  const { commitmentReady, takeCommitment } = useOriginalsCommitment(createRouletteCommitment, {
    enabled: accountState.signedIn,
  })

  const selectedChip = chipOptions[selectedChipIndex]
  const selectedChipWei = useMemo(() => parseStrkInput(selectedChip.value), [selectedChip.value])
  const totalWagerWei = useMemo(() => bets.reduce((sum, bet) => sum + bet.amountWei, 0n), [bets])
  const totalBetDisplay = totalWagerWei === 0n ? '0' : formatWagerInput(totalWagerWei)
  const wheelSegments = useMemo(
    () =>
      rouletteWheelOrder.map((value, index) => {
        const startAngle = -90 + index * rouletteSegmentAngle
        const endAngle = startAngle + rouletteSegmentAngle
        const centerAngle = startAngle + rouletteSegmentAngle / 2
        const numberPoint = polarPoint(
          rouletteWheelCenter,
          rouletteWheelCenter,
          (rouletteNumberRingOuterRadius + rouletteNumberRingInnerRadius) / 2,
          centerAngle,
        )

        return {
          value,
          key: `wheel-${value}-${index}`,
          tone: pocketTone(value),
          numberPath: ringSegmentPath(
            rouletteWheelCenter,
            rouletteWheelCenter,
            rouletteNumberRingOuterRadius,
            rouletteNumberRingInnerRadius,
            startAngle,
            endAngle,
          ),
          pocketPath: ringSegmentPath(
            rouletteWheelCenter,
            rouletteWheelCenter,
            roulettePocketRingOuterRadius,
            roulettePocketRingInnerRadius,
            startAngle,
            endAngle,
          ),
          separatorOuterStart: polarPoint(
            rouletteWheelCenter,
            rouletteWheelCenter,
            rouletteNumberRingInnerRadius,
            startAngle,
          ),
          separatorOuterEnd: polarPoint(
            rouletteWheelCenter,
            rouletteWheelCenter,
            rouletteNumberRingOuterRadius,
            startAngle,
          ),
          separatorPocketStart: polarPoint(
            rouletteWheelCenter,
            rouletteWheelCenter,
            roulettePocketRingInnerRadius,
            startAngle,
          ),
          separatorPocketEnd: polarPoint(
            rouletteWheelCenter,
            rouletteWheelCenter,
            roulettePocketRingOuterRadius,
            startAngle,
          ),
          numberPoint,
          numberRotation: centerAngle + 90,
        }
      }),
    [],
  )
  const ballPosition = useMemo(
    () => polarPoint(rouletteWheelCenter, rouletteWheelCenter, rouletteBallTrackRadius, ballAngle),
    [ballAngle],
  )

  const dozens: OutsideBetDescriptor[] = [
    { key: 'dozen-1', kind: 7, selection: 1, label: '1 to 12' },
    { key: 'dozen-2', kind: 7, selection: 2, label: '13 to 24' },
    { key: 'dozen-3', kind: 7, selection: 3, label: '25 to 36' },
  ]

  const outsideBets: OutsideBetDescriptor[] = [
    { key: 'low', kind: 5, selection: 0, label: '1 to 18' },
    { key: 'even', kind: 4, selection: 0, label: 'Even' },
    { key: 'red', kind: 1, selection: 0, label: 'Red', tone: 'red' },
    { key: 'black', kind: 2, selection: 0, label: 'Black', tone: 'black' },
    { key: 'odd', kind: 3, selection: 0, label: 'Odd' },
    { key: 'high', kind: 6, selection: 0, label: '19 to 36' },
  ]

  const insideBetGroups: Array<{ label: string; bets: OutsideBetDescriptor[] }> = [
    {
      label: 'Top line',
      bets: [{ key: 'top-line', kind: 13, selection: 0, label: compactBetLabel(13, 0), tone: 'green' }],
    },
    {
      label: 'Splits',
      bets: [
        ...[100, 101, 102].map((selection) => ({
          key: `zero-split-${selection}`,
          kind: 10,
          selection,
          label: compactBetLabel(10, selection),
        })),
        ...Array.from({ length: 33 }, (_, index) => {
          const selection = index + 1
          return {
            key: `vertical-split-${selection}`,
            kind: 10,
            selection,
            label: compactBetLabel(10, selection),
          }
        }),
        ...Array.from({ length: 35 }, (_, index) => index + 1)
          .filter((selection) => selection % 3 !== 0)
          .map((selection) => ({
            key: `horizontal-split-${selection}`,
            kind: 10,
            selection: selection + 40,
            label: compactBetLabel(10, selection + 40),
          })),
      ],
    },
    {
      label: 'Streets',
      bets: Array.from({ length: 12 }, (_, selection) => ({
        key: `street-${selection}`,
        kind: 9,
        selection,
        label: compactBetLabel(9, selection),
      })),
    },
    {
      label: 'Corners',
      bets: Array.from({ length: 32 }, (_, index) => index + 1)
        .filter((selection) => selection % 3 !== 0)
        .map((selection) => ({
          key: `corner-${selection}`,
          kind: 11,
          selection,
          label: compactBetLabel(11, selection),
        })),
    },
    {
      label: 'Six lines',
      bets: Array.from({ length: 11 }, (_, selection) => ({
        key: `six-line-${selection}`,
        kind: 12,
        selection,
        label: compactBetLabel(12, selection),
      })),
    },
  ]

  useEffect(() => {
    let cancelled = false

    if (!spin || !lastSpin?.server_seed) {
      setProof(undefined)
      return () => {
        cancelled = true
      }
    }

    void verifyRouletteProof(spin, lastSpin.server_seed)
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
  }, [lastSpin?.server_seed, spin])

  useEffect(() => {
    return () => {
      if (wheelFrameRef.current) {
        window.cancelAnimationFrame(wheelFrameRef.current)
      }
    }
  }, [])

  function syncWheel(nextRotation: number) {
    wheelRotationRef.current = nextRotation
    setWheelRotation(nextRotation)
  }

  function syncBall(nextAngle: number) {
    ballAngleRef.current = nextAngle
    setBallAngle(nextAngle)
  }

  function stopWheelMotion() {
    if (wheelFrameRef.current !== null) {
      window.cancelAnimationFrame(wheelFrameRef.current)
      wheelFrameRef.current = null
    }
  }

  function startWheelMotion() {
    stopWheelMotion()
    setWheelSettling(false)
    let lastTime = performance.now()

    const tick = (time: number) => {
      const deltaSeconds = (time - lastTime) / 1000
      lastTime = time
      syncWheel(wheelRotationRef.current + deltaSeconds * 300)
      syncBall(ballAngleRef.current - deltaSeconds * 420)
      wheelFrameRef.current = window.requestAnimationFrame(tick)
    }

    wheelFrameRef.current = window.requestAnimationFrame(tick)
  }

  function settleWheel(resultNumber: number) {
    stopWheelMotion()
    const resultIndex = rouletteWheelOrder.indexOf(resultNumber)
    if (resultIndex === -1) {
      return
    }

    setWheelSettling(true)
    const current = wheelRotationRef.current
    const currentModulo = ((current % 360) + 360) % 360
    const finalModulo = ((360 - resultIndex * rouletteSegmentAngle) % 360 + 360) % 360
    const delta = ((finalModulo - currentModulo) % 360 + 360) % 360
    const target = current + 1440 + delta
    const currentBall = ballAngleRef.current
    const currentBallModulo = ((currentBall % 360) + 360) % 360
    const finalBallModulo = 270
    const backwardDelta = ((currentBallModulo - finalBallModulo) % 360 + 360) % 360
    const targetBall = currentBall - 1080 - backwardDelta
    const start = current
    const startBall = currentBall
    const duration = 3200
    const startTime = performance.now()

    const tick = (time: number) => {
      const progress = Math.min(1, (time - startTime) / duration)
      const eased = 1 - Math.pow(1 - progress, 4)
      syncWheel(start + (target - start) * eased)
      syncBall(startBall + (targetBall - startBall) * eased)
      if (progress < 1) {
        wheelFrameRef.current = window.requestAnimationFrame(tick)
      } else {
        setWheelSettling(false)
      }
    }

    wheelFrameRef.current = window.requestAnimationFrame(tick)
  }

  function cycleChip(direction: 'prev' | 'next') {
    setSelectedChipIndex((current) => {
      if (direction === 'prev') {
        return current === 0 ? chipOptions.length - 1 : current - 1
      }
      return current === chipOptions.length - 1 ? 0 : current + 1
    })
  }

  function addBet(kind: number, selection: number, label: string) {
    setStatusMessage(undefined)
    setBets((current) => {
      const key = betKey(kind, selection)
      const existing = current.find((entry) => entry.key === key)
      if (existing) {
        return current.map((entry) =>
          entry.key === key ? { ...entry, amountWei: entry.amountWei + selectedChipWei } : entry,
        )
      }
      if (current.length >= ROULETTE_MAX_BET_SPOTS) {
        setStatusMessage(`Roulette supports up to ${ROULETTE_MAX_BET_SPOTS} unique bet spots per spin.`)
        return current
      }
      return [...current, { key, kind, selection, label, amountWei: selectedChipWei }]
    })
  }

  function scaleBets(direction: 'half' | 'double') {
    if (bets.length === 0) {
      return
    }

    setBets((current) =>
      current
        .map((entry) => ({
          ...entry,
          amountWei: direction === 'half' ? entry.amountWei / 2n : entry.amountWei * 2n,
        }))
        .filter((entry) => entry.amountWei > 0n),
    )
  }

  function betAmountFor(kind: number, selection: number) {
    const bet = bets.find((entry) => entry.kind === kind && entry.selection === selection)
    return bet ? formatWagerInput(bet.amountWei) : undefined
  }

  function updateTotalBetInput(nextValue: string) {
    setStatusMessage(undefined)
    try {
      const parsed = parseStrkInputToWei(nextValue, { allowZero: true, label: 'STRK amount' })
      setBets((current) => {
        if (parsed === 0n) {
          return []
        }
        const currentTotal = current.reduce((sum, entry) => sum + entry.amountWei, 0n)
        if (current.length === 0 || currentTotal === 0n) {
          setStatusMessage('Select a roulette spot before editing the total wager.')
          return current
        }
        if (current.length === 1) {
          return [{ ...current[0], amountWei: parsed }]
        }

        let allocated = 0n
        return current.map((entry, index) => {
          if (index === current.length - 1) {
            return { ...entry, amountWei: parsed - allocated }
          }
          const nextAmount = entry.amountWei * parsed / currentTotal
          allocated += nextAmount
          return { ...entry, amountWei: nextAmount }
        }).filter((entry) => entry.amountWei > 0n)
      })
    } catch (error) {
      const message = error instanceof Error ? error.message : 'Enter a valid STRK wager.'
      setStatusMessage(message)
      pushToast({ message, tone: 'warn', title: 'Roulette wager' })
    }
  }

  async function handleSpin() {
    if (spinInFlightRef.current) {
      return
    }

    spinInFlightRef.current = true
    setIsSpinning(true)
    setSpin(undefined)
    setLastSpin(undefined)
    setProof(undefined)
    setActiveCommitment(undefined)
    setTransientFairnessPhase('commit')
    setStatusMessage(undefined)
    try {
      let playerAddress = address
      if (!playerAddress) {
        const connected = await connect()
        playerAddress = connected.address
      }
      if (!playerAddress) {
        throw new Error('Connect a wallet before spinning.')
      }
      if (bets.length === 0) {
        setTransientFairnessPhase(undefined)
        setStatusMessage('Place at least one chip before spinning.')
        return
      }

      startWheelMotion()
      playerAddress =
        useAccountStore.getState().walletAddress ??
        useWalletStore.getState().address ??
        playerAddress
      const canReuseTableState =
        Boolean(tableState)
        && typeof lastLoadedAt === 'number'
        && resolvedWalletAddress?.toLowerCase() === playerAddress.toLowerCase()
        && Date.now() - lastLoadedAt <= TABLE_STATE_FRESH_MS
      const [liveTable, commitment] = await Promise.all([
        canReuseTableState
          ? Promise.resolve({ live_players: undefined, state: tableState! })
          : refreshTableState(playerAddress),
        takeCommitment(),
      ])
      setActiveCommitment(commitment.commitment.commitment_id)
      setTransientFairnessPhase('open')
      const clientSeed = clientSeedDraft ?? randomClientSeed()
      const bankrollBalanceWei = liveTable.state.player_balance ?? '0'
      const bankrollShortfall =
        totalWagerWei > BigInt(bankrollBalanceWei) ? totalWagerWei - BigInt(bankrollBalanceWei) : 0n
      setClientSeedDraft(undefined)
      setStatusMessage(
        bankrollShortfall > 0n
          ? 'Deposit STRK into your Moros balance before betting.'
          : commitmentReady
            ? 'Opening the spin...'
            : 'Roulette seed hash committed. Opening the spin...',
      )
      await openRouletteSpin({
        tableId: rouletteTableId,
        totalWagerWei: totalWagerWei.toString(),
        bankrollBalanceWei,
        clientSeed,
        commitmentId: commitment.commitment.commitment_id,
        bets: bets.map((bet) => ({
          kind: bet.kind,
          selection: bet.selection,
          amountWei: bet.amountWei.toString(),
        })),
      })
      setTransientFairnessPhase('reveal')
      setStatusMessage('Spin opened. Revealing server seed and settling on Starknet...')
      const result = await settleRouletteCommitment(commitment.commitment.commitment_id)
      setTransientFairnessPhase('verify')
      setSpin(result.spin)
      setLastSpin(result)
      void refreshTableState(playerAddress)
      settleWheel(result.spin.result_number)
      setStatusMessage(`Roulette spin ${result.spin.spin_id} landed on ${result.spin.result_number}.`)
    } catch (error) {
      stopWheelMotion()
      setWheelSettling(false)
      const message = error instanceof Error ? error.message : 'Failed to spin roulette.'
      setTransientFairnessPhase(undefined)
      setStatusMessage(message)
      pushToast({ message, tone: 'error', title: 'Roulette error' })
    } finally {
      spinInFlightRef.current = false
      setIsSpinning(false)
    }
  }

  const fairnessFields = [
    { label: 'Commitment id', value: spin?.commitment_id ? `#${spin.commitment_id}` : 'Pending' },
    { label: 'Server seed hash', value: spin?.server_seed_hash ?? 'Pending', sensitive: true },
    { label: 'Server seed', value: lastSpin?.server_seed ?? 'Revealed after settlement', sensitive: true },
    { label: 'Client seed', value: clientSeedDraft ?? spin?.client_seed ?? 'Generated on spin', sensitive: true },
    { label: 'Nonce', value: spin?.spin_id ? `#${spin.spin_id}` : 'Pending' },
  ]

  const liveStats = [
    { label: 'Last result', value: spin ? String(spin.result_number) : '—' },
    { label: 'Bet spots', value: `${bets.length}/${ROULETTE_MAX_BET_SPOTS}` },
    { label: 'Total bet', value: formatStrk(totalWagerWei) },
    { label: 'Bankroll', value: formatStrk(tableState?.player_balance) },
  ]

  const settingsStats = [
    { label: 'Table max', value: formatStrk(tableState?.table.max_wager) },
    { label: 'House available', value: formatStrk(tableState?.house_available) },
    { label: 'Covered max', value: formatStrk(tableState?.fully_covered_max_wager) },
  ]
  const walletBusy =
    walletStatus === 'connecting' || walletStatus === 'preparing' || walletStatus === 'funding' || walletStatus === 'confirming'
  const primaryActionLabel = isSpinning
    ? 'Spinning...'
    : resolveMorosPrimaryActionLabel({
      accountState,
      pendingLabel,
      readyLabel: 'Bet',
      walletBusy,
    })
  const spinOutcome = spin ? playerOutcomeTone(spin.payout, spin.wager) : undefined

  function rouletteBetOutcome(kind: number, selection: number) {
    const settledBet = spin?.bets.find((bet) => bet.kind === kind && bet.selection === selection)
    return settledBet ? playerOutcomeTone(settledBet.payout, settledBet.amount) : undefined
  }

  const pageClassName = `page page--roulette${theatreMode ? ' page--theatre' : ''}`

  return (
    <section className={pageClassName}>
      <div className="roulette-desk">
        <aside className="roulette-sidebar">
          <section className="roulette-sidebar__section">
            <div className="roulette-sidebar__label-row">
              <span>Chip Value</span>
            </div>
            <div className="roulette-chip-strip">
              <button aria-label="Previous chip" className="roulette-chip-strip__arrow" onClick={() => cycleChip('prev')} type="button">
                ‹
              </button>
              <div className="roulette-chip-strip__list">
                {chipOptions.map((chip, index) => (
                  <button
                    className={index === selectedChipIndex ? 'roulette-chip roulette-chip--active' : 'roulette-chip'}
                    key={chip.label}
                    onClick={() => setSelectedChipIndex(index)}
                    type="button"
                  >
                    {chip.label}
                  </button>
                ))}
              </div>
              <button aria-label="Next chip" className="roulette-chip-strip__arrow" onClick={() => cycleChip('next')} type="button">
                ›
              </button>
            </div>
          </section>

          <section className="roulette-sidebar__section">
            <div className="roulette-sidebar__label-row">
              <span>Total Bet</span>
              <strong>{formatStrk(totalWagerWei)}</strong>
            </div>
            <div className="roulette-total-row">
              <label className="dice-token-input">
                <input
                  className="text-input text-input--large dice-token-input__field"
                  inputMode="decimal"
                  onChange={(event) => updateTotalBetInput(event.target.value)}
                  value={totalBetDisplay}
                />
                <span className="dice-token-input__token dice-token-input__token--label">STRK</span>
              </label>
              <button className="chip" onClick={() => scaleBets('half')} type="button">½</button>
              <button className="chip" onClick={() => scaleBets('double')} type="button">2×</button>
            </div>
          </section>

          <div className="roulette-sidebar__footer">
            <div className="game-sidebar__action-row">
              <button
                className="button button--wide game-primary-action"
                disabled={isSpinning || walletBusy}
                onClick={() => void handleSpin()}
                type="button"
              >
                {primaryActionLabel}
              </button>
              <button className="button button--ghost button--wide" disabled={bets.length === 0} onClick={() => setBets([])} type="button">
                Clear Bets
              </button>
            </div>
            {statusMessage ? <p className="stack-note">{statusMessage}</p> : null}
            {walletError ? <p className="stack-note stack-note--error">{walletError}</p> : null}
          </div>
        </aside>

        <article className="roulette-main">
          <div className="roulette-main-surface">
            <div className="roulette-wheel-stage">
              {spin ? (
                <div className={cx(
                  'roulette-wheel-result',
                  `roulette-wheel-result--${pocketTone(spin.result_number)}`,
                  spinOutcome && `roulette-wheel-result--player-${spinOutcome}`,
                )}>
                  Result {spin.result_number}
                </div>
              ) : null}

              <div className="roulette-wheel-frame">
                <div className="roulette-wheel-pointer" />
                <div className="roulette-wheel-board">
                  <div
                    className={wheelSettling ? 'roulette-wheel-board__wheel roulette-wheel-board__wheel--settling' : 'roulette-wheel-board__wheel'}
                    style={{ transform: `rotate(${wheelRotation}deg)` }}
                  >
                    <svg
                      aria-hidden="true"
                      className="roulette-wheel-board__svg"
                      viewBox={`0 0 ${rouletteWheelViewSize} ${rouletteWheelViewSize}`}
                    >
                      <circle
                        className="roulette-wheel-board__outer-rim"
                        cx={rouletteWheelCenter}
                        cy={rouletteWheelCenter}
                        r={rouletteOuterRimRadius}
                      />
                      <circle
                        className="roulette-wheel-board__outer-rim-highlight"
                        cx={rouletteWheelCenter}
                        cy={rouletteWheelCenter}
                        r={rouletteOuterRimRadius - 5}
                      />

                      {wheelSegments.map((segment) => (
                        <path
                          key={`${segment.key}-number`}
                          className={`roulette-wheel-board__number-segment roulette-wheel-board__number-segment--${segment.tone}`}
                          d={segment.numberPath}
                          fill={rouletteSegmentFill(segment.value)}
                        />
                      ))}

                      <circle
                        className="roulette-wheel-board__number-ring-border"
                        cx={rouletteWheelCenter}
                        cy={rouletteWheelCenter}
                        r={rouletteNumberRingOuterRadius}
                      />
                      <circle
                        className="roulette-wheel-board__number-ring-border"
                        cx={rouletteWheelCenter}
                        cy={rouletteWheelCenter}
                        r={rouletteNumberRingInnerRadius}
                      />

                      {wheelSegments.map((segment) => (
                        <line
                          className="roulette-wheel-board__separator-line"
                          key={`${segment.key}-number-separator`}
                          x1={segment.separatorOuterStart.x}
                          x2={segment.separatorOuterEnd.x}
                          y1={segment.separatorOuterStart.y}
                          y2={segment.separatorOuterEnd.y}
                        />
                      ))}

                      {wheelSegments.map((segment) => (
                        <text
                          className={cx(
                            'roulette-wheel-board__number-label',
                            spin?.result_number === segment.value && 'roulette-wheel-board__number-label--winner',
                          )}
                          dominantBaseline="middle"
                          key={`${segment.key}-label`}
                          textAnchor="middle"
                          transform={`rotate(${segment.numberRotation} ${segment.numberPoint.x} ${segment.numberPoint.y})`}
                          x={segment.numberPoint.x}
                          y={segment.numberPoint.y}
                        >
                          {segment.value}
                        </text>
                      ))}

                      <circle
                        className="roulette-wheel-board__separator-ring"
                        cx={rouletteWheelCenter}
                        cy={rouletteWheelCenter}
                        r={rouletteSeparatorRingRadius}
                      />

                      {wheelSegments.map((segment) => (
                        <path
                          key={`${segment.key}-pocket`}
                          className={`roulette-wheel-board__pocket-segment roulette-wheel-board__pocket-segment--${segment.tone}`}
                          d={segment.pocketPath}
                          fill={roulettePocketFill(segment.value)}
                        />
                      ))}

                      <circle
                        className="roulette-wheel-board__pocket-ring-border"
                        cx={rouletteWheelCenter}
                        cy={rouletteWheelCenter}
                        r={roulettePocketRingOuterRadius}
                      />
                      <circle
                        className="roulette-wheel-board__pocket-ring-border"
                        cx={rouletteWheelCenter}
                        cy={rouletteWheelCenter}
                        r={roulettePocketRingInnerRadius}
                      />

                      {wheelSegments.map((segment) => (
                        <line
                          className="roulette-wheel-board__separator-line roulette-wheel-board__separator-line--inner"
                          key={`${segment.key}-pocket-separator`}
                          x1={segment.separatorPocketStart.x}
                          x2={segment.separatorPocketEnd.x}
                          y1={segment.separatorPocketStart.y}
                          y2={segment.separatorPocketEnd.y}
                        />
                      ))}

                    </svg>
                    <div className="roulette-wheel-board__spindle">
                    </div>
                  </div>
                  <svg
                    aria-hidden="true"
                    className="roulette-wheel-board__ball-layer"
                    viewBox={`0 0 ${rouletteWheelViewSize} ${rouletteWheelViewSize}`}
                  >
                    <circle
                      className="roulette-wheel-board__ball-shadow"
                      cx={ballPosition.x + 2.6}
                      cy={ballPosition.y + 3.8}
                      r={8.4}
                    />
                    <circle
                      className="roulette-wheel-board__ball"
                      cx={ballPosition.x}
                      cy={ballPosition.y}
                      r={8}
                    />
                  </svg>
                </div>
              </div>
            </div>

            <div className="roulette-table-shell">
              <div className="roulette-table-layout">
              <button
                className={cx(
                  'roulette-bet-cell roulette-bet-cell--zero',
                  betAmountFor(0, 0) && 'roulette-bet-cell--placed',
                  rouletteBetOutcome(0, 0) === 'win' && 'roulette-bet-cell--player-win',
                  rouletteBetOutcome(0, 0) === 'loss' && 'roulette-bet-cell--player-loss',
                  spin?.result_number === 0 && 'roulette-bet-cell--winning',
                )}
                onClick={() => addBet(0, 0, '0')}
                type="button"
              >
                <span>0</span>
                {betAmountFor(0, 0) ? <i>{betAmountFor(0, 0)}</i> : null}
              </button>

                <div className="roulette-number-grid">
                  {numberRows.map((row, rowIndex) =>
                    row.map((number) => (
                      <button
                        className={cx(
                          'roulette-bet-cell',
                          pocketTone(number) === 'red' && 'roulette-bet-cell--red',
                          pocketTone(number) === 'black' && 'roulette-bet-cell--black',
                          betAmountFor(0, number) && 'roulette-bet-cell--placed',
                          rouletteBetOutcome(0, number) === 'win' && 'roulette-bet-cell--player-win',
                          rouletteBetOutcome(0, number) === 'loss' && 'roulette-bet-cell--player-loss',
                          spin?.result_number === number && 'roulette-bet-cell--winning',
                        )}
                        key={number}
                        onClick={() => addBet(0, number, compactBetLabel(0, number))}
                        style={{ gridColumn: String(Math.floor((number - 1) / 3) + 1), gridRow: String(rowIndex + 1) }}
                        type="button"
                      >
                        <span>{number}</span>
                        {betAmountFor(0, number) ? <i>{betAmountFor(0, number)}</i> : null}
                      </button>
                    )),
                  )}
                </div>

                <div className="roulette-column-grid">
                  {[0, 1, 2].map((rowIndex) => {
                    const selection = columnSelectionForRow(rowIndex)
                    const winning = spin ? spinMatchesBet(spin.result_number, 8, selection) : false
                    return (
                      <button
                        className={cx(
                          'roulette-bet-cell roulette-bet-cell--outside roulette-bet-cell--column',
                          betAmountFor(8, selection) && 'roulette-bet-cell--placed',
                          rouletteBetOutcome(8, selection) === 'win' && 'roulette-bet-cell--player-win',
                          rouletteBetOutcome(8, selection) === 'loss' && 'roulette-bet-cell--player-loss',
                          winning && 'roulette-bet-cell--winning',
                        )}
                        key={`column-${selection}`}
                        onClick={() => addBet(8, selection, compactBetLabel(8, selection))}
                        type="button"
                      >
                        <span>2:1</span>
                        {betAmountFor(8, selection) ? <i>{betAmountFor(8, selection)}</i> : null}
                      </button>
                    )
                  })}
                </div>
              </div>

              <div className="roulette-outside-row roulette-outside-row--dozens">
                {dozens.map((entry) => {
                  const winning = spin ? spinMatchesBet(spin.result_number, entry.kind, entry.selection) : false
                  return (
                    <button
                      className={cx(
                        'roulette-bet-cell roulette-bet-cell--outside',
                        betAmountFor(entry.kind, entry.selection) && 'roulette-bet-cell--placed',
                        rouletteBetOutcome(entry.kind, entry.selection) === 'win' && 'roulette-bet-cell--player-win',
                        rouletteBetOutcome(entry.kind, entry.selection) === 'loss' && 'roulette-bet-cell--player-loss',
                        winning && 'roulette-bet-cell--winning',
                      )}
                      key={entry.key}
                      onClick={() => addBet(entry.kind, entry.selection, entry.label)}
                      type="button"
                    >
                      <span>{entry.label}</span>
                      {betAmountFor(entry.kind, entry.selection) ? <i>{betAmountFor(entry.kind, entry.selection)}</i> : null}
                    </button>
                  )
                })}
              </div>

              <div className="roulette-outside-row roulette-outside-row--chances">
                {outsideBets.map((entry) => {
                  const winning = spin ? spinMatchesBet(spin.result_number, entry.kind, entry.selection) : false
                  return (
                    <button
                      className={cx(
                        'roulette-bet-cell roulette-bet-cell--outside',
                        entry.tone === 'red' && 'roulette-bet-cell--red',
                        entry.tone === 'black' && 'roulette-bet-cell--black',
                        betAmountFor(entry.kind, entry.selection) && 'roulette-bet-cell--placed',
                        rouletteBetOutcome(entry.kind, entry.selection) === 'win' && 'roulette-bet-cell--player-win',
                        rouletteBetOutcome(entry.kind, entry.selection) === 'loss' && 'roulette-bet-cell--player-loss',
                        winning && 'roulette-bet-cell--winning',
                      )}
                      key={entry.key}
                      onClick={() => addBet(entry.kind, entry.selection, entry.label)}
                      type="button"
                    >
                      <span>{entry.label}</span>
                      {betAmountFor(entry.kind, entry.selection) ? <i>{betAmountFor(entry.kind, entry.selection)}</i> : null}
                    </button>
                  )
                })}
              </div>
            </div>
          </div>

          <OriginalsFairnessStepper
            committed={Boolean(activeCommitment || spin?.server_seed_hash)}
            label="Roulette commit-reveal verification progress"
            opened={Boolean(spin)}
            phase={transientFairnessPhase}
            verified={Boolean(proof && proof.seedHashMatches && proof.resultMatches)}
            warning={Boolean(proof && (!proof.seedHashMatches || !proof.resultMatches))}
          />

          <GameUtilityBar
            fairnessFields={fairnessFields}
            fairnessStatus={{
              label: proof ? (proof.seedHashMatches && proof.resultMatches ? 'Verification passed' : 'Verification mismatch') : 'Awaiting next settled spin',
              tone: proof ? (proof.seedHashMatches && proof.resultMatches ? 'good' : 'warn') : 'neutral',
            }}
            fairnessSummary={
              proof
                ? proof.seedHashMatches && proof.resultMatches
                  ? 'The revealed seed reproduces the same roulette number that settled onchain.'
                  : 'The revealed seed does not reproduce the settled number. Recheck the commitment hash and spin id.'
                : 'Each spin commits a server seed hash before the relayer opens the bet basket. Reveal data appears after settlement.'
            }
            liveStats={liveStats}
            onRegenerate={() => {
              setClientSeedDraft(randomClientSeed())
              setStatusMessage('Queued a fresh client seed for the next roulette spin.')
            }}
            onToggleTheatre={setTheatreMode}
            regenerateLabel="New client seed"
            settingsStats={settingsStats}
            theatreMode={theatreMode}
          />
        </article>
      </div>
    </section>
  )
}
