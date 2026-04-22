import { morosConfig } from './config'

export type ServiceHealth = {
  service: string
  role: string
  status: string
  infra: {
    environment: string
    database_configured: boolean
    database_ready: boolean
    redis_configured: boolean
    redis_ready: boolean
    starknet_rpc_configured: boolean
  }
}

export type DepositRouterHealth = ServiceHealth & {
  deposit_master_configured: boolean
  executor_configured: boolean
  supported_asset_count: number
}

export type CoordinatorHand = {
  hand_id: string
  player: string
  table_id: number
  wager: string
  status: string
  phase: string
  transcript_root: string
  active_seat: number
  seat_count: number
  dealer_upcard?: number | null
  chain_hand_id?: number | null
  created_at: string
  updated_at: string
}

export type BlackjackCardView = {
  label: string
  revealed: boolean
}

export type BlackjackSeatView = {
  seat_index: number
  wager: string
  status: string
  outcome?: string | null
  payout: string
  doubled: boolean
  split_depth?: number
  split_aces?: boolean
  total: number
  soft: boolean
  is_blackjack: boolean
  active: boolean
  can_double: boolean
  can_split: boolean
  cards: BlackjackCardView[]
}

export type BlackjackDealerView = {
  cards: BlackjackCardView[]
  total?: number | null
  soft?: boolean | null
  hidden_cards: number
}

export type BlackjackInsuranceState = {
  offered: boolean
  supported: boolean
  max_wager: string
  wager: string
  taken: boolean
  settled: boolean
  outcome: string
}

export type BlackjackDealerPeekState = {
  required: boolean
  checked: boolean
  upcard_rank?: number | null
  hole_card_index?: number | null
  outcome: string
  proof_mode: string
  target_proof_mode: string
  target_proof_kind: string
  statement_kind: string
  public_inputs_hash: string
  hidden_value_class_commitment: string
  witness_commitment: string
  no_blackjack_proof: {
    available: boolean
    verifier_status: string
    verifier_namespace: string
    claim: string
    statement_hash: string
    statement: {
      hand_id: string
      player: string
      table_id: number
      ruleset_hash: string
      deck_commitment_root: string
      encrypted_deck_root: string
      dealer_upcard_rank?: number | null
      hole_card_index?: number | null
      statement_kind: string
    }
    current_proof_mode: string
    target_proof_mode: string
    current_proof_kind: string
    target_proof_kind: string
    statement_kind: string
    public_inputs_hash: string
    hidden_value_class_commitment: string
    witness_commitment: string
    receipt: {
      proof_kind: string
      receipt: string
      verified: boolean
    }
    opening: {
      leaf_hash: string
      leaf_index: number
      root: string
      siblings: string[]
      verified: boolean
    }
    zk_proof_target: {
      available: boolean
      verifier_namespace: string
      verifier_kind: string
      proof_system: string
      circuit_family: string
      circuit_id: string
      verification_key_id: string
      claim: string
      statement_hash: string
      public_inputs_hash: string
      encrypted_deck_root: string
      dealer_upcard_rank?: number | null
      hole_card_index?: number | null
      hidden_value_class_commitment: string
      witness_commitment: string
      artifact_hash: string
      request_id: string
    }
    proof_binding: {
      available: boolean
      status: string
      request_bound: boolean
      proof_verified: boolean
      verifier_namespace: string
      verifier_kind: string
      proof_system: string
      circuit_family: string
      circuit_id: string
      verification_key_id: string
      claim: string
      statement_hash: string
      public_inputs_hash: string
      target_artifact_hash: string
      request_id: string
      proof_id: string
    }
  }
  receipt: {
    proof_kind: string
    receipt: string
    verified: boolean
  }
  opening: {
    leaf_hash: string
    leaf_index: number
    root: string
    siblings: string[]
    verified: boolean
  }
}

export type BlackjackEncryptedCardEnvelopeView = {
  deck_index: number
  commitment: string
  ciphertext: string
  nonce_commitment: string
  reveal_key_commitment: string
}

export type BlackjackFairnessView = {
  protocol_mode: string
  target_protocol_mode: string
  encryption_scheme: string
  target_encryption_scheme: string
  deck_commitment_root: string
  reveal_count: number
  dealer_peek_required: boolean
  dealer_peek_status: string
  insurance_offered: boolean
  insurance_status: string
}

export type BlackjackHandView = {
  hand_id: string
  player: string
  table_id: number
  wager: string
  status: string
  phase: string
  transcript_root: string
  server_seed_hash: string
  server_seed?: string | null
  client_seed: string
  active_seat: number
  seat_count: number
  dealer_upcard?: number | null
  total_payout: string
  allowed_actions: string[]
  proof_verified: boolean
  insurance: BlackjackInsuranceState
  fairness: BlackjackFairnessView
  dealer: BlackjackDealerView
  seats: BlackjackSeatView[]
  action_log: Array<{
    action: string
    seat_index?: number | null
    detail: string
  }>
}

