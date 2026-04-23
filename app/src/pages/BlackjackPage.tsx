import { useEffect, useMemo, useRef, useState } from 'react'
import {
  createCoordinatorHand,
  fetchCoordinatorHandFairness,
  fetchCoordinatorHandView,
  fetchCoordinatorTableState,
  relayHandAction,
  type BlackjackFairnessArtifactView,
  type BlackjackHandView,
  type BlackjackTableState,
} from '../lib/api'
import { GameUtilityBar } from '../components/GameUtilityBar'
import { useMorosAuthRuntime } from '../components/MorosAuthProvider'
import { deriveMorosAccountState, resolveMorosPrimaryActionLabel } from '../lib/account-state'
import { morosConfig } from '../lib/config'
import { morosGameBySlug } from '../lib/game-config'
import {
  clearStoredGameplaySession,
  gameplaySessionMatchesAddress,
  gameplaySessionMatchesKey,
  readStoredGameplaySession,
} from '../lib/gameplay-session'
import { useMorosWallet } from '../hooks/useMorosWallet'
import { PlayingCard, type PlayingCardSuit } from '../components/PlayingCard'
import { useAccountStore } from '../store/account'
import { useWalletStore } from '../store/wallet'

const tableIds: Record<string, number> = {
  'blackjack-main-floor': morosGameBySlug('blackjack')?.tableId ?? 2,
}
const TABLE_STATE_FRESH_MS = 5_000

function rankLabel(card?: number | null) {
  if (!card) {
    return '—'
  }

  if (card === 1) {
    return 'A'
  }

  if (card === 11) {
    return 'J'
  }

  if (card === 12) {
    return 'Q'
  }

  if (card === 13) {
    return 'K'
  }

  return String(card)
}

const displaySuits: PlayingCardSuit[] = ['spades', 'hearts', 'diamonds', 'clubs']

function displaySuit(rank: string, lane: string, index: number): PlayingCardSuit {
  const seed = `${lane}-${rank}-${index}`
  let total = 0
  for (const char of seed) {
    total += char.charCodeAt(0)
  }
  return displaySuits[total % displaySuits.length]
}

function cardTilt(index: number, count: number) {
  if (count <= 1) {
    return 0
  }
  const center = (count - 1) / 2
  return (index - center) * 1.6
}

function formatStrk(wei?: string | null) {
  if (!wei) {
    return '0 STRK'
  }

  const value = BigInt(wei)
  const whole = value / 10n ** 18n
  const fraction = (value % 10n ** 18n).toString().padStart(18, '0').slice(0, 2)
  return fraction === '00' ? `${whole} STRK` : `${whole}.${fraction} STRK`
}

function parseStrkInput(value: string) {
  const normalized = value.trim()
  if (!/^\d+(\.\d{0,4})?$/.test(normalized)) {
    throw new Error('Enter a STRK wager with up to 4 decimal places.')
  }
  const [whole, fraction = ''] = normalized.split('.')
  return BigInt(whole) * 10n ** 18n + BigInt(fraction.padEnd(18, '0'))
}

function formatWagerInput(wei: bigint) {
  const whole = wei / 10n ** 18n
  const fraction = (wei % 10n ** 18n).toString().padStart(18, '0').slice(0, 4).replace(/0+$/, '')
  return fraction ? `${whole}.${fraction}` : whole.toString()
}

function seatOutcomeTone(outcome?: string | null) {
  if (outcome === 'win' || outcome === 'blackjack') {
    return 'blackjack-player-seat blackjack-player-seat--win'
  }

  if (outcome === 'push') {
    return 'blackjack-player-seat blackjack-player-seat--push'
  }

  if (outcome) {
    return 'blackjack-player-seat blackjack-player-seat--loss'
  }

  return 'blackjack-player-seat'
}

function badgeOutcomeTone(outcome?: string | null) {
  if (outcome === 'win' || outcome === 'blackjack') {
    return 'blackjack-total-badge blackjack-total-badge--win'
  }

  if (outcome === 'loss') {
    return 'blackjack-total-badge blackjack-total-badge--loss'
  }

  if (outcome === 'push') {
    return 'blackjack-total-badge blackjack-total-badge--push'
  }

  return 'blackjack-total-badge'
}

function formatBlackjackTotal(total?: number | null, soft?: boolean | null) {
  if (total === undefined || total === null) {
    return '—'
  }

  if (!soft || total < 12) {
    return String(total)
  }

  return `${total - 10}/${total}`
}

