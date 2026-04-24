import { useEffect, useMemo, useRef, useState } from 'react'
import {
  createBaccaratCommitment,
  settleBaccaratCommitment,
  type BaccaratRoundView,
  type SettleBaccaratCommitmentResponse,
} from '../lib/api'
import { GameUtilityBar } from '../components/GameUtilityBar'
import { OriginalsFairnessStepper, type OriginalsFairnessStage } from '../components/OriginalsFairnessStepper'
import { deriveMorosAccountState, resolveMorosPrimaryActionLabel } from '../lib/account-state'
import { BACCARAT_MAX_SHOE_DRAW_ATTEMPTS, morosGameBySlug } from '../lib/game-config'
import { useMorosWallet } from '../hooks/useMorosWallet'
import { useOriginalsCommitment } from '../hooks/useOriginalsCommitment'
import { useTableState } from '../hooks/useTableState'
import {
  MOROS_BACCARAT_CARD_DOMAIN,
  MOROS_BACCARAT_SHOE_DOMAIN,
  MOROS_BACCARAT_TRANSCRIPT_DOMAIN,
  MOROS_SERVER_SEED_DOMAIN,
  computePoseidonOnElements,
  feltToModulo,
} from '../lib/poseidon'
import { formatStrk, formatWagerInput, parseStrkInputToWei } from '../lib/format'
import { resolveEffectiveMorosBalanceWei } from '../lib/game-balance'
import { randomClientSeed, sameFelt } from '../lib/random'
import { PlayingCard, type PlayingCardSuit } from '../components/PlayingCard'
import { useAccountStore } from '../store/account'
import { useToastStore } from '../store/toast'
import { useWalletStore } from '../store/wallet'

const baccaratTableId = morosGameBySlug('baccarat')?.tableId ?? 4
const chipOptions = [
  { label: '1', value: '1' },
  { label: '10', value: '10' },
  { label: '100', value: '100' },
  { label: '1K', value: '1000' },
]
const TABLE_STATE_FRESH_MS = 5_000

const betZones = [
  { id: 0, label: 'Player', tone: 'player', priority: 'primary' },
  { id: 2, label: 'Tie', tone: 'tie', priority: 'secondary' },
  { id: 1, label: 'Banker', tone: 'banker', priority: 'primary' },
] as const

const displaySuits: PlayingCardSuit[] = ['spades', 'hearts', 'diamonds', 'clubs']

type DrawnBaccaratCard = {
  card: number
  position: number
  drawIndex: number
  attempt: number
  commitment: string
}

function parseStrkInput(value: string) {
  return parseStrkInputToWei(value, { allowZero: false, label: 'STRK wager' })
}

function labelForSide(side: number) {
  if (side === 0) {
    return 'Player'
  }
  if (side === 1) {
    return 'Banker'
  }
  return 'Tie'
}

function baccaratProfitOnWin(wager: bigint, side: number) {
  if (wager <= 0n) {
    return 0n
  }
  if (side === 1) {
    return (wager * 95n) / 100n
  }
  if (side === 2) {
    return wager * 8n
  }
  return wager
}

async function baccaratCardCommitment(
  roundId: number,
  handIndex: number,
  cardIndex: number,
  draw: Omit<DrawnBaccaratCard, 'commitment'>,
) {
  return computePoseidonOnElements([
    MOROS_BACCARAT_CARD_DOMAIN,
    roundId.toString(),
    handIndex.toString(),
    cardIndex.toString(),
    draw.drawIndex.toString(),
    draw.attempt.toString(),
    draw.position.toString(),
    draw.card.toString(),
  ])
}