export type BlackjackFairnessArtifactView = {
  hand_id: string
  player: string
  table_id: number
  transcript_root: string
  protocol_mode: string
  target_protocol_mode: string
  commitment_scheme: string
  encryption_scheme: string
  target_encryption_scheme: string
  ruleset_hash: string
  deck_commitment_root: string
  encrypted_deck_root: string
  dealer_entropy_commitment: string
  player_entropy_commitment: string
  shuffle_commitment: string
  hole_card_index: number
  next_reveal_position: number
  server_seed_hash: string
  server_seed?: string | null
  client_seed_commitment: string
  client_seed?: string | null
  settled: boolean
  dealer_peek: BlackjackDealerPeekState
  insurance: BlackjackInsuranceState
  committed_cards: Array<{
    deck_index: number
    commitment: string
  }>
  encrypted_cards: BlackjackEncryptedCardEnvelopeView[]
  reveals: Array<{
    deck_index: number
    rank: number
    stage: string
    target: string
    proof_kind: string
    receipt: string
    verified: boolean
    opening: {
      leaf_hash: string
      leaf_index: number
      root: string
      siblings: string[]
      verified: boolean
    }
  }>
  audit: {
    mode: string
    passed: boolean
    reveal_openings_verified: boolean
    dealer_peek_opening_verified: boolean
    dealer_peek_statement_hash_verified: boolean
    dealer_peek_public_inputs_hash_verified: boolean
    dealer_peek_artifact_consistent: boolean
    dealer_peek_zk_target_consistent: boolean
    dealer_peek_proof_binding_verified: boolean
    settlement_redaction_respected: boolean
    issues: string[]
  }
}

export type CreateCoordinatorHandRequest = {
  table_id: number
  player: string
  wager: string
  client_seed?: string
}

export type CreateCoordinatorHandResponse = {
  hand_id: string
  session_id: string
  transcript_root: string
  relay_token: string
  phase: string
  runtime_cached: boolean
}

export type SessionRuntimeResponse = {
  session: {
    session_id: string
    hand_id: string
    player: string
    table_id: number
    transcript_root: string
    status: string
    phase: string
    allowed_actions: string[]
    expires_at_unix: number
  }
}

export type GameplaySessionChallengeRequest = {
  wallet_address: string
}

export type GameplaySessionChallengeResponse = {
  wallet_address: string
  challenge_id: string
  expires_at_unix: number
  typed_data: Record<string, unknown>
}

export type CreateGameplaySessionRequest = {
  wallet_address: string
  challenge_id: string
  signature: string[]
}

export type GameplaySessionResponse = {
  session_token: string
  wallet_address: string
  expires_at_unix: number
}

export type BlackjackTableState = {
  table: {
    table_id: number
    table_contract: string
    game_kind: string
    status: string
    min_wager: string
    max_wager: string
  }
  house_available: string
  house_locked: string
  recommended_house_bankroll: string
  fully_covered_max_wager: string
  player_balance?: string | null
  player_fully_covered_max_wager?: string | null
}

export type CoordinatorTableStateResponse = {
  state: BlackjackTableState
  live_players: number
}

export type BalanceAccount = {
  user_id: string
  gambling_balance: string
  gambling_reserved: string
  vault_balance: string
  updated_at: string
}

export type PlayerAccount = {
  user_id: string
  wallet_address?: string | null
  created_at: string
  updated_at: string
}

export type WithdrawalRequest = {
  withdrawal_id: string
  user_id: string
  requested_by_wallet?: string | null
  source_balance: 'vault' | 'gambling' | string
  destination_chain_key: string
  destination_asset_symbol: string
  destination_address: string
  amount_raw: string
  route_kind: string
  status: string
  route_job_id?: string | null
  destination_tx_hash?: string | null
  failure_reason?: string | null
  metadata: Record<string, unknown>
  created_at: string
  updated_at: string
  completed_at?: string | null
}

export type RelayActionRequest = {
  hand_id: string
  action: string
  relay_token: string
}

export type RelayActionResponse = {
  relay_id: string
  status: string
  hand?: BlackjackHandView
}

export type DiceRoundView = {
  round_id: number
  table_id: number
  player: string
  wager: string
  status: string
  transcript_root: string
  commitment_id: number
  server_seed_hash: string
  client_seed: string
  target_bps: number
  roll_over: boolean
  roll_bps: number
  chance_bps: number
  multiplier_bps: number
  payout: string
  win: boolean
}

export type DiceCommitmentView = {
  commitment_id: number
  server_seed_hash: string
  reveal_deadline: number
  status: string
  round_id: number
}

export type DiceQuoteView = {
  chance_bps: number
  multiplier_bps: number
  payout: string
  exposure: string
}

export type CreateDiceCommitmentResponse = {
  commitment: DiceCommitmentView
  tx_hash: string
}

export type SettleDiceCommitmentResponse = {
  round: DiceRoundView
  tx_hash: string
  server_seed: string
}

export type RouletteBetView = {
  kind: number
  selection: number
  amount: string
  payout_multiplier: string
  payout: string
  win: boolean
}

export type RouletteSpinView = {
  spin_id: number
  table_id: number
  player: string
  wager: string
  status: string
  transcript_root: string
  commitment_id: number
  server_seed_hash: string
  client_seed: string
  result_number: number
  bet_count: number
  payout: string
  bets: RouletteBetView[]
}

export type SettleRouletteCommitmentResponse = {
  spin: RouletteSpinView
  tx_hash: string
  server_seed: string
}