function readCachedGameplaySessionToken(playerAddress?: string) {
  const session = readStoredGameplaySession()
  const nowUnix = Math.floor(Date.now() / 1000)
  if (
    !session
    || session.expiresAtUnix <= nowUnix + 5
    || !gameplaySessionMatchesAddress(session, playerAddress)
    || !gameplaySessionMatchesKey(session, morosConfig.gameplaySessionKey)
  ) {
    return undefined
  }

  return session.sessionToken
}

function actionLabel(action: string) {
  switch (action) {
    case 'hit':
      return 'Hit'
    case 'stand':
      return 'Stand'
    case 'split':
      return 'Split'
    case 'double':
      return 'Double'
    case 'take_insurance':
      return 'Insurance'
    case 'decline_insurance':
      return 'No Insurance'
    default:
      return action
  }
}

function randomClientSeed() {
  const bytes = new Uint32Array(4)
  crypto.getRandomValues(bytes)
  let value = 0n
  for (const byte of bytes) {
    value = (value << 32n) + BigInt(byte)
  }
  return value.toString()
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

export function BlackjackPage() {
  const auth = useMorosAuthRuntime()
  const {
    address,
    connect,
    ensureGameplaySession,
    error: walletError,
    fund,
    pendingLabel,
    status: walletStatus,
  } = useMorosWallet()
  const accountUserId = useAccountStore((state) => state.userId)
  const accountWalletAddress = useAccountStore((state) => state.walletAddress)
  const accountState = deriveMorosAccountState({
    authReady: auth.ready,
    authenticated: auth.authenticated,
    authLoading: auth.loading,
    oauthLoading: auth.oauthLoading,
    emailState: auth.emailState,
    accountUserId,
    accountWalletAddress,
    runtimeWalletAddress: address,
    walletStatus,
  })
  const { resolvedWalletAddress } = accountState
  const selectedTable = 'blackjack-main-floor'
  const [wager, setWager] = useState('0')
  const [hand, setHand] = useState<BlackjackHandView>()
  const [fairnessArtifact, setFairnessArtifact] = useState<BlackjackFairnessArtifactView>()
  const [session, setSession] = useState<{ session_id: string; relay_token: string }>()
  const [tableState, setTableState] = useState<BlackjackTableState>()
  const [tableStateLoadedAt, setTableStateLoadedAt] = useState<number>()
  const [statusMessage, setStatusMessage] = useState<string>()
  const [actionPending, setActionPending] = useState<string>()
  const [createPending, setCreatePending] = useState(false)
  const [theatreMode, setTheatreMode] = useState(false)
  const previousCardCountRef = useRef(0)
  const createInFlightRef = useRef(false)
  const createRequestIdRef = useRef(0)
  const activeRuntimeRef = useRef<{ handId: string; sessionId: string; relayToken: string } | undefined>(undefined)
  const allowedActions = hand?.allowed_actions ?? []
  const playerSeats = [...(hand?.seats ?? [])].sort((left, right) => left.seat_index - right.seat_index)
  const dealerTotalLabel = hand?.dealer.total !== undefined && hand?.dealer.total !== null
    ? formatBlackjackTotal(hand.dealer.total, hand.dealer.soft)
    : '—'
  const betLocked = Boolean(hand && hand.phase !== 'settled')
  const walletBusy =
    walletStatus === 'connecting' || walletStatus === 'preparing' || walletStatus === 'funding' || walletStatus === 'confirming'
  const primaryActionLabel = createPending
    ? 'Opening hand...'
    : betLocked
      ? 'Bet'
    : resolveMorosPrimaryActionLabel({
      accountState,
      pendingLabel,
      readyLabel: 'Bet',
      walletBusy,
    })
  const actionButtons =
    hand?.phase === 'insurance'
      ? [
          { id: 'take_insurance', label: 'Insurance' },
          { id: 'decline_insurance', label: 'No Insurance' },
        ]
      : [
          { id: 'hit', label: 'Hit' },
          { id: 'stand', label: 'Stand' },
          { id: 'split', label: 'Split' },
          { id: 'double', label: 'Double' },
        ]
  const expectedProfit = useMemo(() => {
    try {
      return formatStrk(parseStrkInput(wager).toString())
    } catch {
      return '0 STRK'
    }
  }, [wager])

  function adjustWager(next: 'half' | 'double') {
    try {
      const currentWei = parseStrkInput(wager)
      const adjusted = next === 'half'
        ? currentWei > 0n ? currentWei / 2n : currentWei
        : currentWei * 2n
      setWager(formatWagerInput(adjusted))
    } catch {
      setWager('0')
    }
  }

  useEffect(() => {
    const tableId = tableIds[selectedTable] ?? 2
    void fetchCoordinatorTableState(tableId, resolvedWalletAddress)
      .then((response) => setTableState(response.state))
      .catch(() => setTableState(undefined))
  }, [resolvedWalletAddress, selectedTable])

  useEffect(() => {
    if (!hand) {
      previousCardCountRef.current = 0
      setFairnessArtifact(undefined)
      return
    }

    let cancelled = false
    const sessionToken = readCachedGameplaySessionToken(resolvedWalletAddress ?? hand.player)
    if (!sessionToken) {
      return
    }

    void fetchCoordinatorHandFairness(hand.hand_id, sessionToken)
      .then((artifact) => {
        if (!cancelled) {
          setFairnessArtifact(artifact)
        }
      })
      .catch(() => {
        if (!cancelled) {
          setFairnessArtifact(undefined)
        }
      })

    return () => {
      cancelled = true
    }
  }, [hand?.hand_id, hand?.status, hand?.player, resolvedWalletAddress])

  useEffect(() => {
    const currentCardCount =
      (hand?.dealer.cards.length ?? 0) +
      playerSeats.reduce((total, seat) => total + seat.cards.length, 0)
    const previousCardCount = previousCardCountRef.current

    if (currentCardCount > previousCardCount) {
      playPickupSoundBurst(currentCardCount - previousCardCount)
    }

    previousCardCountRef.current = currentCardCount
  }, [hand?.dealer.cards.length, playerSeats])

  async function handleCreateHand() {
    if (createInFlightRef.current) {
      return
    }

    const requestId = createRequestIdRef.current + 1
    createRequestIdRef.current = requestId
    createInFlightRef.current = true
    setCreatePending(true)
    setStatusMessage(undefined)
    try {
      const wagerValue = parseStrkInput(wager)
      const tableId = tableIds[selectedTable] ?? 2
      let playerAddress = address as string | undefined
      if (!playerAddress) {
        const connected = await connect()
        playerAddress = connected.address
      }
      if (!playerAddress) {
        throw new Error('Connect a wallet before opening a hand.')
      }
      if (wagerValue < 0n) {
        throw new Error('Enter a valid STRK wager to open a live blackjack hand.')
      }
      if (wagerValue === 0n) {
        throw new Error('Enter a positive STRK wager to open a live blackjack hand.')
      }
      let gameplaySessionToken = await ensureGameplaySession()
      playerAddress =
        useAccountStore.getState().walletAddress ??
        useWalletStore.getState().address ??
        playerAddress
      const canReuseTableState =
        Boolean(tableState)
        && typeof tableStateLoadedAt === 'number'
        && resolvedWalletAddress?.toLowerCase() === playerAddress.toLowerCase()
        && Date.now() - tableStateLoadedAt <= TABLE_STATE_FRESH_MS
      const liveTable = canReuseTableState
        ? { live_players: undefined, state: tableState! }
        : await fetchCoordinatorTableState(tableId, playerAddress)
      setTableState(liveTable.state)
      setTableStateLoadedAt(Date.now())
      const liveMaxWager = BigInt(liveTable.state.table.max_wager)
      if (liveMaxWager > 0n && wagerValue > liveMaxWager) {
        throw new Error(`Moros currently allows wagers up to ${formatStrk(liveTable.state.table.max_wager)} on this table.`)
      }
      const bankrollBalanceWei = BigInt(liveTable.state.player_balance ?? '0')
      const bankrollShortfall = wagerValue > bankrollBalanceWei ? wagerValue - bankrollBalanceWei : 0n
      if (bankrollShortfall > 0n) {
        setStatusMessage('Funding bankroll and opening your hand...')
        await fund(formatWagerInput(bankrollShortfall))
        const refreshedTable = await fetchCoordinatorTableState(tableId, playerAddress)
        setTableState(refreshedTable.state)
        setTableStateLoadedAt(Date.now())
      }
      let created
      try {
        created = await createCoordinatorHand({
          table_id: tableId,
          player: playerAddress,
          wager: wagerValue.toString(),
          client_seed: randomClientSeed(),
        }, gameplaySessionToken)
      } catch (error) {
        const message = error instanceof Error ? error.message.toLowerCase() : ''
        if (!message.includes('gameplay session grant')) {
          throw error
        }
        clearStoredGameplaySession()
        gameplaySessionToken = await ensureGameplaySession()
        created = await createCoordinatorHand({
          table_id: tableId,
          player: playerAddress,
          wager: wagerValue.toString(),
          client_seed: randomClientSeed(),
        }, gameplaySessionToken)
      }
      const [handRecord, refreshedTable, fairnessRecord] = await Promise.all([
        fetchCoordinatorHandView(created.hand_id, gameplaySessionToken),
        fetchCoordinatorTableState(tableId, playerAddress),
        fetchCoordinatorHandFairness(created.hand_id, gameplaySessionToken).catch(() => undefined),
      ])
      if (requestId !== createRequestIdRef.current) {
        return
      }
      activeRuntimeRef.current = {
        handId: created.hand_id,
        sessionId: created.session_id,
        relayToken: created.relay_token,
      }
      setHand(handRecord)
      setSession({
        session_id: created.session_id,
        relay_token: created.relay_token,
      })
      setTableState(refreshedTable.state)
      setTableStateLoadedAt(Date.now())
      if (fairnessRecord) {
        setFairnessArtifact(fairnessRecord)
      }
      setStatusMessage(`Hand ${created.hand_id} is open on Starknet and mirrored into the Moros runtime.`)
    } catch (error) {
      setStatusMessage(error instanceof Error ? error.message : 'Failed to create hand.')
    } finally {
      if (requestId === createRequestIdRef.current) {
        createInFlightRef.current = false
        setCreatePending(false)
      }
    }
  }

  async function handleRelayAction(action: string) {
    const label = actionLabel(action)
    if (createInFlightRef.current) {
      setStatusMessage('Blackjack runtime is still opening. Wait for the hand to load.')
      return
    }

    if (!hand || !session) {
      setStatusMessage('Create a hand session before relaying player actions.')
      return
    }

    const activeRuntime = activeRuntimeRef.current
    if (!activeRuntime || activeRuntime.handId !== hand.hand_id) {
      setStatusMessage('Blackjack runtime is syncing. Retry the action.')
      return
    }

    setActionPending(action)
    setStatusMessage(undefined)
    try {
      const relay = await relayHandAction({
        hand_id: activeRuntime.handId,
        action,
        relay_token: activeRuntime.relayToken,
      })
      setStatusMessage(relay.status === 'completed' ? `${label} applied.` : `${label} submitted.`)
      const nextHand = relay.hand
      if (nextHand) {
        setHand(nextHand)
        if (nextHand.phase === 'settled') {
          activeRuntimeRef.current = undefined
          setSession(undefined)
        }
      }
      const sessionToken = readCachedGameplaySessionToken(
        resolvedWalletAddress ?? nextHand?.player ?? hand.player,
      )
      const [refreshedHand, nextTableState, nextFairness] = await Promise.all([
        sessionToken
          ? fetchCoordinatorHandView(activeRuntime.handId, sessionToken).catch(() => nextHand)
          : Promise.resolve(nextHand),
        fetchCoordinatorTableState(
          tableIds[selectedTable] ?? 2,
          resolvedWalletAddress ?? nextHand?.player ?? hand.player,
        ),
        sessionToken
          ? fetchCoordinatorHandFairness(activeRuntime.handId, sessionToken).catch(() => undefined)
          : Promise.resolve(undefined),
      ])
      if (refreshedHand) {
        setHand(refreshedHand)
        if (refreshedHand.phase === 'settled') {
          activeRuntimeRef.current = undefined
          setSession(undefined)
        }
      }
      setTableState(nextTableState.state)
      setTableStateLoadedAt(Date.now())
      if (nextFairness) {
        setFairnessArtifact(nextFairness)
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : 'Failed to relay action.'
      if (message.includes('relay token does not match active runtime')) {
        activeRuntimeRef.current = undefined
      }
      setStatusMessage(message)
    } finally {
      setActionPending(undefined)
    }
  }

  const fairnessFields = [
    { label: 'Transcript root', value: hand?.transcript_root ?? 'Open a hand to anchor a transcript root.', sensitive: true },
    { label: 'Server seed hash', value: hand?.server_seed_hash ?? 'Committed when the hand opens.', sensitive: true },
    { label: 'Server seed', value: hand?.server_seed ?? 'Revealed after settlement.', sensitive: true },
    { label: 'Verification', value: hand ? (hand.proof_verified ? 'Verified against committed transcript' : 'Pending reveal') : 'Open and settle a hand to verify it.' },
    { label: 'Audit mode', value: fairnessArtifact?.audit.mode ?? 'Waiting for fairness artifact.' },
    {
      label: 'Peek proof',
      value: fairnessArtifact?.dealer_peek.statement_kind
        ? `${fairnessArtifact.dealer_peek.statement_kind} (${fairnessArtifact.dealer_peek.proof_mode})`
        : 'Waiting for dealer peek state.',
    },
    {
      label: 'Peek verifier',
      value: fairnessArtifact?.dealer_peek.no_blackjack_proof.available
        ? fairnessArtifact.dealer_peek.no_blackjack_proof.proof_binding.available
          ? `${fairnessArtifact.dealer_peek.no_blackjack_proof.verifier_status} · ${fairnessArtifact.dealer_peek.no_blackjack_proof.proof_binding.verification_key_id}`
          : 'Proof binding pending'
        : 'Not required for this hand.',
    },
    {
      label: 'ZK target',
      value: fairnessArtifact?.dealer_peek.no_blackjack_proof.zk_proof_target.available
        ? `${fairnessArtifact.dealer_peek.no_blackjack_proof.zk_proof_target.circuit_id} · ${fairnessArtifact.dealer_peek.no_blackjack_proof.zk_proof_target.request_id}`
        : 'Not required for this hand.',
    },
    {
      label: 'Proof binding',
      value: fairnessArtifact?.dealer_peek.no_blackjack_proof.proof_binding.available
        ? `${fairnessArtifact.dealer_peek.no_blackjack_proof.proof_binding.status} · ${fairnessArtifact.dealer_peek.no_blackjack_proof.proof_binding.proof_id}`
        : fairnessArtifact?.dealer_peek.no_blackjack_proof.zk_proof_target.available
          ? 'Proof binding pending'
          : 'Not required for this hand.',
    },
    {
      label: 'Public audit',
      value: fairnessArtifact
        ? fairnessArtifact.audit.passed
          ? 'Encrypted-deck openings and peek bindings passed'
          : fairnessArtifact.audit.issues.join(', ')
        : 'Waiting for fairness artifact.',
    },
    { label: 'Runtime id', value: session?.session_id ?? 'Issued when the hand opens.' },
    { label: 'Hand id', value: hand?.hand_id ?? 'Pending' },
    { label: 'Active seat', value: hand ? String(hand.active_seat + 1) : 'Pending' },
    { label: 'Dealer upcard', value: hand?.dealer_upcard ? rankLabel(hand.dealer_upcard) : 'Pending' },
  ]

  const fairnessStatus = hand?.server_seed
    ? {
        label: 'Seed revealed for audit',
        tone: 'good' as const,
      }
    : fairnessArtifact
      ? fairnessArtifact.audit.passed
        ? {
            label: 'Public artifact checks passed',
            tone: 'good' as const,
          }
        : {
            label: 'Fairness artifact needs review',
            tone: 'warn' as const,
          }
      : hand
        ? {
            label: 'Transcript commitment active',
            tone: 'good' as const,
          }
        : {
            label: 'Open a hand to initialize a commitment',
            tone: 'neutral' as const,
          }

  const fairnessSummary = fairnessArtifact
      ? fairnessArtifact.audit.passed
        ? 'Blackjack now exposes an auditable hidden-card surface: reveal openings, dealer peek binding, and pre-settlement seed redaction are checked against the encrypted deck root.'
        : `Blackjack fairness artifact has inconsistencies: ${fairnessArtifact.audit.issues.join(', ')}.`
      : 'Blackjack commits a hidden dealer seed hash when the hand opens, uses that secret seed to drive the offchain shoe, and reveals the seed after settlement so the committed transcript can be replayed.'

  const liveStats: Array<{ label: string; value: string }> = []

  const settingsStats = [
    { label: 'Table max', value: formatStrk(tableState?.table.max_wager) },
    { label: 'Table min', value: formatStrk(tableState?.table.min_wager) },
    { label: 'Allowed actions', value: allowedActions.length ? allowedActions.join(', ') : 'Bet to begin' },
  ]
  const liveMaxHand = formatStrk(tableState?.table.max_wager)

  const pageClassName = `page page--blackjack${theatreMode ? ' page--theatre' : ''}`

  return (
    <section className={pageClassName}>
      <div className="blackjack-layout">
        <aside className="blackjack-sidebar">
          <section className="blackjack-sidebar__section">
            <div className="blackjack-sidebar__label-row">
              <span>Bet Amount</span>
              <strong>MAX {liveMaxHand}</strong>
            </div>
            <div className="blackjack-bet-row">
              <label className="dice-token-input">
                <input
                  className="text-input text-input--large dice-token-input__field"
                  inputMode="decimal"
                  onChange={(event) => setWager(event.target.value)}
                  type="text"
                  value={wager}
                />
                <span className="dice-token-input__token dice-token-input__token--label">STRK</span>
              </label>
              <button className="chip" onClick={() => adjustWager('half')} type="button">½</button>
              <button className="chip" onClick={() => adjustWager('double')} type="button">2×</button>
            </div>
          </section>

          <section className="blackjack-sidebar__section">
            <div className="blackjack-sidebar__label-row">
              <span>Profit on Win</span>
              <strong>{expectedProfit}</strong>
            </div>
            <label className="dice-token-input">
              <input className="text-input text-input--large dice-token-input__field" readOnly value={expectedProfit.replace(' STRK', '')} />
            </label>
          </section>

          <section className="blackjack-sidebar__section blackjack-sidebar__section--actions">
            {hand?.phase === 'insurance' ? (
              <div className="stack-note">
                Dealer shows Ace. Insurance available up to {formatStrk(hand.insurance.max_wager)}.
              </div>
            ) : null}
            <div className="blackjack-action-grid">
              {actionButtons.map((action) => {
                const enabled = allowedActions.includes(action.id)
                return (
                  <button
                    key={action.id}
                    className={enabled ? 'button button--ghost blackjack-action blackjack-action--available' : 'button button--ghost blackjack-action'}
                    disabled={!session || actionPending !== undefined || !enabled}
                    onClick={() => void handleRelayAction(action.id)}
                    type="button"
                  >
                    {actionPending === action.id ? 'Queued...' : action.label}
                  </button>
                )
              })}
            </div>
          </section>

          <div className="blackjack-sidebar__footer">
            <button
              className="button button--wide game-primary-action"
              disabled={betLocked || createPending || walletBusy}
              onClick={() => void handleCreateHand()}
              type="button"
            >
              {primaryActionLabel}
            </button>

            {statusMessage ? <p className="stack-note">{statusMessage}</p> : null}
            {walletError ? <p className="stack-note stack-note--error">{walletError}</p> : null}
          </div>
        </aside>

        <article className="blackjack-table-shell">
          <div className="blackjack-table-surface">
            <div className="blackjack-deck-stack" aria-hidden="true">
              <PlayingCard className="stage-playing-card blackjack-deck-stack__card blackjack-deck-stack__card--back-1" faceDown tilt={-4} />
              <PlayingCard className="stage-playing-card blackjack-deck-stack__card blackjack-deck-stack__card--back-2" faceDown tilt={1} />
              <PlayingCard className="stage-playing-card blackjack-deck-stack__card blackjack-deck-stack__card--front" faceDown tilt={5} />
            </div>
            <div className="blackjack-zone blackjack-zone--dealer">
              <div className="blackjack-total-badge">{dealerTotalLabel}</div>
              <div className="blackjack-card-stack">
                {hand?.dealer.cards.length ? (
                  hand.dealer.cards.map((card, index) => (
                    <div className="blackjack-card-shell" key={`dealer-${index}`} style={{ animationDelay: `${index * 70}ms` }}>
                      <PlayingCard
                        ariaLabel={card.revealed ? `${card.label} dealer card` : 'Dealer hole card'}
                        className="stage-playing-card"
                        faceDown={!card.revealed}
                        rank={card.label}
                        suit={displaySuit(card.label, 'dealer', index)}
                        tilt={cardTilt(index, hand.dealer.cards.length)}
                      />
                    </div>
                  ))
                ) : (
                  <>
                    <div className="blackjack-card-shell" style={{ animationDelay: '0ms' }}>
                      <PlayingCard
                        ariaLabel={hand?.dealer_upcard ? `${rankLabel(hand.dealer_upcard)} dealer upcard` : 'Waiting for dealer upcard'}
                        className="stage-playing-card"
                        placeholder={!hand?.dealer_upcard}
                        rank={rankLabel(hand?.dealer_upcard ?? null)}
                        suit={displaySuit(rankLabel(hand?.dealer_upcard ?? null), 'dealer-upcard', 0)}
                        tilt={-1}
                      />
                    </div>
                    <div className="blackjack-card-shell" style={{ animationDelay: '70ms' }}>
                      <PlayingCard
                        ariaLabel="Dealer hole card"
                        className="stage-playing-card"
                        faceDown
                        tilt={1}
                      />
                    </div>
                  </>
                )}
              </div>
            </div>

            <div className="blackjack-center-banner">
              <div>
                <span>BLACKJACK PAYS 3 TO 2</span>
              </div>
              <div>
                <span>INSURANCE PAYS 2 TO 1</span>
              </div>
            </div>

            <div className="blackjack-zone blackjack-zone--player">
              <div className={playerSeats.length > 1 ? 'blackjack-player-seats blackjack-player-seats--split' : 'blackjack-player-seats'}>
                {playerSeats.length ? (
                  playerSeats.map((seat) => {
                    const seatCardClass =
                      seat!.outcome === 'win' || seat!.outcome === 'blackjack'
                        ? 'stage-playing-card stage-playing-card--win'
                        : seat!.outcome === 'loss'
                          ? 'stage-playing-card stage-playing-card--loss'
                          : seat!.outcome === 'push'
                        ? 'stage-playing-card stage-playing-card--push'
                            : 'stage-playing-card'

                    return (
                      <div className={seatOutcomeTone(seat!.outcome)} key={`seat-${seat!.seat_index}`}>
                        <div className={badgeOutcomeTone(seat!.outcome)}>
                          {formatBlackjackTotal(seat!.total, seat!.soft)}
                        </div>
                        <div className="blackjack-card-stack">
                          {seat!.cards.map((card, index) => (
                            <div className="blackjack-card-shell" key={`seat-${seat!.seat_index}-card-${index}`} style={{ animationDelay: `${index * 70}ms` }}>
                              <PlayingCard
                                ariaLabel={`${card.label} player card`}
                                className={seatCardClass}
                                rank={card.label}
                                suit={displaySuit(card.label, `seat-${seat!.seat_index}`, index)}
                                tilt={cardTilt(index, seat!.cards.length)}
                              />
                            </div>
                          ))}
                        </div>
                        <div className="blackjack-seat-caption">
                          <span>Seat {seat!.seat_index + 1}</span>
                          <strong>{seat!.active ? 'Active' : seat!.outcome ?? seat!.status}</strong>
                        </div>
                      </div>
                    )
                  })
                ) : (
                  <div className="blackjack-player-seat blackjack-player-seat--placeholder">
                    <div className="blackjack-total-badge">—</div>
                    <div className="blackjack-card-stack">
                      <div className="blackjack-card-shell" style={{ animationDelay: '0ms' }}>
                        <PlayingCard ariaLabel="Empty player card slot" className="stage-playing-card" placeholder tilt={-1} />
                      </div>
                      <div className="blackjack-card-shell" style={{ animationDelay: '70ms' }}>
                        <PlayingCard ariaLabel="Empty player card slot" className="stage-playing-card" placeholder tilt={1} />
                      </div>
                    </div>
                    <div className="blackjack-seat-caption">
                      <span>Player</span>
                      <strong>{resolvedWalletAddress ? 'Ready to bet' : 'Connect to sit down'}</strong>
                    </div>
                  </div>
                )}
              </div>
            </div>
          </div>

          <GameUtilityBar
            fairnessFields={fairnessFields}
            fairnessStatus={fairnessStatus}
            fairnessSummary={fairnessSummary}
            liveStats={liveStats}
            onRegenerate={() => setStatusMessage('Open a new hand to rotate the committed blackjack transcript.')}
            onToggleTheatre={setTheatreMode}
            regenerateLabel="New hand"
            settingsStats={settingsStats}
            theatreMode={theatreMode}
          />
        </article>
      </div>
    </section>
  )
}