async function drawCards(serverSeed: string, clientSeed: string, player: string, roundId: number) {
  const usedPositions: number[] = []
  const draw = async (drawIndex: number) => {
    for (let attempt = 0; attempt < BACCARAT_MAX_SHOE_DRAW_ATTEMPTS; attempt += 1) {
      const mixed = await computePoseidonOnElements([
        MOROS_BACCARAT_SHOE_DOMAIN,
        serverSeed,
        clientSeed,
        player,
        roundId.toString(),
        drawIndex.toString(),
        attempt.toString(),
      ])
      const position = feltToModulo(mixed, 416n)
      if (!usedPositions.includes(position)) {
        usedPositions.push(position)
        return {
          card: (position % 13) + 1,
          position,
          drawIndex,
          attempt,
        }
      }
    }
    throw new Error('Unable to reproduce baccarat shoe draw')
  }

  const p0Raw = await draw(0)
  const b0Raw = await draw(1)
  const p1Raw = await draw(2)
  const b1Raw = await draw(3)
  const playerDetails: DrawnBaccaratCard[] = [
    { ...p0Raw, commitment: await baccaratCardCommitment(roundId, 0, 0, p0Raw) },
    { ...p1Raw, commitment: await baccaratCardCommitment(roundId, 0, 1, p1Raw) },
  ]
  const bankerDetails: DrawnBaccaratCard[] = [
    { ...b0Raw, commitment: await baccaratCardCommitment(roundId, 1, 0, b0Raw) },
    { ...b1Raw, commitment: await baccaratCardCommitment(roundId, 1, 1, b1Raw) },
  ]

  let playerTotal = baccaratTotal(playerDetails.map((card) => card.card))
  let bankerTotal = baccaratTotal(bankerDetails.map((card) => card.card))
  const natural = playerTotal >= 8 || bankerTotal >= 8
  let playerThird = 0
  if (!natural) {
    if (playerTotal <= 5) {
      const p2Raw = await draw(4)
      playerDetails.push({ ...p2Raw, commitment: await baccaratCardCommitment(roundId, 0, 2, p2Raw) })
      playerThird = p2Raw.card
      playerTotal = baccaratTotal(playerDetails.map((card) => card.card))
    }
    if (bankerDraws(bankerTotal, playerDetails.length === 3, cardValue(playerThird))) {
      const bankerDrawIndex = playerDetails.length === 3 ? 5 : 4
      const b2Raw = await draw(bankerDrawIndex)
      bankerDetails.push({ ...b2Raw, commitment: await baccaratCardCommitment(roundId, 1, 2, b2Raw) })
      bankerTotal = baccaratTotal(bankerDetails.map((card) => card.card))
    }
  }

  const playerCards = playerDetails.map((card) => card.card)
  const bankerCards = bankerDetails.map((card) => card.card)
  const transcriptRoot = await computePoseidonOnElements([
    MOROS_BACCARAT_TRANSCRIPT_DOMAIN,
    await computePoseidonOnElements([MOROS_SERVER_SEED_DOMAIN, serverSeed]),
    clientSeed,
    player,
    roundId.toString(),
    playerDetails[0]?.commitment ?? '0',
    bankerDetails[0]?.commitment ?? '0',
    playerDetails[1]?.commitment ?? '0',
    bankerDetails[1]?.commitment ?? '0',
    playerDetails[2]?.commitment ?? '0',
    bankerDetails[2]?.commitment ?? '0',
  ])

  return { playerCards, bankerCards, playerDetails, bankerDetails, transcriptRoot }
}

function cardValue(card: number) {
  if (card === 0 || card >= 10) {
    return 0
  }
  return card
}

function baccaratTotal(cards: number[]) {
  return cards.reduce((sum, card) => sum + cardValue(card), 0) % 10
}

function bankerDraws(total: number, playerDrew: boolean, playerThirdValue: number) {
  if (!playerDrew) return total <= 5
  if (total <= 2) return true
  if (total === 3) return playerThirdValue !== 8
  if (total === 4) return playerThirdValue >= 2 && playerThirdValue <= 7
  if (total === 5) return playerThirdValue >= 4 && playerThirdValue <= 7
  if (total === 6) return playerThirdValue === 6 || playerThirdValue === 7
  return false
}

function cardLabel(card: number) {
  if (card === 1) return 'A'
  if (card === 11) return 'J'
  if (card === 12) return 'Q'
  if (card === 13) return 'K'
  return String(card)
}

function displaySuit(rank: string, lane: string, index: number): PlayingCardSuit {
  const seed = `${lane}-${rank}-${index}`
  let total = 0
  for (const char of seed) {
    total += char.charCodeAt(0)
  }
  return displaySuits[total % displaySuits.length]
}