export type BaccaratRoundView = {
  round_id: number
  table_id: number
  player: string
  wager: string
  status: string
  transcript_root: string
  commitment_id: number
  server_seed_hash: string
  client_seed: string
  bet_side: number
  player_total: number
  banker_total: number
  player_card_count: number
  banker_card_count: number
  winner: number
  payout: string
  player_cards: number[]
  banker_cards: number[]
  player_card_positions: number[]
  banker_card_positions: number[]
  player_card_draw_indices: number[]
  banker_card_draw_indices: number[]
  player_card_attempts: number[]
  banker_card_attempts: number[]
  player_card_commitments: string[]
  banker_card_commitments: string[]
}

export type BetFeedItem = {
  game: string
  user: string
  wallet_address: string
  bet_amount: string
  multiplier_bps: string
  payout: string
  tx_hash?: string | null
  settled_at: string
}

export type BetFeedResponse = {
  my_bets: BetFeedItem[]
  all_bets: BetFeedItem[]
  high_rollers: BetFeedItem[]
  race_leaderboard: BetFeedItem[]
}

export type SettleBaccaratCommitmentResponse = {
  round: BaccaratRoundView
  tx_hash: string
  server_seed: string
}

export type OpenDiceRoundRequest = {
  table_id: number
  player: string
  wager: string
  target_bps: number
  roll_over: boolean
  client_seed: string
  commitment_id: number
}

export type OpenRouletteSpinRequest = {
  table_id: number
  player: string
  total_wager: string
  client_seed: string
  commitment_id: number
  bets: Array<{
    kind: number
    selection: number
    amount: string
  }>
}

export type OpenBaccaratRoundRequest = {
  table_id: number
  player: string
  wager: string
  bet_side: number
  client_seed: string
  commitment_id: number
}

export type OpenGameplayResponse = {
  game_id: number
  tx_hash: string
}

export type UsernameAvailabilityResponse = {
  username: string
  available: boolean
}

export type PlayerProfile = {
  wallet_address?: string | null
  username?: string | null
  auth_provider: string
  auth_subject?: string | null
  created_at: string
  updated_at: string
}

export type ProfileClaimChallengeRequest = {
  wallet_address: string
  username?: string | null
  auth_provider?: string
}

export type ProfileClaimChallengeResponse = {
  wallet_address: string
  username?: string | null
  auth_provider: string
  challenge_id: string
  expires_at_unix: number
  typed_data: Record<string, unknown>
}

export type DepositSupportedAsset = {
  id: string
  chain_key: string
  chain_family: string
  network: string
  chain_id: string
  asset_symbol: string
  asset_address: string
  asset_decimals: number
  route_kind: string
  watch_mode: string
  min_amount: string
  max_amount: string
  confirmations_required: number
  status: string
  metadata: Record<string, unknown>
  created_at: string
  updated_at: string
}

export type DepositChannel = {
  channel_id: string
  wallet_address?: string | null
  username?: string | null
  asset_id: string
  chain_key: string
  asset_symbol: string
  deposit_address: string
  qr_payload: string
  route_kind: string
  status: string
  watch_from_block?: number | null
  last_scanned_block?: number | null
  last_seen_at?: string | null
  created_at: string
  updated_at: string
}

export type DepositTransfer = {
  transfer_id: string
  channel_id: string
  wallet_address?: string | null
  username?: string | null
  asset_id: string
  chain_key: string
  asset_symbol: string
  deposit_address: string
  sender_address?: string | null
  tx_hash: string
  block_number?: number | null
  amount_raw: string
  amount_display: string
  confirmations: number
  required_confirmations: number
  status: string
  risk_state: string
  credit_target?: string | null
  destination_tx_hash?: string | null
  detected_at: string
  confirmed_at?: string | null
  completed_at?: string | null
  created_at: string
  updated_at: string
}

export type DepositRouteJob = {
  job_id: string
  transfer_id: string
  job_type: string
  status: string
  attempts: number
  payload: Record<string, unknown>
  response?: Record<string, unknown> | null
  last_error?: string | null
  created_at: string
  updated_at: string
}

export type DepositRiskFlag = {
  flag_id: string
  transfer_id: string
  code: string
  severity: string
  description: string
  resolution_status: string
  resolution_notes?: string | null
  created_at: string
  resolved_at?: string | null
}

export type DepositRecovery = {
  recovery_id: string
  transfer_id: string
  reason: string
  notes?: string | null
  requested_by?: string | null
  status: string
  resolution_notes?: string | null
  created_at: string
  updated_at: string
  resolved_at?: string | null
}

export type CreateDepositChannelRequest = {
  wallet_address: string
  asset_id: string
  chain_key: string
}

export type CreateAuthenticatedDepositChannelRequest = {
  asset_id: string
  chain_key: string
}

export type CreateDepositChannelResponse = {
  channel: DepositChannel
  asset: DepositSupportedAsset
  status_url: string
}

export type DepositStatusResponse = {
  channel: DepositChannel
  transfers: DepositTransfer[]
  route_jobs: DepositRouteJob[]
  risk_flags: DepositRiskFlag[]
  recoveries: DepositRecovery[]
}

export type OperatorSession = {
  user_id: string
  emails: string[]
  wallets: string[]
  matches: string[]
}

export type ResolveOperatorRiskFlagRequest = {
  resolution_status: string
  resolution_notes?: string | null
}

export type ResolveOperatorRecoveryRequest = {
  status: string
  resolution_notes?: string | null
}

function describeMorosService(url: string) {
  const normalized = url.toLowerCase()

  if (normalized.includes('/coordinator')) {
    return 'the Moros coordinator service'
  }
  if (normalized.includes('/relayer')) {
    return 'the Moros relayer service'
  }
  if (normalized.includes('/indexer')) {
    return 'the Moros indexer service'
  }
  if (normalized.includes('/deposit')) {
    return 'the Moros deposit service'
  }
  if (normalized.includes('/auth')) {
    return 'the Moros auth service'
  }

  return 'the Moros service'
}

async function fetchJson<T>(url: string, init?: RequestInit): Promise<T> {
  let response: Response
  try {
    response = await fetch(url, init)
  } catch {
    throw new Error(`Could not reach ${describeMorosService(url)}. Check your Moros service connection and try again.`)
  }
  if (!response.ok) {
    const body = await response.text()
    throw new Error(`Request failed for ${describeMorosService(url)}: ${response.status} ${body}`)
  }
  return response.json() as Promise<T>
}

async function fetchPrivyBridgeJson<T>(path: string, idToken: string, init?: RequestInit): Promise<T> {
  const headers = new Headers(init?.headers)
  headers.set('authorization', `Bearer ${idToken}`)
  return fetchJson<T>(`${morosConfig.privyBridgeUrl}${path}`, {
    ...init,
    headers,
  })
}

function withBearerAuthorization(init: RequestInit | undefined, token?: string) {
  if (!token) {
    return init
  }
  const headers = new Headers(init?.headers)
  headers.set('authorization', `Bearer ${token}`)
  return {
    ...init,
    headers,
  }
}

export function fetchCoordinatorHealth() {
  return fetchJson<ServiceHealth>(`${morosConfig.coordinatorUrl}/`)
}

export function fetchRelayerHealth() {
  return fetchJson<ServiceHealth>(`${morosConfig.relayerUrl}/`)
}

export function fetchIndexerHealth() {
  return fetchJson<ServiceHealth>(`${morosConfig.indexerUrl}/`)
}

export function fetchDepositRouterHealth() {
  return fetchJson<DepositRouterHealth>(`${morosConfig.depositRouterUrl}/`)
}

export function fetchDepositSupportedAssets() {
  return fetchJson<DepositSupportedAsset[]>(`${morosConfig.depositRouterUrl}/v1/deposits/supported-assets`)
}

export function createDepositChannel(payload: CreateDepositChannelRequest) {
  return fetchJson<CreateDepositChannelResponse>(`${morosConfig.depositRouterUrl}/v1/deposits`, {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
    },
    body: JSON.stringify(payload),
  })
}

export function createAuthenticatedDepositChannel(
  idToken: string,
  payload: CreateAuthenticatedDepositChannelRequest,
) {
  return fetchPrivyBridgeJson<CreateDepositChannelResponse>('/v1/deposits/channels', idToken, {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
    },
    body: JSON.stringify(payload),
  })
}

export function fetchDepositStatus(depositAddress: string) {
  return fetchJson<DepositStatusResponse>(
    `${morosConfig.depositRouterUrl}/v1/deposits/status/${encodeURIComponent(depositAddress)}`,
  )
}

export function fetchOperatorSession(idToken: string) {
  return fetchPrivyBridgeJson<OperatorSession>('/v1/operators/session', idToken)
}

export function fetchOperatorRouteJobs(idToken: string, options: { status?: string; limit?: number } = {}) {
  const url = new URL('/v1/operators/deposits/route-jobs', morosConfig.privyBridgeUrl)
  if (options.status) {
    url.searchParams.set('status', options.status)
  }
  if (typeof options.limit === 'number') {
    url.searchParams.set('limit', String(options.limit))
  }
  return fetchPrivyBridgeJson<DepositRouteJob[]>(
    `${url.pathname}${url.search}`,
    idToken,
  )
}

export function retryOperatorRouteJob(idToken: string, jobId: string) {
  return fetchPrivyBridgeJson<DepositRouteJob>(
    `/v1/operators/deposits/route-jobs/${encodeURIComponent(jobId)}/retry`,
    idToken,
    {
      method: 'POST',
    },
  )
}

export function fetchOperatorRiskFlags(idToken: string) {
  return fetchPrivyBridgeJson<DepositRiskFlag[]>('/v1/operators/deposits/risk-flags', idToken)
}

export function resolveOperatorRiskFlag(
  idToken: string,
  flagId: string,
  payload: ResolveOperatorRiskFlagRequest,
) {
  return fetchPrivyBridgeJson<DepositRiskFlag>(
    `/v1/operators/deposits/risk-flags/${encodeURIComponent(flagId)}/resolve`,
    idToken,
    {
      method: 'POST',
      headers: {
        'content-type': 'application/json',
      },
      body: JSON.stringify(payload),
    },
  )
}

export function fetchOperatorRecoveries(idToken: string, options: { status?: string; limit?: number } = {}) {
  const url = new URL('/v1/operators/deposits/recoveries', morosConfig.privyBridgeUrl)
  if (options.status) {
    url.searchParams.set('status', options.status)
  }
  if (typeof options.limit === 'number') {
    url.searchParams.set('limit', String(options.limit))
  }
  return fetchPrivyBridgeJson<DepositRecovery[]>(
    `${url.pathname}${url.search}`,
    idToken,
  )
}