async function verifyBaccaratProof(round?: BaccaratRoundView, serverSeed?: string) {
  if (!round || !serverSeed) {
    return undefined
  }
  const serverSeedHash = await computePoseidonOnElements([MOROS_SERVER_SEED_DOMAIN, serverSeed])
  const cards = await drawCards(serverSeed, round.client_seed, round.player, round.round_id)
  const playerTranscriptMatches =
    sameNumberArray(round.player_card_positions, cards.playerDetails.map((card) => card.position))
    && sameNumberArray(round.player_card_draw_indices, cards.playerDetails.map((card) => card.drawIndex))
    && sameNumberArray(round.player_card_attempts, cards.playerDetails.map((card) => card.attempt))
    && sameFeltArray(round.player_card_commitments, cards.playerDetails.map((card) => card.commitment))
  const bankerTranscriptMatches =
    sameNumberArray(round.banker_card_positions, cards.bankerDetails.map((card) => card.position))
    && sameNumberArray(round.banker_card_draw_indices, cards.bankerDetails.map((card) => card.drawIndex))
    && sameNumberArray(round.banker_card_attempts, cards.bankerDetails.map((card) => card.attempt))
    && sameFeltArray(round.banker_card_commitments, cards.bankerDetails.map((card) => card.commitment))
  return {
    seedHashMatches: sameFelt(serverSeedHash, round.server_seed_hash),
    playerCardsMatch: round.player_cards.every((card, index) => card === cards.playerCards[index]),
    bankerCardsMatch: round.banker_cards.every((card, index) => card === cards.bankerCards[index]),
    playerTranscriptMatches,
    bankerTranscriptMatches,
    transcriptRootMatches: sameFelt(cards.transcriptRoot, round.transcript_root),
  }
}

function sameNumberArray(left: number[], right: number[]) {
  return left.length === right.length && left.every((value, index) => value === right[index])
}