export function resolveOperatorRecovery(
  idToken: string,
  recoveryId: string,
  payload: ResolveOperatorRecoveryRequest,
) {
  return fetchPrivyBridgeJson<DepositRecovery>(
    `/v1/operators/deposits/recoveries/${encodeURIComponent(recoveryId)}/resolve`,
    idToken,
    {
      method: 'POST',
      headers: {
        'content-type': 'application/json',
      },
      body: JSON.stringify(payload),
    },
  )
}

export function createCoordinatorHand(payload: CreateCoordinatorHandRequest, sessionToken?: string) {
  return fetchJson<CreateCoordinatorHandResponse>(`${morosConfig.coordinatorUrl}/v1/hands`, {
    method: 'POST',
    headers: {
      ...(sessionToken ? { authorization: `Bearer ${sessionToken}` } : {}),
      'content-type': 'application/json',
    },
    body: JSON.stringify(payload),
  })
}

export function fetchCoordinatorHand(handId: string, authToken?: string) {
  return fetchJson<CoordinatorHand>(
    `${morosConfig.coordinatorUrl}/v1/hands/${handId}`,
    withBearerAuthorization(undefined, authToken),
  )
}

export function fetchCoordinatorHandView(handId: string, authToken?: string) {
  return fetchJson<BlackjackHandView>(
    `${morosConfig.coordinatorUrl}/v1/hands/${handId}/view`,
    withBearerAuthorization(undefined, authToken),
  )
}

export function fetchCoordinatorHandFairness(handId: string, authToken?: string) {
  return fetchJson<BlackjackFairnessArtifactView>(
    `${morosConfig.coordinatorUrl}/v1/hands/${handId}/fairness`,
    withBearerAuthorization(undefined, authToken),
  )
}

export function fetchCoordinatorSession(sessionId: string, authToken?: string) {
  return fetchJson<SessionRuntimeResponse>(
    `${morosConfig.coordinatorUrl}/v1/sessions/${sessionId}`,
    withBearerAuthorization(undefined, authToken),
  )
}

export function createGameplaySessionChallenge(payload: GameplaySessionChallengeRequest) {
  return fetchJson<GameplaySessionChallengeResponse>(
    `${morosConfig.coordinatorUrl}/v1/gameplay/sessions/challenge`,
    {
      method: 'POST',
      headers: {
        'content-type': 'application/json',
      },
      body: JSON.stringify(payload),
    },
  )
}

export function createGameplaySession(payload: CreateGameplaySessionRequest) {
  return fetchJson<GameplaySessionResponse>(`${morosConfig.coordinatorUrl}/v1/gameplay/sessions`, {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
    },
    body: JSON.stringify(payload),
  })
}

export function fetchCoordinatorTableState(tableId: number, player?: string) {
  const url = new URL(`${morosConfig.coordinatorUrl}/v1/tables/${tableId}/state`)
  if (player) {
    url.searchParams.set('player', player)
  }
  return fetchJson<CoordinatorTableStateResponse>(url.toString())
}

export type AccountResolveChallengeResponse = {
  wallet_address: string
  challenge_id: string
  expires_at_unix: number
  typed_data: Record<string, unknown>
}

export function fetchMorosAccountByUserId(userId: string) {
  return fetchJson<PlayerAccount>(`${morosConfig.coordinatorUrl}/v1/accounts/resolve`, {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
    },
    body: JSON.stringify({
      user_id: userId,
    }),
  })
}

export function createMorosAccountResolveChallenge(payload: {
  wallet_address: string
  linked_via?: string
  make_primary?: boolean
}) {
  return fetchJson<AccountResolveChallengeResponse>(
    `${morosConfig.coordinatorUrl}/v1/accounts/resolve/challenge`,
    {
      method: 'POST',
      headers: {
        'content-type': 'application/json',
      },
      body: JSON.stringify(payload),
    },
  )
}

export function resolveMorosWalletAccount(payload: {
  wallet_address: string
  challenge_id: string
  signature: string[]
}) {
  return fetchJson<PlayerAccount>(`${morosConfig.coordinatorUrl}/v1/accounts/resolve`, {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
    },
    body: JSON.stringify(payload),
  })
}

export function resolveMorosPrivyAccount(
  idToken: string,
  payload: {
    wallet_address?: string
    linked_via?: string
    make_primary?: boolean
  },
) {
  return fetchPrivyBridgeJson<PlayerAccount>('/v1/accounts/resolve', idToken, {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
    },
    body: JSON.stringify(payload),
  })
}

export function fetchAccountBalances(payload: { userId: string }, authToken?: string) {
  if (!payload.userId) {
    throw new Error('userId is required to fetch account balances.')
  }

  return fetchJson<BalanceAccount>(
    `${morosConfig.coordinatorUrl}/v1/accounts/users/${encodeURIComponent(payload.userId)}/balances`,
    withBearerAuthorization(undefined, authToken),
  )
}

export function fetchAccountBalancesByWalletAddress(walletAddress: string) {
  if (!walletAddress) {
    throw new Error('walletAddress is required to fetch account balances.')
  }

  return fetchJson<BalanceAccount>(
    `${morosConfig.coordinatorUrl}/v1/accounts/wallets/${encodeURIComponent(walletAddress)}/balances`,
  )
}

export function transferAccountBalances(payload: {
  user_id?: string
  wallet_address?: string
  direction: 'gambling_to_vault' | 'vault_to_gambling'
  amount: string
}, authToken?: string) {
  return fetchJson<BalanceAccount>(`${morosConfig.coordinatorUrl}/v1/accounts/transfers`, withBearerAuthorization({
    method: 'POST',
    headers: {
      'content-type': 'application/json',
    },
    body: JSON.stringify(payload),
  }, authToken))
}

export function createAccountWithdrawal(payload: {
  user_id?: string
  wallet_address?: string
  amount: string
  destination_address: string
  source_balance?: 'vault' | 'gambling'
  destination_chain_key?: string
  destination_asset_symbol?: string
  destination_tx_hash?: string
}, authToken?: string) {
  return fetchJson<WithdrawalRequest>(`${morosConfig.coordinatorUrl}/v1/accounts/withdrawals`, withBearerAuthorization({
    method: 'POST',
    headers: {
      'content-type': 'application/json',
    },
    body: JSON.stringify(payload),
  }, authToken))
}

export function fetchAccountWithdrawals(payload: { userId: string }, limit = 10, authToken?: string) {
  if (!payload.userId) {
    throw new Error('userId is required to fetch withdrawals.')
  }

  const url = new URL(
    `${morosConfig.coordinatorUrl}/v1/accounts/users/${encodeURIComponent(payload.userId)}/withdrawals`,
  )
  url.searchParams.set('limit', String(limit))
  return fetchJson<WithdrawalRequest[]>(url.toString(), withBearerAuthorization(undefined, authToken))
}

export function relayHandAction(payload: RelayActionRequest) {
  return fetchJson<RelayActionResponse>(`${morosConfig.relayerUrl}/v1/actions`, {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
    },
    body: JSON.stringify(payload),
  })
}

export function createDiceCommitment() {
  return fetchJson<CreateDiceCommitmentResponse>(`${morosConfig.coordinatorUrl}/v1/dice/commitments`, {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
    },
    body: JSON.stringify({}),
  })
}

export function settleDiceCommitment(commitmentId: number) {
  return fetchJson<SettleDiceCommitmentResponse>(`${morosConfig.coordinatorUrl}/v1/dice/commitments/${commitmentId}/settle`, {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
    },
    body: JSON.stringify({}),
  })
}

export function fetchDiceRound(roundId: number) {
  return fetchJson<DiceRoundView>(`${morosConfig.coordinatorUrl}/v1/dice/rounds/${roundId}`)
}

export function openDiceRoundRelayed(sessionToken: string, payload: OpenDiceRoundRequest) {
  return fetchJson<OpenGameplayResponse>(`${morosConfig.coordinatorUrl}/v1/dice/rounds`, {
    method: 'POST',
    headers: {
      authorization: `Bearer ${sessionToken}`,
      'content-type': 'application/json',
    },
    body: JSON.stringify(payload),
  })
}

export function createRouletteCommitment() {
  return fetchJson<CreateDiceCommitmentResponse>(`${morosConfig.coordinatorUrl}/v1/roulette/commitments`, {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
    },
    body: JSON.stringify({}),
  })
}

export function settleRouletteCommitment(commitmentId: number) {
  return fetchJson<SettleRouletteCommitmentResponse>(`${morosConfig.coordinatorUrl}/v1/roulette/commitments/${commitmentId}/settle`, {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
    },
    body: JSON.stringify({}),
  })
}

export function fetchRouletteSpin(spinId: number) {
  return fetchJson<RouletteSpinView>(`${morosConfig.coordinatorUrl}/v1/roulette/spins/${spinId}`)
}

export function openRouletteSpinRelayed(sessionToken: string, payload: OpenRouletteSpinRequest) {
  return fetchJson<OpenGameplayResponse>(`${morosConfig.coordinatorUrl}/v1/roulette/spins`, {
    method: 'POST',
    headers: {
      authorization: `Bearer ${sessionToken}`,
      'content-type': 'application/json',
    },
    body: JSON.stringify(payload),
  })
}

export function createBaccaratCommitment() {
  return fetchJson<CreateDiceCommitmentResponse>(`${morosConfig.coordinatorUrl}/v1/baccarat/commitments`, {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
    },
    body: JSON.stringify({}),
  })
}

export function settleBaccaratCommitment(commitmentId: number) {
  return fetchJson<SettleBaccaratCommitmentResponse>(`${morosConfig.coordinatorUrl}/v1/baccarat/commitments/${commitmentId}/settle`, {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
    },
    body: JSON.stringify({}),
  })
}

export function fetchBaccaratRound(roundId: number) {
  return fetchJson<BaccaratRoundView>(`${morosConfig.coordinatorUrl}/v1/baccarat/rounds/${roundId}`)
}

export function openBaccaratRoundRelayed(sessionToken: string, payload: OpenBaccaratRoundRequest) {
  return fetchJson<OpenGameplayResponse>(`${morosConfig.coordinatorUrl}/v1/baccarat/rounds`, {
    method: 'POST',
    headers: {
      authorization: `Bearer ${sessionToken}`,
      'content-type': 'application/json',
    },
    body: JSON.stringify(payload),
  })
}