function sameFeltArray(left: string[], right: string[]) {
  return left.length === right.length && left.every((value, index) => sameFelt(value, right[index]))
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

function playPickupSoundBurst(count: number) {
  if (typeof window === 'undefined' || count <= 0) {
    return
  }

  for (let index = 0; index < count; index += 1) {
    window.setTimeout(() => {
      const audio = new Audio('/pickup.wav')
      audio.volume = 0.72
      void audio.play().catch(() => {})
    }, index * 70)
  }
}

export function BaccaratPage() {
  const {
    address,
    connect,
    ensureGameplaySession,
    error: walletError,
    openBaccaratRound,
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
  const [selectedChipIndex, setSelectedChipIndex] = useState(1)
  const [selectedSide, setSelectedSide] = useState<number>(0)
  const [zoneBets, setZoneBets] = useState<Record<number, bigint>>(() => ({ 0: 0n, 1: 0n, 2: 0n }))
  const [round, setRound] = useState<BaccaratRoundView>()
  const [lastDeal, setLastDeal] = useState<SettleBaccaratCommitmentResponse>()
  const [statusMessage, setStatusMessage] = useState<string>()
  const [isDealing, setIsDealing] = useState(false)
  const [theatreMode, setTheatreMode] = useState(false)
  const [clientSeedDraft, setClientSeedDraft] = useState<string>()
  const [activeCommitment, setActiveCommitment] = useState<number>()
  const [proof, setProof] = useState<Awaited<ReturnType<typeof verifyBaccaratProof>>>()
  const [transientFairnessPhase, setTransientFairnessPhase] = useState<OriginalsFairnessStage>()
  const dealInFlightRef = useRef(false)
  const previousCardCountRef = useRef(0)
  const pushToast = useToastStore((state) => state.pushToast)
  const { lastLoadedAt, refreshTableState, tableState } = useTableState(baccaratTableId, resolvedWalletAddress)
  const { commitmentReady, takeCommitment } = useOriginalsCommitment(createBaccaratCommitment, {
    enabled: accountState.signedIn,
  })

  const selectedChip = chipOptions[selectedChipIndex]
  const selectedChipWei = useMemo(() => parseStrkInput(selectedChip.value), [selectedChip.value])
  const totalBetWei = useMemo(() => Object.values(zoneBets).reduce((sum, value) => sum + value, 0n), [zoneBets])
  const totalBetInput = totalBetWei === 0n ? '0' : formatWagerInput(totalBetWei)
  const activeBets = useMemo(
    () => betZones.filter((zone) => (zoneBets[zone.id] ?? 0n) > 0n),
    [zoneBets],
  )
  const expectedProfit = useMemo(() => {
    const activeZone = activeBets[0]
    if (!activeZone) {
      return '0 STRK'
    }
    return formatStrk(baccaratProfitOnWin(zoneBets[activeZone.id] ?? 0n, activeZone.id).toString())
  }, [activeBets, zoneBets])

  useEffect(() => {
    let cancelled = false

    if (!round || !lastDeal?.server_seed) {
      setProof(undefined)
      return () => {
        cancelled = true
      }
    }

    void verifyBaccaratProof(round, lastDeal.server_seed)
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
  }, [lastDeal?.server_seed, round])

  useEffect(() => {
    const currentCardCount =
      (round?.player_cards.length ?? 0) +
      (round?.banker_cards.length ?? 0)
    const previousCardCount = previousCardCountRef.current

    if (currentCardCount > previousCardCount) {
      playPickupSoundBurst(currentCardCount - previousCardCount)
    }

    previousCardCountRef.current = currentCardCount
  }, [round?.banker_cards.length, round?.player_cards.length])

  function cycleChip(direction: 'prev' | 'next') {
    setSelectedChipIndex((current) => {
      if (direction === 'prev') {
        return current === 0 ? chipOptions.length - 1 : current - 1
      }
      return current === chipOptions.length - 1 ? 0 : current + 1
    })
  }

  function clearBets() {
    setZoneBets({ 0: 0n, 1: 0n, 2: 0n })
    setStatusMessage(undefined)
  }

  function addChipToZone(sideId: number) {
    setSelectedSide(sideId)
    setStatusMessage(undefined)
    setZoneBets((current) => ({
      0: sideId === 0 ? (current[0] ?? 0n) + selectedChipWei : 0n,
      1: sideId === 1 ? (current[1] ?? 0n) + selectedChipWei : 0n,
      2: sideId === 2 ? (current[2] ?? 0n) + selectedChipWei : 0n,
    }))
  }

  function scaleBets(direction: 'half' | 'double') {
    if (totalBetWei === 0n) {
      return
    }
    setZoneBets((current) => ({
      0: direction === 'half' ? current[0] / 2n : current[0] * 2n,
      1: direction === 'half' ? current[1] / 2n : current[1] * 2n,
      2: direction === 'half' ? current[2] / 2n : current[2] * 2n,
    }))
  }

  function updateTotalBetInput(nextValue: string) {
    setStatusMessage(undefined)
    if (!nextValue.trim()) {
      setZoneBets((current) => ({ ...current, [selectedSide]: 0n }))
      return
    }

    try {
      const parsed = parseStrkInput(nextValue)
      setZoneBets((current) => ({ ...current, [selectedSide]: parsed }))
    } catch (error) {
      const message = error instanceof Error ? error.message : 'Enter a valid STRK wager.'
      setStatusMessage(message)
      pushToast({ message, tone: 'warn', title: 'Baccarat wager' })
    }
  }

  async function handleDeal() {
    if (dealInFlightRef.current) {
      return
    }

    dealInFlightRef.current = true
    setIsDealing(true)
    setRound(undefined)
    setLastDeal(undefined)
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
        throw new Error('Connect a wallet before dealing.')
      }

      if (activeBets.length === 0) {
        setTransientFairnessPhase(undefined)
        setStatusMessage('Place at least one chip on Player, Tie, or Banker.')
        return
      }

      playerAddress =
        useAccountStore.getState().walletAddress ??
        useWalletStore.getState().address ??
        playerAddress
      const activeZone = activeBets[0]
      const wagerWei = (zoneBets[activeZone.id] ?? 0n).toString()
      const canReuseTableState =
        Boolean(tableState)
        && typeof lastLoadedAt === 'number'
        && resolvedWalletAddress?.toLowerCase() === playerAddress.toLowerCase()
        && Date.now() - lastLoadedAt <= TABLE_STATE_FRESH_MS
      const liveTable = canReuseTableState
        ? { live_players: undefined, state: tableState! }
        : await refreshTableState(playerAddress)
      const clientSeed = clientSeedDraft ?? randomClientSeed()
      const bankrollBalanceWei = await resolveEffectiveMorosBalanceWei(
        playerAddress,
        liveTable.state.player_balance,
      )
      const bankrollShortfall = BigInt(wagerWei) > BigInt(bankrollBalanceWei) ? BigInt(wagerWei) - BigInt(bankrollBalanceWei) : 0n
      if (bankrollShortfall > 0n) {
        throw new Error('Deposit STRK into your Moros balance before betting.')
      }
      setStatusMessage('Authorizing gameplay...')
      await ensureGameplaySession()
      setTransientFairnessPhase('commit')
      setStatusMessage('Preparing baccarat commitment...')
      const commitment = await takeCommitment()
      setActiveCommitment(commitment.commitment.commitment_id)
      setTransientFairnessPhase('open')
      setClientSeedDraft(undefined)
      setStatusMessage(
        commitmentReady
          ? 'Opening the deal...'
          : 'Baccarat seed hash committed. Opening the deal...',
      )
      await openBaccaratRound({
        tableId: baccaratTableId,
        wagerWei,
        bankrollBalanceWei,
        betSide: activeZone.id,
        clientSeed,
        commitmentId: commitment.commitment.commitment_id,
      })
      setTransientFairnessPhase('reveal')
      setStatusMessage('Deal opened. Revealing server seed and settling on Starknet...')
      const result = await settleBaccaratCommitment(commitment.commitment.commitment_id)
      setTransientFairnessPhase('verify')
      setRound(result.round)
      setLastDeal(result)
      void refreshTableState(playerAddress)
      setSelectedSide(activeZone.id)
      setStatusMessage(`Baccarat round ${result.round.round_id} settled: ${labelForSide(result.round.winner)}.`)
    } catch (error) {
      const message = error instanceof Error ? error.message : 'Failed to deal baccarat.'
      setTransientFairnessPhase(undefined)
      setStatusMessage(message)
    } finally {
      dealInFlightRef.current = false
      setIsDealing(false)
    }
  }

  const fairnessFields = [
    { label: 'Commitment id', value: round?.commitment_id ? `#${round.commitment_id}` : 'Pending' },
    { label: 'Server seed hash', value: round?.server_seed_hash ?? 'Pending', sensitive: true },
    { label: 'Server seed', value: lastDeal?.server_seed ?? 'Revealed after settlement', sensitive: true },
    { label: 'Client seed', value: clientSeedDraft ?? round?.client_seed ?? 'Generated on deal', sensitive: true },
    { label: 'Nonce', value: round?.round_id ? `#${round.round_id}` : 'Pending' },
  ]

  const liveStats = [
    { label: 'Winner', value: round ? labelForSide(round.winner) : '—' },
    { label: 'Player total', value: round ? String(round.player_total) : '—' },
    { label: 'Banker total', value: round ? String(round.banker_total) : '—' },
    { label: 'Payout', value: formatStrk(round?.payout) },
  ]

  const settingsStats = [
    { label: 'Current side', value: labelForSide(selectedSide) },
    { label: 'Table max', value: formatStrk(tableState?.table.max_wager) },
    { label: 'House available', value: formatStrk(tableState?.house_available) },
  ]
  const walletBusy =
    walletStatus === 'connecting' || walletStatus === 'preparing' || walletStatus === 'funding' || walletStatus === 'confirming'
  const inlineWalletError = walletError && walletError !== statusMessage ? walletError : undefined
  const primaryActionLabel = isDealing
    ? 'Dealing...'
    : resolveMorosPrimaryActionLabel({
      accountState,
      pendingLabel,
      readyLabel: 'Bet',
      walletBusy,
    })
  const roundOutcome = round ? playerOutcomeTone(round.payout, round.wager) : undefined
  const baccaratProofPassed = proof
    ? proof.seedHashMatches
      && proof.playerCardsMatch
      && proof.bankerCardsMatch
      && proof.playerTranscriptMatches
      && proof.bankerTranscriptMatches
      && proof.transcriptRootMatches
    : false

  const pageClassName = `page page--baccarat${theatreMode ? ' page--theatre' : ''}`

  return (
    <section className={pageClassName}>
      <div className="baccarat-desk">
        <aside className="baccarat-sidebar">
          <section className="baccarat-sidebar__section">
            <div className="baccarat-sidebar__label-row">
              <span>Chip Value</span>
            </div>
            <div className="baccarat-chip-strip">
              <button aria-label="Previous chip" className="baccarat-chip-strip__arrow" onClick={() => cycleChip('prev')} type="button">
                ‹
              </button>
              <div className="baccarat-chip-strip__list">
                {chipOptions.map((chip, index) => (
                  <button
                    className={index === selectedChipIndex ? 'baccarat-chip baccarat-chip--active' : 'baccarat-chip'}
                    key={chip.label}
                    onClick={() => setSelectedChipIndex(index)}
                    type="button"
                  >
                    {chip.label}
                  </button>
                ))}
              </div>
              <button aria-label="Next chip" className="baccarat-chip-strip__arrow" onClick={() => cycleChip('next')} type="button">
                ›
              </button>
            </div>
          </section>

          <section className="baccarat-sidebar__section">
            <div className="baccarat-sidebar__label-row">
              <span>Total Bet</span>
              <strong>{formatStrk(totalBetWei)}</strong>
            </div>
            <div className="baccarat-total-row">
              <label className="dice-token-input">
                <input
                  className="text-input text-input--large dice-token-input__field"
                  inputMode="decimal"
                  onChange={(event) => updateTotalBetInput(event.target.value)}
                  value={totalBetInput}
                />
                <span className="dice-token-input__token dice-token-input__token--label">STRK</span>
              </label>
              <button className="chip" onClick={() => scaleBets('half')} type="button">½</button>
              <button className="chip" onClick={() => scaleBets('double')} type="button">2×</button>
            </div>
          </section>

          <section className="baccarat-sidebar__section">
            <div className="baccarat-sidebar__label-row">
              <span>Profit on Win</span>
              <strong>{expectedProfit}</strong>
            </div>
            <label className="dice-token-input">
              <input className="text-input text-input--large dice-token-input__field" readOnly value={expectedProfit.replace(' STRK', '')} />
            </label>
          </section>

          <div className="baccarat-sidebar__footer">
            <div className="game-sidebar__action-row">
              <button
                className="button button--wide game-primary-action"
                disabled={isDealing || walletBusy}
                onClick={() => void handleDeal()}
                type="button"
              >
                {primaryActionLabel}
              </button>
              <button className="button button--ghost button--wide" disabled={totalBetWei === 0n} onClick={clearBets} type="button">
                Clear Bets
              </button>
            </div>
            {statusMessage ? <p className="stack-note">{statusMessage}</p> : null}
            {inlineWalletError ? <p className="stack-note stack-note--error">{inlineWalletError}</p> : null}
          </div>
        </aside>

        <article className="baccarat-main">
          <div className="baccarat-table-surface">
            <div className="baccarat-info-banner">TIE PAYS 8 TO 1</div>

            <div className="baccarat-cards-stage">
              <div className="baccarat-hand">
                <div className="baccarat-hand__header">
                  <span>Player</span>
                  <strong>{round ? round.player_total : '—'}</strong>
                </div>
                <div className="baccarat-hand__cards">
                  {round?.player_cards.length
                    ? round.player_cards.map((card, index) => (
                        <div className="baccarat-card-shell" key={`player-${index}`} style={{ animationDelay: `${index * 70}ms` }}>
                          <PlayingCard
                            ariaLabel={`${cardLabel(card)} player card`}
                            className="stage-playing-card baccarat-stage-card-visual"
                            rank={cardLabel(card)}
                            suit={displaySuit(cardLabel(card), 'player', index)}
                            tilt={0}
                          />
                        </div>
                      ))
                    : [0, 1].map((index) => (
                        <div className="baccarat-card-shell" key={`player-placeholder-${index}`}>
                          <PlayingCard
                            ariaLabel="Empty player baccarat card slot"
                            className="stage-playing-card baccarat-stage-card-visual"
                            placeholder
                          />
                        </div>
                      ))}
                </div>
              </div>

              <div className="baccarat-hand">
                <div className="baccarat-hand__header">
                  <span>Banker</span>
                  <strong>{round ? round.banker_total : '—'}</strong>
                </div>
                <div className="baccarat-hand__cards">
                  {round?.banker_cards.length
                    ? round.banker_cards.map((card, index) => (
                        <div className="baccarat-card-shell" key={`banker-${index}`} style={{ animationDelay: `${index * 70}ms` }}>
                          <PlayingCard
                            ariaLabel={`${cardLabel(card)} banker card`}
                            className="stage-playing-card baccarat-stage-card-visual"
                            rank={cardLabel(card)}
                            suit={displaySuit(cardLabel(card), 'banker', index)}
                            tilt={0}
                          />
                        </div>
                      ))
                    : [0, 1].map((index) => (
                        <div className="baccarat-card-shell" key={`banker-placeholder-${index}`}>
                          <PlayingCard
                            ariaLabel="Empty banker baccarat card slot"
                            className="stage-playing-card baccarat-stage-card-visual"
                            placeholder
                          />
                        </div>
                      ))}
                </div>
              </div>
            </div>

            <div className="baccarat-bets-row">
              {betZones.map((zone) => {
                const amount = zoneBets[zone.id] ?? 0n
                const isWinning = round?.winner === zone.id
                const isPlayerBet = round?.bet_side === zone.id
                return (
                  <button
                    className={cx(
                      'baccarat-bet-zone',
                      zone.priority === 'secondary' && 'baccarat-bet-zone--secondary',
                      zone.tone === 'player' && 'baccarat-bet-zone--player',
                      zone.tone === 'tie' && 'baccarat-bet-zone--tie',
                      zone.tone === 'banker' && 'baccarat-bet-zone--banker',
                      amount > 0n && 'baccarat-bet-zone--placed',
                      isWinning && 'baccarat-bet-zone--winning',
                      isPlayerBet && roundOutcome === 'win' && 'baccarat-bet-zone--player-win',
                      isPlayerBet && roundOutcome === 'loss' && 'baccarat-bet-zone--player-loss',
                      isPlayerBet && roundOutcome === 'push' && 'baccarat-bet-zone--player-push',
                      selectedSide === zone.id && 'baccarat-bet-zone--selected',
                    )}
                    key={zone.id}
                    onClick={() => addChipToZone(zone.id)}
                    type="button"
                  >
                    <span className="baccarat-bet-zone__title">{zone.label}</span>
                    <strong className="baccarat-bet-zone__amount">
                      <i />
                      {formatStrk(amount)}
                    </strong>
                  </button>
                )
              })}
            </div>
          </div>

          <OriginalsFairnessStepper
            committed={Boolean(activeCommitment || round?.server_seed_hash)}
            label="Baccarat commit-reveal verification progress"
            opened={Boolean(round)}
            phase={transientFairnessPhase}
            verified={Boolean(proof && baccaratProofPassed)}
            warning={Boolean(proof && !baccaratProofPassed)}
          />

          <GameUtilityBar
            fairnessFields={fairnessFields}
            fairnessStatus={{
              label: proof ? (baccaratProofPassed ? 'Verification passed' : 'Verification mismatch') : 'Awaiting next settled deal',
              tone: proof ? (baccaratProofPassed ? 'good' : 'warn') : 'neutral',
            }}
            fairnessSummary={
              proof
                ? baccaratProofPassed
                  ? 'The revealed seed recreates the settled cards, card positions, commitments, and transcript root.'
                  : 'The revealed seed does not recreate the settled transcript. Recheck the hash, client seed, round id, and card commitments.'
                : 'Each baccarat round commits a server hash before the relayer opens the deal. Reveal data appears after settlement.'
            }
            liveStats={liveStats}
            onRegenerate={() => {
              setClientSeedDraft(randomClientSeed())
              setStatusMessage('Queued a fresh client seed for the next baccarat deal.')
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