export function fetchBetFeed(payload?: { userId?: string; walletAddress?: string }) {
  const url = new URL(`${morosConfig.coordinatorUrl}/v1/bets`)
  if (payload?.userId) {
    url.searchParams.set('user_id', payload.userId)
  }
  if (payload?.walletAddress) {
    url.searchParams.set('player', payload.walletAddress)
  }
  return fetchJson<BetFeedResponse>(url.toString())
}

export function fetchUsernameAvailability(username: string) {
  return fetchJson<UsernameAvailabilityResponse>(
    `${morosConfig.coordinatorUrl}/v1/auth/usernames/${encodeURIComponent(username)}/availability`,
  )
}

export function fetchPlayerProfile(walletAddress: string) {
  return fetchJson<PlayerProfile>(
    `${morosConfig.coordinatorUrl}/v1/auth/profiles/${encodeURIComponent(walletAddress)}`,
  )
}

export function fetchPlayerProfileByUsername(username: string) {
  return fetchJson<PlayerProfile>(
    `${morosConfig.coordinatorUrl}/v1/auth/profiles/by-username/${encodeURIComponent(username)}`,
  )
}

export function fetchProfileClaimChallenge(payload: ProfileClaimChallengeRequest) {
  return fetchJson<ProfileClaimChallengeResponse>(`${morosConfig.coordinatorUrl}/v1/auth/profiles/challenge`, {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
    },
    body: JSON.stringify(payload),
  })
}

export function upsertPlayerProfile(payload: {
  wallet_address: string
  username?: string | null
  auth_provider?: string
  challenge_id: string
  signature: string[]
}) {
  return fetchJson<PlayerProfile>(`${morosConfig.coordinatorUrl}/v1/auth/profiles`, {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
    },
    body: JSON.stringify(payload),
  })
}

export type RewardKind = 'rakeback' | 'weekly' | 'level_up' | 'referral'

export type RewardsTier = {
  level: number
  name: string
  threshold_raw: string
  rakeback_bps: number
  weekly_bps: number
  level_up_bonus_raw: string
}

export type RewardsConfig = {
  budget_share_bps: number
  rakeback_share_bps: number
  weekly_share_bps: number
  level_up_share_bps: number
  referral_rate_bps: number
  max_counted_wager_per_bet_raw: string
  rewards_pool_cap_raw?: string | null
  rakeback_user_cap_raw: string
  weekly_user_cap_raw: string
  global_epoch_cap_raw: string
  referral_user_cap_raw: string
  referral_global_cap_raw: string
  weekly_min_weighted_volume_raw: string
  claim_reservation_ttl_seconds: number
  blackjack_reward_house_edge_bps: number
  dice_reward_house_edge_bps: number
  roulette_reward_house_edge_bps: number
  baccarat_reward_house_edge_bps: number
  tiers: RewardsTier[]
}

export type VipProgress = {
  lifetime_wager_raw: string
  wager_7d_raw: string
  wager_30d_raw: string
  lifetime_weighted_volume_raw: string
  weighted_volume_7d_raw: string
  weighted_volume_30d_raw: string
  vip_points_raw: string
  current_tier_level: number
  current_tier_name: string
  next_tier_level?: number | null
  next_tier_name?: string | null
  next_tier_threshold_raw?: string | null
  progress_bps: number
}

export type RewardBucket = {
  accrued_raw: string
  claimed_raw: string
  claimable_raw: string
  scale_bps: number
}

export type RewardEpoch = {
  epoch_key: string
  tier_level: number
  tier_name: string
  wager_volume_raw: string
  weighted_volume_raw: string
  raw_bonus_raw: string
  claimable_raw: string
  scale_bps: number
}

export type LevelUpReward = {
  tier_level: number
  tier_name: string
  bonus_raw: string
  claimable_raw: string
  crossed_at_unix: number
  scale_bps: number
}

export type ReferralRewards = {
  referrer_wallet_address?: string | null
  referrer_username?: string | null
  linked_at_unix?: number | null
  referred_users: number
  accrued_raw: string
  claimed_raw: string
  claimable_raw: string
  referral_rate_bps: number
}

export type GlobalRewardsVolume = {
  lifetime_wager_raw: string
  lifetime_weighted_volume_raw: string
  weighted_volume_7d_raw: string
  weighted_volume_30d_raw: string
}

export type RewardsState = {
  wallet_address: string
  vip: VipProgress
  global_volume?: GlobalRewardsVolume
  rakeback: RewardBucket
  weekly: RewardBucket
  level_up: RewardBucket
  referral: ReferralRewards
  rakeback_epochs: RewardEpoch[]
  weekly_epochs: RewardEpoch[]
  level_up_rewards: LevelUpReward[]
  claimable_total_raw: string
  config: RewardsConfig
}

export type RewardsClaimChallengeResponse = {
  wallet_address: string
  reward_kind: RewardKind
  claim_id: string
  amount_raw: string
  challenge_id: string
  expires_at_unix: number
  typed_data: Record<string, unknown>
}

export type ClaimRewardsResponse = {
  reward_kind: RewardKind
  claim_id: string
  amount_raw: string
  tx_hash: string
  claim_rows: number
  status: string
}

export type RewardCoupon = {
  id: string
  code: string
  description?: string | null
  amount_raw: string
  max_global_redemptions: number
  max_per_user_redemptions: number
  redeemed_count: number
  active: boolean
  starts_at_unix?: number | null
  expires_at_unix?: number | null
  created_by?: string | null
  created_at_unix: number
  updated_at_unix: number
}

export type CreateRewardCouponResponse = {
  coupons: RewardCoupon[]
}

export type RedeemRewardCouponResponse = {
  redemption_id: string
  coupon_id: string
  code: string
  amount_raw: string
  tx_hash: string
  status: string
}

export type ReferrerLinkChallengeResponse = {
  wallet_address: string
  referrer: string
  challenge_id: string
  expires_at_unix: number
  typed_data: Record<string, unknown>
}

export type ReferralBinding = {
  referrer_wallet_address?: string | null
  referrer_username?: string | null
  linked_at_unix: number
}

type RewardIdentityPayload = {
  userId?: string
  walletAddress?: string
}

function normalizeRewardIdentityPayload(payload: RewardIdentityPayload) {
  const userId = payload.userId?.trim()
  const walletAddress = payload.walletAddress?.trim()
  if (!userId && !walletAddress) {
    throw new Error('userId or walletAddress is required for rewards requests.')
  }
  return {
    user_id: userId,
    wallet_address: walletAddress,
  }
}

export function fetchRewardsState(payload: RewardIdentityPayload, authToken?: string) {
  const identity = normalizeRewardIdentityPayload(payload)
  if (identity.user_id && authToken) {
    return fetchJson<RewardsState>(
      `${morosConfig.coordinatorUrl}/v1/accounts/users/${encodeURIComponent(identity.user_id)}/rewards`,
      withBearerAuthorization(undefined, authToken),
    )
  }
  if (!identity.wallet_address) {
    throw new Error('walletAddress is required when no rewards auth token is supplied.')
  }
  return fetchJson<RewardsState>(
    `${morosConfig.coordinatorUrl}/v1/rewards/${encodeURIComponent(identity.wallet_address!)}`,
  )
}

export function createRewardsClaimChallenge(payload: {
  user_id?: string
  wallet_address?: string
  reward_kind: RewardKind
}) {
  return fetchJson<RewardsClaimChallengeResponse>(
    `${morosConfig.coordinatorUrl}/v1/rewards/claims/challenge`,
    {
      method: 'POST',
      headers: {
        'content-type': 'application/json',
      },
      body: JSON.stringify({
        ...normalizeRewardIdentityPayload({
          userId: payload.user_id,
          walletAddress: payload.wallet_address,
        }),
        reward_kind: payload.reward_kind,
      }),
    },
  )
}

export function claimRewards(payload: {
  user_id?: string
  wallet_address?: string
  reward_kind: RewardKind
  claim_id: string
  challenge_id: string
  signature: string[]
}) {
  return fetchJson<ClaimRewardsResponse>(`${morosConfig.coordinatorUrl}/v1/rewards/claims`, {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
    },
    body: JSON.stringify({
      ...normalizeRewardIdentityPayload({
        userId: payload.user_id,
        walletAddress: payload.wallet_address,
      }),
      reward_kind: payload.reward_kind,
      claim_id: payload.claim_id,
      challenge_id: payload.challenge_id,
      signature: payload.signature,
    }),
  })
}

export function redeemRewardCoupon(
  payload: {
    user_id?: string
    wallet_address?: string
    code: string
  },
  authToken: string,
) {
  const headers = new Headers()
  headers.set('authorization', `Bearer ${authToken}`)
  headers.set('content-type', 'application/json')
  return fetchJson<RedeemRewardCouponResponse>(`${morosConfig.coordinatorUrl}/v1/rewards/coupons/redeem`, {
    method: 'POST',
    headers,
    body: JSON.stringify({
      ...normalizeRewardIdentityPayload({
        userId: payload.user_id,
        walletAddress: payload.wallet_address,
      }),
      code: payload.code,
    }),
  })
}

export function createReferrerLinkChallenge(payload: {
  user_id?: string
  wallet_address?: string
  referrer: string
}) {
  return fetchJson<ReferrerLinkChallengeResponse>(
    `${morosConfig.coordinatorUrl}/v1/rewards/referrer/challenge`,
    {
      method: 'POST',
      headers: {
        'content-type': 'application/json',
      },
      body: JSON.stringify({
        ...normalizeRewardIdentityPayload({
          userId: payload.user_id,
          walletAddress: payload.wallet_address,
        }),
        referrer: payload.referrer,
      }),
    },
  )
}

export function setReferrer(payload: {
  user_id?: string
  wallet_address?: string
  referrer: string
  challenge_id: string
  signature: string[]
}) {
  return fetchJson<ReferralBinding>(`${morosConfig.coordinatorUrl}/v1/rewards/referrer`, {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
    },
    body: JSON.stringify({
      ...normalizeRewardIdentityPayload({
        userId: payload.user_id,
        walletAddress: payload.wallet_address,
      }),
      referrer: payload.referrer,
      challenge_id: payload.challenge_id,
      signature: payload.signature,
    }),
  })
}
