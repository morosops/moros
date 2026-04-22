use anyhow::{Context, bail};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use moros_common::{
    accounts, balances, blackjack,
    chain::{
        BaccaratRoundView, BlackjackTableState, ChainService, DiceCommitmentView, DiceRoundView,
        RouletteSpinView,
    },
    config::ServiceConfig,
    infra::{InfraSnapshot, ServiceInfra},
    persistence::{self, CreateHandInput},
    rewards::{self, ReferralBindingView, RewardKind, RewardsConfig, RewardsStateView},
    runtime::{self, SessionRuntimeState},
    telemetry,
    web::base_router,
    withdrawals,
};
use redis::{AsyncCommands, Client as RedisClient};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use starknet::core::types::{Felt, TypedData};
use std::sync::Arc;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use uuid::Uuid;

const GAMEPLAY_CHALLENGE_TTL_SECONDS: i64 = 300;
const GAMEPLAY_SESSION_TTL_SECONDS: i64 = 86_400;
const ACCOUNT_RESOLVE_CHALLENGE_TTL_SECONDS: i64 = 300;
const BLACKJACK_OPEN_ADVISORY_LOCK_KEY: i64 = 0x4d4f_424a_4f50_454e;
const ORIGINALS_REVEAL_TIMEOUT_BLOCKS: u64 = 50;

#[derive(Clone)]
struct AppState {
    service: &'static str,
    infra: InfraSnapshot,
    database: Option<PgPool>,
    redis: Option<RedisClient>,
    chain: Option<ChainService>,
    admin_token: Option<String>,
    profile_claim_chain_id: &'static str,
    rewards_config: RewardsConfig,
    blackjack_verifier_url: Option<String>,
    http_client: reqwest::Client,
    blackjack_open_lock: Arc<tokio::sync::Mutex<()>>,
}

#[derive(Debug, Deserialize)]
struct CreateHandRequest {
    table_id: u64,
    player: String,
    wager: String,
    client_seed: Option<String>,
}

#[derive(Debug, Serialize)]
struct CreateHandResponse {
    hand_id: String,
    session_id: String,
    transcript_root: String,
    relay_token: String,
    phase: String,
    runtime_cached: bool,
}

#[derive(Debug, Serialize)]
struct TableStateResponse {
    state: BlackjackTableState,
    live_players: u64,
}

#[derive(Debug, Serialize)]
struct CreateDiceCommitmentResponse {
    commitment: DiceCommitmentView,
    tx_hash: String,
}

#[derive(Debug, Serialize)]
struct SettleDiceCommitmentResponse {
    round: DiceRoundView,
    tx_hash: String,
    server_seed: String,
}

#[derive(Debug, Serialize)]
struct SettleRouletteCommitmentResponse {
    spin: RouletteSpinView,
    tx_hash: String,
    server_seed: String,
}

#[derive(Debug, Serialize)]
struct SettleBaccaratCommitmentResponse {
    round: BaccaratRoundView,
    tx_hash: String,
    server_seed: String,
}

#[derive(Debug, Serialize, Clone)]
struct BetFeedItem {
    game: String,
    user: String,
    wallet_address: String,
    bet_amount: String,
    multiplier_bps: String,
    payout: String,
    tx_hash: Option<String>,
    settled_at: String,
}

#[derive(Debug, Serialize)]
struct BetFeedResponse {
    my_bets: Vec<BetFeedItem>,
    all_bets: Vec<BetFeedItem>,
    high_rollers: Vec<BetFeedItem>,
    race_leaderboard: Vec<BetFeedItem>,
}

#[derive(Debug, Deserialize)]
struct UpsertProfileRequest {
    wallet_address: String,
    username: Option<String>,
    auth_provider: Option<String>,
    challenge_id: String,
    signature: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct CreateProfileClaimChallengeRequest {
    wallet_address: String,
    username: Option<String>,
    auth_provider: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CreateGameplaySessionChallengeRequest {
    wallet_address: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct GameplaySessionChallengeState {
    wallet_address: String,
    challenge_id: String,
    expires_at_unix: i64,
}

#[derive(Debug, Serialize)]
struct GameplaySessionChallengeResponse {
    wallet_address: String,
    challenge_id: String,
    expires_at_unix: i64,
    typed_data: Value,
}

#[derive(Debug, Deserialize)]
struct CreateGameplaySessionRequest {
    wallet_address: String,
    challenge_id: String,
    signature: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GameplaySessionState {
    session_token: String,
    wallet_address: String,
    expires_at_unix: i64,
}

#[derive(Debug, Deserialize)]
struct OpenDiceRoundRequest {
    table_id: u64,
    player: String,
    wager: String,
    target_bps: u32,
    roll_over: bool,
    client_seed: String,
    commitment_id: u64,
}

#[derive(Debug, Deserialize)]
struct OpenRouletteSpinBetRequest {
    kind: u8,
    selection: u8,
    amount: String,
}

#[derive(Debug, Deserialize)]
struct OpenRouletteSpinRequest {
    table_id: u64,
    player: String,
    total_wager: String,
    client_seed: String,
    commitment_id: u64,
    bets: Vec<OpenRouletteSpinBetRequest>,
}

#[derive(Debug, Deserialize)]
struct OpenBaccaratRoundRequest {
    table_id: u64,
    player: String,
    wager: String,
    bet_side: u8,
    client_seed: String,
    commitment_id: u64,
}

#[derive(Debug, Serialize)]
struct OpenGameplayResponse {
    game_id: u64,
    tx_hash: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ProfileClaimChallengeState {
    wallet_address: String,
    username: Option<String>,
    auth_provider: String,
    challenge_id: String,
    expires_at_unix: i64,
}

#[derive(Debug, Serialize)]
struct ProfileClaimChallengeResponse {
    wallet_address: String,
    username: Option<String>,
    auth_provider: String,
    challenge_id: String,
    expires_at_unix: i64,
    typed_data: Value,
}

#[derive(Debug, Deserialize, Serialize)]
struct RewardsClaimChallengeState {
    wallet_address: String,
    reward_kind: String,
    claim_id: String,
    amount_raw: String,
    challenge_id: String,
    expires_at_unix: i64,
}

#[derive(Debug, Deserialize)]
struct CreateRewardsClaimChallengeRequest {
    user_id: Option<String>,
    wallet_address: Option<String>,
    reward_kind: String,
}

#[derive(Debug, Serialize)]
struct RewardsClaimChallengeResponse {
    wallet_address: String,
    reward_kind: String,
    claim_id: String,
    amount_raw: String,
    challenge_id: String,
    expires_at_unix: i64,
    typed_data: Value,
}

#[derive(Debug, Deserialize)]
struct ClaimRewardsRequest {
    user_id: Option<String>,
    wallet_address: Option<String>,
    reward_kind: String,
    claim_id: String,
    challenge_id: String,
    signature: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ClaimRewardsResponse {
    reward_kind: String,
    claim_id: String,
    amount_raw: String,
    tx_hash: String,
    claim_rows: usize,
    status: String,
}

#[derive(Debug, Deserialize)]
struct CreateRewardCouponRequest {
    code: Option<String>,
    description: Option<String>,
    amount_raw: String,
    max_global_redemptions: Option<i64>,
    max_per_user_redemptions: Option<i64>,
    starts_at_unix: Option<i64>,
    expires_at_unix: Option<i64>,
    active: Option<bool>,
    quantity: Option<usize>,
    created_by: Option<String>,
}

#[derive(Debug, Serialize)]
struct CreateRewardCouponResponse {
    coupons: Vec<rewards::RewardCouponRecord>,
}

#[derive(Debug, Deserialize)]
struct RedeemRewardCouponRequest {
    user_id: Option<String>,
    wallet_address: Option<String>,
    code: String,
}

#[derive(Debug, Serialize)]
struct RedeemRewardCouponResponse {
    redemption_id: String,
    coupon_id: String,
    code: String,
    amount_raw: String,
    tx_hash: String,
    status: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct ReferrerLinkChallengeState {
    wallet_address: String,
    referrer: String,
    challenge_id: String,
    expires_at_unix: i64,
}

#[derive(Debug, Deserialize, Serialize)]
struct AccountResolveChallengeState {
    wallet_address: String,
    linked_via: String,
    make_primary: bool,
    challenge_id: String,
    expires_at_unix: i64,
}

#[derive(Debug, Deserialize)]
struct CreateAccountResolveChallengeRequest {
    wallet_address: String,
    linked_via: Option<String>,
    make_primary: Option<bool>,
}

#[derive(Debug, Serialize)]
struct AccountResolveChallengeResponse {
    wallet_address: String,
    challenge_id: String,
    expires_at_unix: i64,
    typed_data: Value,
}

#[derive(Debug, Deserialize)]
struct CreateReferrerLinkChallengeRequest {
    user_id: Option<String>,
    wallet_address: Option<String>,
    referrer: String,
}

#[derive(Debug, Serialize)]
struct ReferrerLinkChallengeResponse {
    wallet_address: String,
    referrer: String,
    challenge_id: String,
    expires_at_unix: i64,
    typed_data: Value,
}

#[derive(Debug, Deserialize)]
struct SetReferrerRequest {
    user_id: Option<String>,
    wallet_address: Option<String>,
    referrer: String,
    challenge_id: String,
    signature: Vec<String>,
}

#[derive(Debug, Serialize)]
struct UsernameAvailabilityResponse {
    username: String,
    available: bool,
}

#[derive(Debug, Serialize)]
struct RootResponse {
    service: &'static str,
    role: &'static str,
    status: &'static str,
    infra: InfraSnapshot,
}

#[derive(Debug, Serialize)]
struct SessionPublicResponse {
    session: SessionPublicState,
}

#[derive(Debug, Serialize)]
struct SessionPublicState {
    session_id: String,
    hand_id: String,
    player: String,
    table_id: u64,
    transcript_root: String,
    status: String,
    phase: String,
    allowed_actions: Vec<String>,
    expires_at_unix: i64,
}

#[derive(Debug, Serialize)]
struct BlackjackHandPublicRecord {
    hand_id: String,
    player: String,
    table_id: u64,
    wager: String,
    status: String,
    phase: String,
    transcript_root: String,
    active_seat: u8,
    seat_count: u8,
    dealer_upcard: Option<u8>,
    chain_hand_id: Option<i64>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Deserialize)]
struct TableStateQuery {
    player: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BetFeedQuery {
    player: Option<String>,
    user_id: Option<String>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct EnsureAccountRequest {
    user_id: Option<String>,
    wallet_address: Option<String>,
    auth_provider: Option<String>,
    auth_subject: Option<String>,
    challenge_id: Option<String>,
    signature: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct EnsureVerifiedAccountRequest {
    wallet_address: Option<String>,
    auth_provider: Option<String>,
    auth_subject: Option<String>,
    linked_via: Option<String>,
    make_primary: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct BalanceTransferRequest {
    user_id: Option<String>,
    wallet_address: Option<String>,
    direction: String,
    amount: String,
}

#[derive(Debug, Deserialize)]
struct CreateWithdrawalRequest {
    user_id: Option<String>,
    wallet_address: Option<String>,
    amount: String,
    destination_address: String,
    source_balance: Option<String>,
    destination_chain_key: Option<String>,
    destination_asset_symbol: Option<String>,
    destination_tx_hash: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WithdrawalListQuery {
    limit: Option<i64>,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

type ApiResult<T> = Result<Json<T>, ApiError>;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    telemetry::init("moros_game_coordinator");
    let config = ServiceConfig::from_env("moros-game-coordinator", 8081);
    let infra = ServiceInfra::from_config(&config)?;
    let readiness = infra.prepare().await?;
    let state = Arc::new(AppState {
        service: "moros-game-coordinator",
        infra: infra.snapshot(&config, readiness),
        database: infra.database.clone(),
        redis: infra.redis.clone(),
        chain: ChainService::from_config(&config)?,
        admin_token: std::env::var("MOROS_ADMIN_TOKEN").ok(),
        profile_claim_chain_id: profile_claim_chain_id(&config.starknet_chain),
        rewards_config: RewardsConfig::from_env()?,
        blackjack_verifier_url: std::env::var("MOROS_BLACKJACK_VERIFIER_URL").ok(),
        http_client: reqwest::Client::new(),
        blackjack_open_lock: Arc::new(tokio::sync::Mutex::new(())),
    });

    let app = base_router::<Arc<AppState>>("moros-game-coordinator")
        .route("/", get(root))
        .route("/v1/tables/{table_id}/state", get(get_table_state))
        .route(
            "/v1/gameplay/sessions/challenge",
            post(create_gameplay_session_challenge),
        )
        .route("/v1/gameplay/sessions", post(create_gameplay_session))
        .route("/v1/dice/commitments", post(create_dice_commitment))
        .route("/v1/dice/rounds", post(open_dice_round))
        .route(
            "/v1/dice/commitments/{commitment_id}/settle",
            post(settle_dice_commitment),
        )
        .route("/v1/dice/rounds/{round_id}", get(get_dice_round))
        .route("/v1/roulette/commitments", post(create_roulette_commitment))
        .route("/v1/roulette/spins", post(open_roulette_spin))
        .route(
            "/v1/roulette/commitments/{commitment_id}/settle",
            post(settle_roulette_commitment),
        )
        .route("/v1/roulette/spins/{spin_id}", get(get_roulette_spin))
        .route("/v1/baccarat/commitments", post(create_baccarat_commitment))
        .route("/v1/baccarat/rounds", post(open_baccarat_round))
        .route(
            "/v1/baccarat/commitments/{commitment_id}/settle",
            post(settle_baccarat_commitment),
        )
        .route("/v1/baccarat/rounds/{round_id}", get(get_baccarat_round))
        .route("/v1/bets", get(get_bet_feed))
        .route(
            "/v1/accounts/resolve/challenge",
            post(create_account_resolve_challenge),
        )
        .route("/v1/accounts/resolve", post(resolve_account))
        .route(
            "/v1/accounts/resolve/verified",
            post(resolve_verified_account),
        )
        .route(
            "/v1/accounts/users/{user_id}/balances",
            get(get_account_balances_by_user),
        )
        .route(
            "/v1/accounts/wallets/{wallet_address}/balances",
            get(get_account_balances_by_wallet),
        )
        .route("/v1/accounts/transfers", post(transfer_account_balances))
        .route("/v1/accounts/withdrawals", post(create_account_withdrawal))
        .route(
            "/v1/accounts/users/{user_id}/withdrawals",
            get(list_account_withdrawals_by_user),
        )
        .route(
            "/v1/accounts/users/{user_id}/rewards",
            get(get_rewards_state_by_user),
        )
        .route(
            "/v1/auth/usernames/{username}/availability",
            get(get_username_availability),
        )
        .route(
            "/v1/auth/profiles/challenge",
            post(create_profile_claim_challenge),
        )
        .route("/v1/auth/profiles", post(upsert_profile))
        .route(
            "/v1/auth/profiles/{wallet_address}",
            get(get_profile_by_wallet),
        )
        .route(
            "/v1/auth/profiles/by-username/{username}",
            get(get_profile_by_username),
        )
        .route("/v1/rewards/{wallet_address}", get(get_rewards_state))
        .route(
            "/v1/rewards/claims/challenge",
            post(create_rewards_claim_challenge),
        )
        .route("/v1/rewards/claims", post(claim_rewards))
        .route("/v1/admin/rewards/coupons", post(create_reward_coupons))
        .route("/v1/rewards/coupons/redeem", post(redeem_reward_coupon))
        .route(
            "/v1/rewards/referrer/challenge",
            post(create_referrer_link_challenge),
        )
        .route("/v1/rewards/referrer", post(set_referrer))
        .route("/v1/hands", post(create_hand))
        .route("/v1/hands/{hand_id}", get(get_hand))
        .route("/v1/hands/{hand_id}/view", get(get_hand_view))
        .route("/v1/hands/{hand_id}/fairness", get(get_hand_fairness))
        .route("/v1/sessions/{session_id}", get(get_session))
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(config.bind_address()).await?;
    tracing::info!(
        "{} listening on {}",
        config.service_name,
        listener.local_addr()?
    );
    axum::serve(listener, app).await?;
    Ok(())
}

async fn root(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(serde_json::json!(RootResponse {
        service: state.service,
        role: "committed transcript coordinator",
        status: "ready",
        infra: state.infra.clone(),
    }))
}

async fn create_hand(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<CreateHandRequest>,
) -> ApiResult<CreateHandResponse> {
    let database = state
        .database
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("database is not configured"))?;
    let redis = state
        .redis
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("redis is not configured"))?;
    let chain = state
        .chain
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("starknet chain is not configured"))?;
    let gameplay_session = require_gameplay_session(redis, &headers).await?;
    let player = normalize_wallet_address(&payload.player)?;
    if !addresses_match(&gameplay_session.wallet_address, &player) {
        return Err(ApiError::bad_request(
            "gameplay session does not match player",
        ));
    }

    let expires_at_unix = (runtime::now_unix() + 900).min(gameplay_session.expires_at_unix);
    let client_seed = payload
        .client_seed
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let server_seed = Felt::from(Uuid::new_v4().as_u128());
    let server_seed_hex = format!("{server_seed:#x}");
    let server_seed_hash = chain.hash_server_seed_commitment(server_seed);
    let server_seed_hash_hex = format!("{server_seed_hash:#x}");
    let relay_token = Uuid::now_v7().simple().to_string();
    let hand_id = Uuid::now_v7().to_string();
    let session_id = Uuid::now_v7().to_string();
    let preview_snapshot = blackjack::seed_hand_snapshot_with_secret(
        &hand_id,
        &player,
        payload.table_id,
        &payload.wager,
        &server_seed_hash_hex,
        &server_seed_hash_hex,
        &server_seed_hex,
        client_seed.as_deref(),
    )
    .map_err(ApiError::internal)?;
    let transcript_root = preview_snapshot.transcript_root.clone();
    let live_state = chain
        .fetch_blackjack_table_state(payload.table_id, Some(&player))
        .await
        .map_err(ApiError::internal)?;

    if live_state.table.status != "active" {
        return Err(ApiError::bad_request(format!(
            "table {} is not active: {}",
            payload.table_id, live_state.table.status
        )));
    }

    let wager = payload
        .wager
        .parse::<u128>()
        .map_err(|error| ApiError::bad_request(format!("invalid wager: {error}")))?;
    let min_wager = live_state
        .table
        .min_wager
        .parse::<u128>()
        .map_err(ApiError::internal)?;
    let max_wager = live_state
        .table
        .max_wager
        .parse::<u128>()
        .map_err(ApiError::internal)?;
    if wager < min_wager || wager > max_wager {
        return Err(ApiError::bad_request(format!(
            "wager must be between {} and {} wei",
            min_wager, max_wager
        )));
    }
    let player_balance = live_state
        .player_balance
        .as_deref()
        .unwrap_or("0")
        .parse::<u128>()
        .map_err(ApiError::internal)?;
    if player_balance < wager {
        return Err(ApiError::bad_request(format!(
            "player bankroll is too low for this hand: have {} wei, need {} wei",
            player_balance, wager
        )));
    }
    let dealer_upcard = preview_snapshot
        .dealer
        .cards
        .first()
        .map(|card| card.rank)
        .unwrap_or_default();
    let exposure_factor = if dealer_upcard == 1 { 9_u128 } else { 8_u128 };
    let worst_case_house_exposure = wager.saturating_mul(exposure_factor);
    let dynamic_house_cap = live_state
        .house_available
        .parse::<u128>()
        .map_err(ApiError::internal)?
        / 100;
    if worst_case_house_exposure > dynamic_house_cap {
        return Err(ApiError::bad_request(format!(
            "house liquidity cap is too low to open this hand: cap {} wei, need {} wei",
            dynamic_house_cap, worst_case_house_exposure
        )));
    }
    let immediate_house_exposure = if preview_snapshot.seats[0].status == "blackjack" {
        (wager * 3) / 2
    } else {
        wager
    };
    let house_available = live_state
        .house_available
        .parse::<u128>()
        .map_err(ApiError::internal)?;
    if house_available < immediate_house_exposure {
        return Err(ApiError::bad_request(format!(
            "house liquidity is too low to open this hand: have {} wei, need {} wei",
            house_available, immediate_house_exposure
        )));
    }
    ensure_operator_gameplay_session(
        chain,
        &player,
        gameplay_session.expires_at_unix,
        &live_state,
    )
    .await
    .map_err(ApiError::internal)?;

    let record = persistence::create_blackjack_hand(
        database,
        CreateHandInput {
            hand_id: Some(hand_id),
            session_id: Some(session_id),
            table_id: payload.table_id,
            player: player.clone(),
            wager: payload.wager.clone(),
            transcript_root: transcript_root.clone(),
            relay_token: relay_token.clone(),
            expires_at_unix,
        },
    )
    .await
    .map_err(ApiError::internal)?;
    let _ = persistence::seed_blackjack_view(
        database,
        &record,
        &server_seed_hash_hex,
        &server_seed_hex,
        client_seed.as_deref(),
    )
    .await
    .map_err(ApiError::internal)?;
    let hand_uuid = Uuid::parse_str(&record.hand_id)
        .map_err(|error| ApiError::internal(anyhow::anyhow!("invalid hand_id: {error}")))?;
    let mut snapshot = persistence::get_blackjack_snapshot(database, hand_uuid)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("hand snapshot not found"))?;
    let opening_plan = blackjack::opening_plan(&snapshot).map_err(ApiError::internal)?;
    let wager = record.wager.parse::<u128>().map_err(ApiError::internal)?;
    let blackjack_open_guard = state.blackjack_open_lock.lock().await;
    let mut blackjack_open_tx = database.begin().await.map_err(ApiError::internal)?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(BLACKJACK_OPEN_ADVISORY_LOCK_KEY)
        .execute(&mut *blackjack_open_tx)
        .await
        .map_err(ApiError::internal)?;
    let expected_chain_hand_id = chain
        .peek_next_hand_id()
        .await
        .map_err(ApiError::internal)?;
    let dealer_peek_proof = match maybe_fetch_blackjack_peek_proof(
        &state,
        database,
        hand_uuid,
        &mut snapshot,
        expected_chain_hand_id,
        wager,
        &opening_plan,
    )
    .await
    {
        Ok(proof) => proof,
        Err(error) => {
            let cleanup_error = persistence::delete_blackjack_hand(database, hand_uuid)
                .await
                .err();
            if let Some(cleanup_error) = cleanup_error {
                tracing::error!(error = ?cleanup_error, hand_id = %record.hand_id, "failed to delete hand after blackjack dealer-peek verifier failure");
            }
            return Err(ApiError::internal(error));
        }
    };
    persistence::append_blackjack_event(
        database,
        hand_uuid,
        record
            .session_id
            .as_deref()
            .map(Uuid::parse_str)
            .transpose()
            .map_err(|error| ApiError::internal(anyhow::anyhow!("invalid session_id: {error}")))?,
        "fairness.prepared",
        serde_json::json!({
            "protocol_mode": snapshot.transcript_artifact.protocol_mode,
            "target_protocol_mode": snapshot.transcript_artifact.target_protocol_mode,
            "encryption_scheme": snapshot.transcript_artifact.encryption_scheme,
            "target_encryption_scheme": snapshot.transcript_artifact.target_encryption_scheme,
            "deck_commitment_root": snapshot.transcript_artifact.deck_commitment_root,
            "encrypted_deck_root": snapshot.transcript_artifact.encrypted_deck_root,
            "dealer_entropy_commitment": snapshot.transcript_artifact.dealer_entropy_commitment,
            "player_entropy_commitment": snapshot.transcript_artifact.player_entropy_commitment,
            "shuffle_commitment": snapshot.transcript_artifact.shuffle_commitment,
            "ruleset_hash": snapshot.transcript_artifact.ruleset_hash,
            "dealer_peek_required": snapshot.transcript_artifact.dealer_peek.required,
            "dealer_peek_proof_mode": snapshot.transcript_artifact.dealer_peek.proof_mode,
            "dealer_peek_target_proof_mode": snapshot.transcript_artifact.dealer_peek.target_proof_mode,
            "dealer_peek_target_proof_kind": snapshot.transcript_artifact.dealer_peek.target_proof_kind,
            "dealer_peek_statement_kind": snapshot.transcript_artifact.dealer_peek.statement_kind,
            "dealer_peek_public_inputs_hash": snapshot.transcript_artifact.dealer_peek.public_inputs_hash,
            "dealer_peek_no_blackjack_proof_available": snapshot.transcript_artifact.dealer_peek.no_blackjack_proof.available,
            "dealer_peek_no_blackjack_proof_verifier_status": snapshot.transcript_artifact.dealer_peek.no_blackjack_proof.verifier_status,
            "dealer_peek_no_blackjack_proof_verifier_namespace": snapshot.transcript_artifact.dealer_peek.no_blackjack_proof.verifier_namespace,
            "dealer_peek_no_blackjack_proof_statement_hash": snapshot.transcript_artifact.dealer_peek.no_blackjack_proof.statement_hash,
            "dealer_peek_no_blackjack_proof_zk_request_id": snapshot.transcript_artifact.dealer_peek.no_blackjack_proof.zk_proof_target.request_id,
            "dealer_peek_no_blackjack_proof_zk_circuit_id": snapshot.transcript_artifact.dealer_peek.no_blackjack_proof.zk_proof_target.circuit_id,
            "dealer_peek_no_blackjack_proof_zk_verification_key_id": snapshot.transcript_artifact.dealer_peek.no_blackjack_proof.zk_proof_target.verification_key_id,
            "hole_card_index": snapshot.transcript_artifact.hole_card_index,
        }),
    )
    .await
    .map_err(ApiError::internal)?;
    let open_result = chain
        .open_hand_verified(
            expected_chain_hand_id,
            &record.player,
            record.table_id,
            wager,
            &record.transcript_root,
            snapshot.transcript_artifact.dealer_peek.required,
            snapshot.transcript_artifact.dealer_peek.outcome == "dealer_blackjack",
            opening_plan.dealer_upcard,
            &opening_plan.dealer_upcard_proof,
            opening_plan.player_first_card,
            &opening_plan.player_first_card_proof,
            opening_plan.player_second_card,
            &opening_plan.player_second_card_proof,
            &dealer_peek_proof,
        )
        .await;
    let (chain_hand_id, _) = match open_result {
        Ok(result) => result,
        Err(error) => {
            let cleanup_error = persistence::delete_blackjack_hand(database, hand_uuid)
                .await
                .err();
            if let Some(cleanup_error) = cleanup_error {
                tracing::error!(error = ?cleanup_error, hand_id = %hand_uuid, "failed to delete hand after open_hand failure");
            }
            return Err(ApiError::internal(error));
        }
    };
    blackjack_open_tx
        .commit()
        .await
        .map_err(ApiError::internal)?;
    drop(blackjack_open_guard);
    persistence::set_chain_hand_id(
        database,
        hand_uuid,
        i64::try_from(chain_hand_id).map_err(ApiError::internal)?,
    )
    .await
    .map_err(ApiError::internal)?;

    if opening_plan.should_finalize {
        for (index, card) in opening_plan.dealer_reveals.iter().enumerate() {
            let proof = opening_plan
                .dealer_reveal_proofs
                .get(index)
                .ok_or_else(|| {
                    ApiError::internal(anyhow::anyhow!("dealer reveal proof missing"))
                })?;
            chain
                .reveal_dealer_card_verified(chain_hand_id, *card, proof)
                .await
                .map_err(ApiError::internal)?;
            chain
                .wait_for_hand_state(
                    chain_hand_id,
                    "awaiting_dealer",
                    "dealer_turn",
                    snapshot.action_count,
                    snapshot.seat_count,
                    index + 2,
                )
                .await
                .map_err(ApiError::internal)?;
        }
        chain
            .finalize_hand(chain_hand_id)
            .await
            .map_err(ApiError::internal)?;
    }

    let chain_hand = chain
        .wait_for_hand_state(
            chain_hand_id,
            &snapshot.status,
            &snapshot.phase,
            snapshot.action_count,
            snapshot.seat_count,
            if opening_plan.should_finalize {
                opening_plan.dealer_reveals.len() + 1
            } else {
                1
            },
        )
        .await
        .map_err(ApiError::internal)?;
    let view =
        blackjack::reconcile_view_with_chain(&snapshot, &chain_hand).map_err(ApiError::internal)?;

    let mut runtime_cached = false;
    if let Some(redis) = &state.redis {
        if let Some(session_id) = &record.session_id {
            runtime::cache_session(
                redis,
                &SessionRuntimeState {
                    session_id: session_id.clone(),
                    hand_id: record.hand_id.clone(),
                    player: record.player.clone(),
                    relay_token: relay_token.clone(),
                    table_id: record.table_id,
                    transcript_root: record.transcript_root.clone(),
                    status: view.status.clone(),
                    phase: view.phase.clone(),
                    allowed_actions: view.allowed_actions.clone(),
                    expires_at_unix,
                },
            )
            .await
            .map_err(ApiError::internal)?;
            runtime_cached = true;
        }
    }

    Ok(Json(CreateHandResponse {
        hand_id: record.hand_id,
        session_id: record.session_id.unwrap_or_default(),
        transcript_root,
        relay_token,
        phase: view.phase,
        runtime_cached,
    }))
}

async fn maybe_fetch_blackjack_peek_proof(
    state: &Arc<AppState>,
    database: &PgPool,
    hand_id: Uuid,
    snapshot: &mut blackjack::BlackjackHandSnapshot,
    chain_hand_id: u64,
    wager: u128,
    opening_plan: &blackjack::BlackjackOpenPlan,
) -> anyhow::Result<Vec<String>> {
    let Some(verifier_url) = state.blackjack_verifier_url.as_deref() else {
        if snapshot.transcript_artifact.dealer_peek.required {
            bail!("MOROS_BLACKJACK_VERIFIER_URL is required for blackjack dealer peek proofs");
        }
        return Ok(Vec::new());
    };
    let Some(mut request) = blackjack::build_no_blackjack_zk_proof_request_with_witness(snapshot)
    else {
        if snapshot.transcript_artifact.dealer_peek.required {
            bail!("blackjack dealer peek proof request could not be constructed");
        }
        return Ok(Vec::new());
    };
    request.onchain_context = Some(blackjack::BlackjackOnchainPeekContext {
        chain_hand_id,
        table_id: snapshot.table_id,
        player: snapshot.player.clone(),
        wager: wager.to_string(),
        transcript_root: snapshot.transcript_root.clone(),
        dealer_upcard: opening_plan.dealer_upcard,
        player_first_card: opening_plan.player_first_card,
        player_second_card: opening_plan.player_second_card,
        dealer_blackjack: snapshot.transcript_artifact.dealer_peek.outcome == "dealer_blackjack",
    });

    let endpoint = blackjack_verifier_endpoint(verifier_url)?;
    let response = state
        .http_client
        .post(&endpoint)
        .json(&request)
        .send()
        .await
        .with_context(|| format!("failed to reach blackjack verifier at {endpoint}"))?
        .error_for_status()
        .context("blackjack verifier rejected dealer peek proof request")?;
    let payload = response
        .json::<blackjack::BlackjackZkPeekProofResponse>()
        .await
        .context("failed to decode blackjack verifier response")?;
    anyhow::ensure!(
        !payload.proof.proof.garaga_calldata.is_empty(),
        "blackjack verifier returned an empty dealer peek proof"
    );
    persistence::store_blackjack_snapshot(database, hand_id, snapshot).await?;
    persistence::append_blackjack_event(
        database,
        hand_id,
        None,
        "fairness.peek_proof_ready",
        serde_json::json!({
            "protocol_mode": snapshot.transcript_artifact.protocol_mode,
            "target_protocol_mode": snapshot.transcript_artifact.target_protocol_mode,
            "dealer_peek_statement_hash": snapshot.transcript_artifact.dealer_peek.no_blackjack_proof.statement_hash,
            "dealer_peek_zk_circuit_id": request.target.circuit_id,
            "dealer_peek_payload_schema_version": payload.proof.schema_version,
            "dealer_peek_proof_encoding": payload.proof.proof_encoding,
            "dealer_peek_proof_bytes_hash": payload.proof.proof_bytes_hash,
            "dealer_peek_proof_transcript_hash": payload.proof.proof_transcript_hash,
            "dealer_peek_vk_hash": payload.proof.verification_key_hash,
            "dealer_peek_public_input_count": payload.proof.proof.public_inputs.len(),
            "dealer_peek_garaga_calldata_len": payload.proof.proof.garaga_calldata.len(),
        }),
    )
    .await?;
    Ok(payload.proof.proof.garaga_calldata)
}

fn blackjack_verifier_endpoint(verifier_url: &str) -> anyhow::Result<String> {
    const VERIFIER_ROUTE: &str = "/v1/blackjack/dealer-peek/prove";
    const PROVER_ROUTE: &str = "/v1/blackjack/proofs/no-blackjack-peek";

    let mut normalized = verifier_url.trim().trim_end_matches('/').to_string();
    anyhow::ensure!(
        !normalized.is_empty(),
        "MOROS_BLACKJACK_VERIFIER_URL cannot be empty"
    );

    if normalized.ends_with(PROVER_ROUTE) {
        bail!(
            "MOROS_BLACKJACK_VERIFIER_URL must point to the blackjack verifier service, not the prover endpoint"
        );
    }

    if normalized.ends_with(VERIFIER_ROUTE) {
        return Ok(normalized);
    }

    normalized.push_str(VERIFIER_ROUTE);
    Ok(normalized)
}

async fn get_table_state(
    State(state): State<Arc<AppState>>,
    Path(table_id): Path<u64>,
    Query(query): Query<TableStateQuery>,
) -> ApiResult<TableStateResponse> {
    let database = state
        .database
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("database is not configured"))?;
    let chain = state
        .chain
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("starknet chain is not configured"))?;
    let live_state =
        fetch_table_state_with_internal_balance(database, chain, table_id, query.player.as_deref())
            .await
            .map_err(ApiError::internal)?;
    let live_players =
        persistence::count_live_activity_for_game(database, &live_state.table.game_kind, table_id)
            .await
            .map_err(ApiError::internal)?;
    Ok(Json(TableStateResponse {
        state: live_state,
        live_players,
    }))
}

async fn create_gameplay_session_challenge(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateGameplaySessionChallengeRequest>,
) -> ApiResult<GameplaySessionChallengeResponse> {
    let redis = state
        .redis
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("redis is not configured"))?;
    let wallet_address = normalize_wallet_address(&payload.wallet_address)?;
    let challenge = GameplaySessionChallengeState {
        wallet_address: wallet_address.clone(),
        challenge_id: felt_to_hex(Felt::from(Uuid::new_v4().as_u128())),
        expires_at_unix: runtime::now_unix() + GAMEPLAY_CHALLENGE_TTL_SECONDS,
    };
    cache_gameplay_session_challenge(redis, &challenge)
        .await
        .map_err(ApiError::internal)?;

    Ok(Json(GameplaySessionChallengeResponse {
        wallet_address: challenge.wallet_address.clone(),
        challenge_id: challenge.challenge_id.clone(),
        expires_at_unix: challenge.expires_at_unix,
        typed_data: build_gameplay_session_typed_data_value(
            state.profile_claim_chain_id,
            &challenge,
        )
        .map_err(ApiError::internal)?,
    }))
}

async fn create_gameplay_session(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateGameplaySessionRequest>,
) -> ApiResult<GameplaySessionState> {
    let redis = state
        .redis
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("redis is not configured"))?;
    let chain = state
        .chain
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("starknet chain is not configured"))?;
    let wallet_address = normalize_wallet_address(&payload.wallet_address)?;
    let challenge = get_gameplay_session_challenge(redis, &payload.challenge_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::bad_request("gameplay session challenge is invalid or expired"))?;
    if challenge.expires_at_unix < runtime::now_unix() {
        delete_gameplay_session_challenge(redis, &challenge.challenge_id)
            .await
            .map_err(ApiError::internal)?;
        return Err(ApiError::bad_request(
            "gameplay session challenge is invalid or expired",
        ));
    }
    if !addresses_match(&challenge.wallet_address, &wallet_address) {
        return Err(ApiError::bad_request(
            "gameplay session challenge does not match wallet",
        ));
    }

    let typed_data = build_gameplay_session_typed_data(state.profile_claim_chain_id, &challenge)
        .map_err(ApiError::internal)?;
    let signature = parse_signature(&payload.signature)?;
    let wallet_felt = felt_from_dec_or_hex(&wallet_address).map_err(ApiError::internal)?;
    let message_hash = typed_data
        .message_hash(wallet_felt)
        .map_err(ApiError::internal)?;
    let signature_valid = chain
        .verify_message_signature(wallet_felt, message_hash, &signature)
        .await
        .map_err(signature_verification_error)?;
    if !signature_valid {
        return Err(ApiError::bad_request(
            "gameplay session signature is invalid",
        ));
    }

    delete_gameplay_session_challenge(redis, &challenge.challenge_id)
        .await
        .map_err(ApiError::internal)?;
    let session = GameplaySessionState {
        session_token: format!("{}{}", Uuid::now_v7().simple(), Uuid::new_v4().simple()),
        wallet_address,
        expires_at_unix: runtime::now_unix() + GAMEPLAY_SESSION_TTL_SECONDS,
    };
    cache_gameplay_session(redis, &session)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(session))
}

async fn open_dice_round(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<OpenDiceRoundRequest>,
) -> ApiResult<OpenGameplayResponse> {
    let database = state
        .database
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("database is not configured"))?;
    let redis = state
        .redis
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("redis is not configured"))?;
    let chain = state
        .chain
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("starknet chain is not configured"))?;
    let session = require_gameplay_session(redis, &headers).await?;
    let player = normalize_wallet_address(&payload.player)?;
    if !addresses_match(&session.wallet_address, &player) {
        return Err(ApiError::bad_request(
            "gameplay session does not match player",
        ));
    }
    let live_state =
        fetch_table_state_with_internal_balance(database, chain, payload.table_id, Some(&player))
            .await
            .map_err(ApiError::internal)?;
    ensure_table_is_bettable(&live_state, &payload.wager)?;
    ensure_player_bankroll(&live_state, &payload.wager)?;
    let wager = parse_wager_u128(&payload.wager, "wager")?;
    if let Err(error) =
        ensure_operator_gameplay_session(chain, &player, session.expires_at_unix, &live_state).await
    {
        return Err(ApiError::internal(error));
    }

    let client_seed = felt_from_dec_or_hex(&payload.client_seed).map_err(ApiError::internal)?;
    let open = chain
        .open_dice_round(
            &player,
            &chain.operator_address_hex(),
            payload.table_id,
            wager,
            payload.target_bps,
            payload.roll_over,
            client_seed,
            payload.commitment_id,
        )
        .await;
    let (round_id, tx_hash) = match open {
        Ok(result) => result,
        Err(error) => return Err(ApiError::internal(error)),
    };
    Ok(Json(OpenGameplayResponse {
        game_id: round_id,
        tx_hash,
    }))
}

async fn open_roulette_spin(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<OpenRouletteSpinRequest>,
) -> ApiResult<OpenGameplayResponse> {
    let database = state
        .database
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("database is not configured"))?;
    let redis = state
        .redis
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("redis is not configured"))?;
    let chain = state
        .chain
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("starknet chain is not configured"))?;
    let session = require_gameplay_session(redis, &headers).await?;
    let player = normalize_wallet_address(&payload.player)?;
    if !addresses_match(&session.wallet_address, &player) {
        return Err(ApiError::bad_request(
            "gameplay session does not match player",
        ));
    }
    if payload.bets.is_empty() || payload.bets.len() > 8 {
        return Err(ApiError::bad_request("roulette supports 1-8 bets per spin"));
    }

    let live_state =
        fetch_table_state_with_internal_balance(database, chain, payload.table_id, Some(&player))
            .await
            .map_err(ApiError::internal)?;
    ensure_table_is_bettable(&live_state, &payload.total_wager)?;
    ensure_player_bankroll(&live_state, &payload.total_wager)?;
    if let Err(error) =
        ensure_operator_gameplay_session(chain, &player, session.expires_at_unix, &live_state).await
    {
        return Err(ApiError::internal(error));
    }
    let total_wager = parse_wager_u128(&payload.total_wager, "total_wager")?;
    let client_seed = felt_from_dec_or_hex(&payload.client_seed).map_err(ApiError::internal)?;
    let bets = payload
        .bets
        .iter()
        .map(|bet| {
            Ok((
                bet.kind,
                bet.selection,
                parse_wager_u128(&bet.amount, "bet amount")?,
            ))
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    let open = chain
        .open_roulette_spin(
            &player,
            &chain.operator_address_hex(),
            payload.table_id,
            total_wager,
            client_seed,
            payload.commitment_id,
            bets,
        )
        .await;
    let (spin_id, tx_hash) = match open {
        Ok(result) => result,
        Err(error) => return Err(ApiError::internal(error)),
    };
    Ok(Json(OpenGameplayResponse {
        game_id: spin_id,
        tx_hash,
    }))
}

async fn open_baccarat_round(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<OpenBaccaratRoundRequest>,
) -> ApiResult<OpenGameplayResponse> {
    let database = state
        .database
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("database is not configured"))?;
    let redis = state
        .redis
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("redis is not configured"))?;
    let chain = state
        .chain
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("starknet chain is not configured"))?;
    let session = require_gameplay_session(redis, &headers).await?;
    let player = normalize_wallet_address(&payload.player)?;
    if !addresses_match(&session.wallet_address, &player) {
        return Err(ApiError::bad_request(
            "gameplay session does not match player",
        ));
    }
    let live_state =
        fetch_table_state_with_internal_balance(database, chain, payload.table_id, Some(&player))
            .await
            .map_err(ApiError::internal)?;
    ensure_table_is_bettable(&live_state, &payload.wager)?;
    ensure_player_bankroll(&live_state, &payload.wager)?;
    if let Err(error) =
        ensure_operator_gameplay_session(chain, &player, session.expires_at_unix, &live_state).await
    {
        return Err(ApiError::internal(error));
    }
    let wager = parse_wager_u128(&payload.wager, "wager")?;
    let client_seed = felt_from_dec_or_hex(&payload.client_seed).map_err(ApiError::internal)?;
    let open = chain
        .open_baccarat_round(
            &player,
            &chain.operator_address_hex(),
            payload.table_id,
            wager,
            payload.bet_side,
            client_seed,
            payload.commitment_id,
        )
        .await;
    let (round_id, tx_hash) = match open {
        Ok(result) => result,
        Err(error) => return Err(ApiError::internal(error)),
    };
    Ok(Json(OpenGameplayResponse {
        game_id: round_id,
        tx_hash,
    }))
}

async fn create_dice_commitment(
    State(state): State<Arc<AppState>>,
    Json(_payload): Json<serde_json::Value>,
) -> ApiResult<CreateDiceCommitmentResponse> {
    let database = state
        .database
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("database is not configured"))?;
    let chain = state
        .chain
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("starknet chain is not configured"))?;

    let server_seed = Felt::from(Uuid::new_v4().as_u128());
    let server_seed_hash = chain.hash_server_seed_commitment(server_seed);
    let reveal_deadline = chain
        .fetch_latest_block_number()
        .await
        .map_err(ApiError::internal)?
        .checked_add(ORIGINALS_REVEAL_TIMEOUT_BLOCKS)
        .ok_or_else(|| ApiError::internal(anyhow::anyhow!("invalid reveal deadline")))?;
    let (commitment_id, tx_hash) = chain
        .commit_dice_server_seed(server_seed_hash, reveal_deadline)
        .await
        .map_err(ApiError::internal)?;
    let commitment = chain
        .fetch_dice_commitment(commitment_id)
        .await
        .map_err(ApiError::internal)?;
    persistence::create_dice_server_commitment(
        database,
        commitment_id,
        &format!("{server_seed:#x}"),
        &format!("{server_seed_hash:#x}"),
        reveal_deadline,
        &tx_hash,
    )
    .await
    .map_err(ApiError::internal)?;
    Ok(Json(CreateDiceCommitmentResponse {
        commitment,
        tx_hash,
    }))
}

async fn settle_dice_commitment(
    State(state): State<Arc<AppState>>,
    Path(commitment_id): Path<u64>,
) -> ApiResult<SettleDiceCommitmentResponse> {
    let database = state
        .database
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("database is not configured"))?;
    let chain = state
        .chain
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("starknet chain is not configured"))?;
    let commitment = persistence::get_dice_server_commitment(database, commitment_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("dice commitment not found"))?;
    if commitment.status == "settled" {
        return Err(ApiError::bad_request("dice commitment is already settled"));
    }
    let round_id = chain
        .fetch_dice_round_for_commitment(commitment_id)
        .await
        .map_err(ApiError::internal)?;
    if round_id == 0 {
        return Err(ApiError::bad_request(
            "dice commitment has not been used by a round",
        ));
    }
    let server_seed = felt_from_dec_or_hex(&commitment.server_seed)
        .map_err(|error| ApiError::bad_request(format!("invalid stored server seed: {error}")))?;
    let (round, tx_hash) = chain
        .settle_dice_round(round_id, server_seed)
        .await
        .map_err(ApiError::internal)?;
    persistence::mark_dice_commitment_settled(database, commitment_id, round_id, &tx_hash)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(SettleDiceCommitmentResponse {
        round,
        tx_hash,
        server_seed: commitment.server_seed,
    }))
}

async fn get_dice_round(
    State(state): State<Arc<AppState>>,
    Path(round_id): Path<u64>,
) -> ApiResult<DiceRoundView> {
    let chain = state
        .chain
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("starknet chain is not configured"))?;
    let round = chain
        .fetch_dice_round(round_id)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(round))
}

async fn create_roulette_commitment(
    State(state): State<Arc<AppState>>,
    Json(_payload): Json<serde_json::Value>,
) -> ApiResult<CreateDiceCommitmentResponse> {
    let database = state
        .database
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("database is not configured"))?;
    let chain = state
        .chain
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("starknet chain is not configured"))?;

    let server_seed = Felt::from(Uuid::new_v4().as_u128());
    let server_seed_hash = chain.hash_server_seed_commitment(server_seed);
    let reveal_deadline = chain
        .fetch_latest_block_number()
        .await
        .map_err(ApiError::internal)?
        .checked_add(ORIGINALS_REVEAL_TIMEOUT_BLOCKS)
        .ok_or_else(|| ApiError::internal(anyhow::anyhow!("invalid reveal deadline")))?;
    let (commitment_id, tx_hash) = chain
        .commit_roulette_server_seed(server_seed_hash, reveal_deadline)
        .await
        .map_err(ApiError::internal)?;
    let commitment = chain
        .fetch_roulette_commitment(commitment_id)
        .await
        .map_err(ApiError::internal)?;
    persistence::create_roulette_server_commitment(
        database,
        commitment_id,
        &format!("{server_seed:#x}"),
        &format!("{server_seed_hash:#x}"),
        reveal_deadline,
        &tx_hash,
    )
    .await
    .map_err(ApiError::internal)?;
    Ok(Json(CreateDiceCommitmentResponse {
        commitment,
        tx_hash,
    }))
}

async fn settle_roulette_commitment(
    State(state): State<Arc<AppState>>,
    Path(commitment_id): Path<u64>,
) -> ApiResult<SettleRouletteCommitmentResponse> {
    let database = state
        .database
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("database is not configured"))?;
    let chain = state
        .chain
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("starknet chain is not configured"))?;
    let commitment = persistence::get_roulette_server_commitment(database, commitment_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("roulette commitment not found"))?;
    if commitment.status == "settled" {
        return Err(ApiError::bad_request(
            "roulette commitment is already settled",
        ));
    }
    let spin_id = chain
        .fetch_roulette_spin_for_commitment(commitment_id)
        .await
        .map_err(ApiError::internal)?;
    if spin_id == 0 {
        return Err(ApiError::bad_request(
            "roulette commitment has not been used by a spin",
        ));
    }
    let server_seed = felt_from_dec_or_hex(&commitment.server_seed)
        .map_err(|error| ApiError::bad_request(format!("invalid stored server seed: {error}")))?;
    let (spin, tx_hash) = chain
        .settle_roulette_spin(spin_id, server_seed)
        .await
        .map_err(ApiError::internal)?;
    persistence::mark_roulette_commitment_settled(database, commitment_id, spin_id, &tx_hash)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(SettleRouletteCommitmentResponse {
        spin,
        tx_hash,
        server_seed: commitment.server_seed,
    }))
}

async fn get_roulette_spin(
    State(state): State<Arc<AppState>>,
    Path(spin_id): Path<u64>,
) -> ApiResult<RouletteSpinView> {
    let chain = state
        .chain
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("starknet chain is not configured"))?;
    let spin = chain
        .fetch_roulette_spin(spin_id)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(spin))
}

async fn create_baccarat_commitment(
    State(state): State<Arc<AppState>>,
    Json(_payload): Json<serde_json::Value>,
) -> ApiResult<CreateDiceCommitmentResponse> {
    let database = state
        .database
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("database is not configured"))?;
    let chain = state
        .chain
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("starknet chain is not configured"))?;

    let server_seed = Felt::from(Uuid::new_v4().as_u128());
    let server_seed_hash = chain.hash_server_seed_commitment(server_seed);
    let reveal_deadline = chain
        .fetch_latest_block_number()
        .await
        .map_err(ApiError::internal)?
        .checked_add(ORIGINALS_REVEAL_TIMEOUT_BLOCKS)
        .ok_or_else(|| ApiError::internal(anyhow::anyhow!("invalid reveal deadline")))?;
    let (commitment_id, tx_hash) = chain
        .commit_baccarat_server_seed(server_seed_hash, reveal_deadline)
        .await
        .map_err(ApiError::internal)?;
    let commitment = chain
        .fetch_baccarat_commitment(commitment_id)
        .await
        .map_err(ApiError::internal)?;
    persistence::create_baccarat_server_commitment(
        database,
        commitment_id,
        &format!("{server_seed:#x}"),
        &format!("{server_seed_hash:#x}"),
        reveal_deadline,
        &tx_hash,
    )
    .await
    .map_err(ApiError::internal)?;
    Ok(Json(CreateDiceCommitmentResponse {
        commitment,
        tx_hash,
    }))
}

async fn settle_baccarat_commitment(
    State(state): State<Arc<AppState>>,
    Path(commitment_id): Path<u64>,
) -> ApiResult<SettleBaccaratCommitmentResponse> {
    let database = state
        .database
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("database is not configured"))?;
    let chain = state
        .chain
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("starknet chain is not configured"))?;
    let commitment = persistence::get_baccarat_server_commitment(database, commitment_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("baccarat commitment not found"))?;
    if commitment.status == "settled" {
        return Err(ApiError::bad_request(
            "baccarat commitment is already settled",
        ));
    }
    let round_id = chain
        .fetch_baccarat_round_for_commitment(commitment_id)
        .await
        .map_err(ApiError::internal)?;
    if round_id == 0 {
        return Err(ApiError::bad_request(
            "baccarat commitment has not been used by a round",
        ));
    }
    let server_seed = felt_from_dec_or_hex(&commitment.server_seed)
        .map_err(|error| ApiError::bad_request(format!("invalid stored server seed: {error}")))?;
    let (round, tx_hash) = chain
        .settle_baccarat_round(round_id, server_seed)
        .await
        .map_err(ApiError::internal)?;
    persistence::mark_baccarat_commitment_settled(database, commitment_id, round_id, &tx_hash)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(SettleBaccaratCommitmentResponse {
        round,
        tx_hash,
        server_seed: commitment.server_seed,
    }))
}

async fn get_baccarat_round(
    State(state): State<Arc<AppState>>,
    Path(round_id): Path<u64>,
) -> ApiResult<BaccaratRoundView> {
    let chain = state
        .chain
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("starknet chain is not configured"))?;
    let round = chain
        .fetch_baccarat_round(round_id)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(round))
}

async fn get_hand(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(hand_id): Path<String>,
) -> ApiResult<BlackjackHandPublicRecord> {
    let database = state
        .database
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("database is not configured"))?;
    let hand_id = Uuid::parse_str(&hand_id)
        .map_err(|error| ApiError::bad_request(format!("invalid hand_id: {error}")))?;

    let hand = persistence::get_blackjack_hand(database, hand_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("hand not found"))?;
    require_authorized_hand_access(&state, &headers, &hand).await?;

    Ok(Json(BlackjackHandPublicRecord {
        hand_id: hand.hand_id,
        player: hand.player,
        table_id: hand.table_id,
        wager: hand.wager,
        status: hand.status,
        phase: hand.phase,
        transcript_root: hand.transcript_root,
        active_seat: hand.active_seat,
        seat_count: hand.seat_count,
        dealer_upcard: hand.dealer_upcard,
        chain_hand_id: hand.chain_hand_id,
        created_at: hand.created_at,
        updated_at: hand.updated_at,
    }))
}

async fn get_hand_view(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(hand_id): Path<String>,
) -> ApiResult<moros_common::blackjack::BlackjackHandView> {
    let database = state
        .database
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("database is not configured"))?;
    let chain = state
        .chain
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("starknet chain is not configured"))?;
    let hand_id = Uuid::parse_str(&hand_id)
        .map_err(|error| ApiError::bad_request(format!("invalid hand_id: {error}")))?;

    let record = persistence::get_blackjack_hand(database, hand_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("hand not found"))?;
    require_authorized_hand_access(&state, &headers, &record).await?;
    let snapshot = persistence::get_blackjack_snapshot(database, hand_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("hand snapshot not found"))?;
    let chain_hand_id = record
        .chain_hand_id
        .ok_or_else(|| ApiError::not_found("chain hand is not attached"))?;
    let chain_hand = chain
        .fetch_blackjack_hand(chain_hand_id as u64)
        .await
        .map_err(ApiError::internal)?;
    let hand =
        blackjack::reconcile_view_with_chain(&snapshot, &chain_hand).map_err(ApiError::internal)?;

    Ok(Json(hand))
}

async fn get_hand_fairness(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(hand_id): Path<String>,
) -> ApiResult<moros_common::blackjack::BlackjackFairnessArtifactView> {
    let database = state
        .database
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("database is not configured"))?;
    let hand_id = Uuid::parse_str(&hand_id)
        .map_err(|error| ApiError::bad_request(format!("invalid hand_id: {error}")))?;

    let record = persistence::get_blackjack_hand(database, hand_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("hand not found"))?;
    require_authorized_hand_access(&state, &headers, &record).await?;
    let snapshot = persistence::get_blackjack_snapshot(database, hand_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("hand snapshot not found"))?;

    Ok(Json(blackjack::fairness_artifact_view(&snapshot)))
}

async fn get_session(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> ApiResult<SessionPublicResponse> {
    let redis = state
        .redis
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("redis is not configured"))?;
    let session_id = Uuid::parse_str(&session_id)
        .map_err(|error| ApiError::bad_request(format!("invalid session_id: {error}")))?;

    let session = runtime::get_session(redis, session_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("session runtime not found"))?;
    if !has_valid_admin_token(&state, &headers)? {
        let gameplay_session = require_gameplay_session(redis, &headers).await?;
        if !addresses_match(&gameplay_session.wallet_address, &session.player) {
            return Err(ApiError::unauthorized(
                "gameplay session does not match blackjack runtime",
            ));
        }
    }

    Ok(Json(SessionPublicResponse {
        session: SessionPublicState {
            session_id: session.session_id,
            hand_id: session.hand_id,
            player: session.player,
            table_id: session.table_id,
            transcript_root: session.transcript_root,
            status: session.status,
            phase: session.phase,
            allowed_actions: session.allowed_actions,
            expires_at_unix: session.expires_at_unix,
        },
    }))
}

async fn get_bet_feed(
    State(state): State<Arc<AppState>>,
    Query(query): Query<BetFeedQuery>,
) -> ApiResult<BetFeedResponse> {
    let database = state
        .database
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("database is not configured"))?;
    let chain = state
        .chain
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("starknet chain is not configured"))?;

    let limit = query.limit.unwrap_or(40).clamp(1, 100);
    let seeds = persistence::list_recent_settled_bets(
        database,
        i64::try_from(limit).map_err(ApiError::internal)?,
    )
    .await
    .map_err(ApiError::internal)?;

    let mut items = Vec::with_capacity(seeds.len());
    for seed in seeds {
        let item = match seed.game.as_str() {
            "blackjack" => {
                let hand = chain
                    .fetch_blackjack_hand(seed.game_id as u64)
                    .await
                    .map_err(ApiError::internal)?;
                let multiplier_bps = multiplier_bps(&hand.wager, &hand.total_payout);
                Some(BetFeedItem {
                    game: "Blackjack".to_string(),
                    user: hand.player.clone(),
                    wallet_address: hand.player,
                    bet_amount: hand.wager,
                    multiplier_bps,
                    payout: hand.total_payout,
                    tx_hash: seed.tx_hash,
                    settled_at: seed.settled_at,
                })
            }
            "dice" => {
                let round = chain
                    .fetch_dice_round(seed.game_id as u64)
                    .await
                    .map_err(ApiError::internal)?;
                Some(BetFeedItem {
                    game: "Dice".to_string(),
                    user: round.player.clone(),
                    wallet_address: round.player,
                    bet_amount: round.wager,
                    multiplier_bps: round.multiplier_bps.to_string(),
                    payout: round.payout,
                    tx_hash: seed.tx_hash,
                    settled_at: seed.settled_at,
                })
            }
            "roulette" => {
                let spin = chain
                    .fetch_roulette_spin(seed.game_id as u64)
                    .await
                    .map_err(ApiError::internal)?;
                let multiplier_bps = multiplier_bps(&spin.wager, &spin.payout);
                Some(BetFeedItem {
                    game: "Roulette".to_string(),
                    user: spin.player.clone(),
                    wallet_address: spin.player,
                    bet_amount: spin.wager,
                    multiplier_bps,
                    payout: spin.payout,
                    tx_hash: seed.tx_hash,
                    settled_at: seed.settled_at,
                })
            }
            "baccarat" => {
                let round = chain
                    .fetch_baccarat_round(seed.game_id as u64)
                    .await
                    .map_err(ApiError::internal)?;
                let multiplier_bps = multiplier_bps(&round.wager, &round.payout);
                Some(BetFeedItem {
                    game: "Baccarat".to_string(),
                    user: round.player.clone(),
                    wallet_address: round.player,
                    bet_amount: round.wager,
                    multiplier_bps,
                    payout: round.payout,
                    tx_hash: seed.tx_hash,
                    settled_at: seed.settled_at,
                })
            }
            _ => None,
        };
        if let Some(item) = item {
            items.push(item);
        }
    }

    let wallet_addresses = items
        .iter()
        .map(|item| item.wallet_address.clone())
        .collect::<Vec<_>>();
    let profiles = persistence::list_player_profiles_by_wallets(database, &wallet_addresses)
        .await
        .map_err(ApiError::internal)?;
    for item in &mut items {
        if let Some(profile) = profiles.get(&item.wallet_address) {
            if let Some(username) = profile.username.clone().filter(|value| !value.is_empty()) {
                item.user = username;
            } else {
                item.user = fallback_feed_username(&item.wallet_address);
            }
        } else {
            item.user = fallback_feed_username(&item.wallet_address);
        }
    }

    let mut high_rollers = items.clone();
    high_rollers
        .sort_by(|left, right| parse_u128(&right.bet_amount).cmp(&parse_u128(&left.bet_amount)));
    high_rollers.truncate(12);

    let mut race_leaderboard = items.clone();
    race_leaderboard
        .sort_by(|left, right| parse_u128(&right.payout).cmp(&parse_u128(&left.payout)));
    race_leaderboard.truncate(12);

    let my_bets_wallet = if let Some(user_id) = query.user_id.as_deref() {
        resolve_existing_account_by_user_id(database, user_id)
            .await?
            .wallet_address
    } else {
        query.player.clone()
    };

    let my_bets = my_bets_wallet
        .as_deref()
        .map(|player| {
            items
                .iter()
                .filter(|item| addresses_match(&item.wallet_address, player))
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok(Json(BetFeedResponse {
        my_bets,
        all_bets: items,
        high_rollers,
        race_leaderboard,
    }))
}

async fn get_account_balances_by_user(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
) -> ApiResult<balances::BalanceAccountRecord> {
    let database = state
        .database
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("database is not configured"))?;
    let (account, player_id) =
        require_authorized_account_access(&state, &headers, Some(&user_id), None).await?;
    Ok(Json(
        load_account_balance_snapshot(database, state.chain.as_ref(), &account, player_id).await?,
    ))
}

async fn get_account_balances_by_wallet(
    State(state): State<Arc<AppState>>,
    Path(wallet_address): Path<String>,
) -> ApiResult<balances::BalanceAccountRecord> {
    let database = state
        .database
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("database is not configured"))?;
    let wallet_address = normalize_wallet_address(&wallet_address)?;
    let account = resolve_existing_account_by_wallet(database, &wallet_address).await?;
    let player_id = Uuid::parse_str(&account.user_id).map_err(ApiError::internal)?;
    Ok(Json(
        load_account_balance_snapshot(database, state.chain.as_ref(), &account, player_id).await?,
    ))
}

async fn create_account_resolve_challenge(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateAccountResolveChallengeRequest>,
) -> ApiResult<AccountResolveChallengeResponse> {
    let redis = state
        .redis
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("redis is not configured"))?;
    let wallet_address = normalize_wallet_address(&payload.wallet_address)?;
    let challenge = AccountResolveChallengeState {
        wallet_address: wallet_address.clone(),
        linked_via: normalize_linked_via(payload.linked_via.as_deref(), "wallet"),
        make_primary: payload.make_primary.unwrap_or(false),
        challenge_id: felt_to_hex(Felt::from(Uuid::new_v4().as_u128())),
        expires_at_unix: runtime::now_unix() + ACCOUNT_RESOLVE_CHALLENGE_TTL_SECONDS,
    };
    cache_json_with_ttl(
        redis,
        account_resolve_challenge_key(&challenge.challenge_id),
        &challenge,
        challenge.expires_at_unix,
        "account resolve challenge",
    )
    .await
    .map_err(ApiError::internal)?;
    Ok(Json(AccountResolveChallengeResponse {
        wallet_address: challenge.wallet_address.clone(),
        challenge_id: challenge.challenge_id.clone(),
        expires_at_unix: challenge.expires_at_unix,
        typed_data: build_account_resolve_typed_data_value(
            state.profile_claim_chain_id,
            &challenge,
        )
        .map_err(ApiError::internal)?,
    }))
}

async fn resolve_account(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<EnsureAccountRequest>,
) -> ApiResult<accounts::PlayerAccountRecord> {
    let database = state
        .database
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("database is not configured"))?;
    let redis = state
        .redis
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("redis is not configured"))?;
    let chain = state
        .chain
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("starknet chain is not configured"))?;
    let account = if let Some(user_id) = payload.user_id.as_deref() {
        resolve_existing_account_by_user_id(database, user_id).await?
    } else {
        if payload.auth_provider.is_some() || payload.auth_subject.is_some() {
            return Err(ApiError::bad_request(
                "authenticated account resolution must go through the verified auth bridge",
            ));
        }
        let wallet_address = payload
            .wallet_address
            .as_deref()
            .ok_or_else(|| ApiError::bad_request("wallet address is required"))?;
        let wallet_address = normalize_wallet_address(wallet_address)?;
        let challenge_id = payload
            .challenge_id
            .as_deref()
            .ok_or_else(|| ApiError::bad_request("account resolve challenge is required"))?;
        let signature = payload
            .signature
            .as_ref()
            .ok_or_else(|| ApiError::bad_request("account resolve signature is required"))?;
        let challenge = read_cached_json::<AccountResolveChallengeState>(
            redis,
            account_resolve_challenge_key(challenge_id),
            "account resolve challenge",
        )
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::bad_request("account resolve challenge is invalid or expired"))?;
        if challenge.expires_at_unix < runtime::now_unix() {
            delete_cached_key(
                redis,
                account_resolve_challenge_key(challenge_id),
                "account resolve challenge",
            )
            .await
            .map_err(ApiError::internal)?;
            return Err(ApiError::bad_request(
                "account resolve challenge is invalid or expired",
            ));
        }
        if !addresses_match(&challenge.wallet_address, &wallet_address) {
            return Err(ApiError::bad_request(
                "account resolve challenge does not match wallet",
            ));
        }
        let typed_data = build_account_resolve_typed_data(state.profile_claim_chain_id, &challenge)
            .map_err(ApiError::internal)?;
        let parsed_signature = parse_signature(signature)?;
        let wallet_felt = felt_from_dec_or_hex(&wallet_address).map_err(ApiError::internal)?;
        let message_hash = typed_data
            .message_hash(wallet_felt)
            .map_err(ApiError::internal)?;
        let signature_valid = chain
            .verify_message_signature(wallet_felt, message_hash, &parsed_signature)
            .await
            .map_err(signature_verification_error)?;
        if !signature_valid {
            return Err(ApiError::bad_request(
                "account resolve signature is invalid",
            ));
        }
        delete_cached_key(
            redis,
            account_resolve_challenge_key(challenge_id),
            "account resolve challenge",
        )
        .await
        .map_err(ApiError::internal)?;
        ensure_account_from_identity(
            database,
            Some(&wallet_address),
            None,
            None,
            &challenge.linked_via,
            challenge.make_primary,
        )
        .await?
    };
    Ok(Json(account))
}

async fn resolve_verified_account(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<EnsureVerifiedAccountRequest>,
) -> ApiResult<accounts::PlayerAccountRecord> {
    require_admin_token(&state, &headers)?;
    let database = state
        .database
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("database is not configured"))?;
    let account = ensure_account_from_identity(
        database,
        payload.wallet_address.as_deref(),
        payload.auth_provider.as_deref(),
        payload.auth_subject.as_deref(),
        payload
            .linked_via
            .as_deref()
            .unwrap_or("verified_auth_bridge"),
        payload.make_primary.unwrap_or(false),
    )
    .await?;
    Ok(Json(account))
}

async fn transfer_account_balances(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<BalanceTransferRequest>,
) -> ApiResult<balances::BalanceAccountRecord> {
    let database = state
        .database
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("database is not configured"))?;
    let (account, player_id) = require_authorized_account_access(
        &state,
        &headers,
        payload.user_id.as_deref(),
        payload.wallet_address.as_deref(),
    )
    .await?;
    let amount = parse_wager_u128(&payload.amount, "amount")?;
    let updated = match payload.direction.as_str() {
        "gambling_to_vault" => {
            balances::transfer_from_gambling_to_vault(
                database,
                player_id,
                amount,
                Some("manual_transfer"),
                account.wallet_address.as_deref(),
                serde_json::json!({ "direction": payload.direction }),
            )
            .await
        }
        "vault_to_gambling" => {
            balances::transfer_from_vault_to_gambling(
                database,
                player_id,
                amount,
                Some("manual_transfer"),
                account.wallet_address.as_deref(),
                serde_json::json!({ "direction": payload.direction }),
            )
            .await
        }
        _ => Err(anyhow::anyhow!("unsupported balance transfer direction")),
    }
    .map_err(ApiError::internal)?;
    Ok(Json(updated))
}

async fn create_account_withdrawal(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<CreateWithdrawalRequest>,
) -> ApiResult<withdrawals::WithdrawalRequestRecord> {
    let database = state
        .database
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("database is not configured"))?;
    let (account, player_id) = require_authorized_account_access(
        &state,
        &headers,
        payload.user_id.as_deref(),
        payload.wallet_address.as_deref(),
    )
    .await?;
    let amount = parse_wager_u128(&payload.amount, "amount")?;
    if amount == 0 {
        return Err(ApiError::bad_request(
            "withdrawal amount must be greater than zero",
        ));
    }
    let destination_address = payload.destination_address.trim();
    if destination_address.is_empty() {
        return Err(ApiError::bad_request("destination address is required"));
    }
    let destination_chain_key = payload
        .destination_chain_key
        .clone()
        .unwrap_or_else(|| "starknet".to_string());
    let destination_asset_symbol = payload
        .destination_asset_symbol
        .clone()
        .unwrap_or_else(|| "STRK".to_string());
    if destination_chain_key.eq_ignore_ascii_case("starknet")
        && destination_asset_symbol.eq_ignore_ascii_case("STRK")
        && payload
            .destination_tx_hash
            .as_deref()
            .unwrap_or_default()
            .trim()
            .is_empty()
    {
        return Err(ApiError::bad_request(
            "submit a signed vault withdrawal transaction before recording withdrawal",
        ));
    }

    let withdrawal = withdrawals::create_withdrawal_request(
        database,
        withdrawals::CreateWithdrawalInput {
            player_id,
            requested_by_wallet: account.wallet_address.clone(),
            source_balance: payload
                .source_balance
                .unwrap_or_else(|| "vault".to_string()),
            destination_chain_key,
            destination_asset_symbol,
            destination_address: destination_address.to_string(),
            amount_raw: amount,
            destination_tx_hash: payload.destination_tx_hash,
            metadata: serde_json::json!({
                "requested_from": "wallet_page",
                "settlement_model": "user_signed_vault_withdrawal",
            }),
        },
    )
    .await
    .map_err(map_financial_error)?;

    Ok(Json(withdrawal))
}

async fn list_account_withdrawals_by_user(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
    Query(query): Query<WithdrawalListQuery>,
) -> ApiResult<Vec<withdrawals::WithdrawalRequestRecord>> {
    let database = state
        .database
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("database is not configured"))?;
    let (_, player_id) =
        require_authorized_account_access(&state, &headers, Some(&user_id), None).await?;
    let withdrawals =
        withdrawals::list_withdrawal_requests(database, player_id, query.limit.unwrap_or(20))
            .await
            .map_err(ApiError::internal)?;
    Ok(Json(withdrawals))
}

async fn get_username_availability(
    State(state): State<Arc<AppState>>,
    Path(username): Path<String>,
) -> ApiResult<UsernameAvailabilityResponse> {
    let database = state
        .database
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("database is not configured"))?;
    let normalized = normalize_username(&username)?;
    let available = persistence::username_is_available(database, &normalized)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(UsernameAvailabilityResponse {
        username: normalized,
        available,
    }))
}

async fn create_profile_claim_challenge(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateProfileClaimChallengeRequest>,
) -> ApiResult<ProfileClaimChallengeResponse> {
    let database = state
        .database
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("database is not configured"))?;
    let redis = state
        .redis
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("redis is not configured"))?;

    let wallet_address = normalize_wallet_address(&payload.wallet_address)?;
    let username = payload
        .username
        .as_deref()
        .map(normalize_username)
        .transpose()?;
    if let Some(username) = username.as_deref() {
        let existing = persistence::get_player_profile_by_username(database, username)
            .await
            .map_err(ApiError::internal)?;
        if let Some(existing) = existing {
            if existing
                .wallet_address
                .as_deref()
                .is_some_and(|existing_wallet| !addresses_match(existing_wallet, &wallet_address))
            {
                return Err(ApiError::bad_request("username is already claimed"));
            }
        }
    }

    let challenge = ProfileClaimChallengeState {
        wallet_address: wallet_address.clone(),
        username: username.clone(),
        auth_provider: normalize_profile_provider(payload.auth_provider.as_deref()),
        challenge_id: felt_to_hex(Felt::from(Uuid::new_v4().as_u128())),
        expires_at_unix: runtime::now_unix() + 300,
    };
    cache_profile_claim_challenge(redis, &challenge)
        .await
        .map_err(ApiError::internal)?;

    Ok(Json(ProfileClaimChallengeResponse {
        wallet_address: challenge.wallet_address.clone(),
        username: challenge.username.clone(),
        auth_provider: challenge.auth_provider.clone(),
        challenge_id: challenge.challenge_id.clone(),
        expires_at_unix: challenge.expires_at_unix,
        typed_data: build_profile_claim_typed_data_value(state.profile_claim_chain_id, &challenge)
            .map_err(ApiError::internal)?,
    }))
}

async fn get_profile_by_wallet(
    State(state): State<Arc<AppState>>,
    Path(wallet_address): Path<String>,
) -> ApiResult<persistence::PlayerProfileRecord> {
    let database = state
        .database
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("database is not configured"))?;
    let profile = persistence::get_player_profile_by_wallet(database, &wallet_address)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("player profile not found"))?;
    Ok(Json(profile))
}

async fn get_profile_by_username(
    State(state): State<Arc<AppState>>,
    Path(username): Path<String>,
) -> ApiResult<persistence::PlayerProfileRecord> {
    let database = state
        .database
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("database is not configured"))?;
    let username = normalize_username(&username)?;
    let profile = persistence::get_player_profile_by_username(database, &username)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("player profile not found"))?;
    Ok(Json(profile))
}

async fn upsert_profile(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<UpsertProfileRequest>,
) -> ApiResult<persistence::PlayerProfileRecord> {
    let database = state
        .database
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("database is not configured"))?;
    let redis = state
        .redis
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("redis is not configured"))?;
    let chain = state
        .chain
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("starknet chain is not configured"))?;
    let wallet_address = normalize_wallet_address(&payload.wallet_address)?;
    let normalized = payload
        .username
        .as_deref()
        .map(normalize_username)
        .transpose()?;
    let challenge = get_profile_claim_challenge(redis, &payload.challenge_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::bad_request("profile claim challenge is invalid or expired"))?;
    if challenge.expires_at_unix < runtime::now_unix() {
        delete_profile_claim_challenge(redis, &challenge.challenge_id)
            .await
            .map_err(ApiError::internal)?;
        return Err(ApiError::bad_request(
            "profile claim challenge is invalid or expired",
        ));
    }
    if !addresses_match(&challenge.wallet_address, &wallet_address) {
        return Err(ApiError::bad_request(
            "profile claim challenge does not match wallet",
        ));
    }
    if challenge.username.as_deref() != normalized.as_deref() {
        return Err(ApiError::bad_request(
            "profile claim challenge does not match username",
        ));
    }
    let auth_provider = normalize_profile_provider(payload.auth_provider.as_deref());
    if challenge.auth_provider != auth_provider {
        return Err(ApiError::bad_request(
            "profile claim challenge does not match auth provider",
        ));
    }
    let typed_data = build_profile_claim_typed_data(state.profile_claim_chain_id, &challenge)
        .map_err(ApiError::internal)?;
    let signature = parse_signature(&payload.signature)?;
    let wallet_felt = felt_from_dec_or_hex(&wallet_address).map_err(ApiError::internal)?;
    let message_hash = typed_data
        .message_hash(wallet_felt)
        .map_err(ApiError::internal)?;
    let signature_valid = chain
        .verify_message_signature(wallet_felt, message_hash, &signature)
        .await
        .map_err(signature_verification_error)?;
    if !signature_valid {
        return Err(ApiError::bad_request("profile signature is invalid"));
    }
    delete_profile_claim_challenge(redis, &challenge.challenge_id)
        .await
        .map_err(ApiError::internal)?;

    if let Some(username) = normalized.as_deref() {
        let existing = persistence::get_player_profile_by_username(database, username)
            .await
            .map_err(ApiError::internal)?;
        if let Some(existing) = existing {
            if existing
                .wallet_address
                .as_deref()
                .is_some_and(|existing_wallet| !addresses_match(existing_wallet, &wallet_address))
            {
                return Err(ApiError::bad_request("username is already claimed"));
            }
        }
    }
    let profile = persistence::upsert_player_profile(
        database,
        &wallet_address,
        normalized.as_deref(),
        &auth_provider,
        Some(&wallet_address),
    )
    .await
    .map_err(ApiError::internal)?;
    Ok(Json(profile))
}

async fn get_rewards_state(
    State(state): State<Arc<AppState>>,
    Path(wallet_address): Path<String>,
) -> ApiResult<RewardsStateView> {
    let database = state
        .database
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("database is not configured"))?;
    let wallet_address = normalize_wallet_address(&wallet_address)?;
    let rewards_state =
        rewards::get_rewards_state(database, &wallet_address, &state.rewards_config)
            .await
            .map_err(map_reward_error)?;
    Ok(Json(rewards_state))
}

async fn get_rewards_state_by_user(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
) -> ApiResult<RewardsStateView> {
    let database = state
        .database
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("database is not configured"))?;
    require_authorized_account_access(&state, &headers, Some(&user_id), None).await?;
    let (_, wallet_address) =
        resolve_rewards_account_wallet(database, Some(&user_id), None).await?;
    let rewards_state =
        rewards::get_rewards_state(database, &wallet_address, &state.rewards_config)
            .await
            .map_err(map_reward_error)?;
    Ok(Json(rewards_state))
}

async fn create_rewards_claim_challenge(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateRewardsClaimChallengeRequest>,
) -> ApiResult<RewardsClaimChallengeResponse> {
    let database = state
        .database
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("database is not configured"))?;
    let redis = state
        .redis
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("redis is not configured"))?;
    let (_, wallet_address) = resolve_rewards_account_wallet(
        database,
        payload.user_id.as_deref(),
        payload.wallet_address.as_deref(),
    )
    .await?;
    let reward_kind = RewardKind::parse(&payload.reward_kind)
        .ok_or_else(|| ApiError::bad_request("reward kind is invalid"))?;
    let reserved = rewards::reserve_reward_claim(
        database,
        &wallet_address,
        reward_kind,
        &state.rewards_config,
    )
    .await
    .map_err(map_reward_error)?;
    let challenge = RewardsClaimChallengeState {
        wallet_address: reserved.wallet_address.clone(),
        reward_kind: reward_kind.as_str().to_string(),
        claim_id: reserved.claim_id.to_string(),
        amount_raw: reserved.amount_raw.to_string(),
        challenge_id: felt_to_hex(Felt::from(Uuid::new_v4().as_u128())),
        expires_at_unix: if reserved.status == "submitted" {
            runtime::now_unix() + 300
        } else {
            reserved.expires_at_unix.min(runtime::now_unix() + 300)
        },
    };
    cache_json_with_ttl(
        redis,
        rewards_claim_challenge_key(&challenge.challenge_id),
        &challenge,
        challenge.expires_at_unix,
        "rewards claim challenge",
    )
    .await
    .map_err(ApiError::internal)?;
    Ok(Json(RewardsClaimChallengeResponse {
        wallet_address: challenge.wallet_address.clone(),
        reward_kind: challenge.reward_kind.clone(),
        claim_id: challenge.claim_id.clone(),
        amount_raw: challenge.amount_raw.clone(),
        challenge_id: challenge.challenge_id.clone(),
        expires_at_unix: challenge.expires_at_unix,
        typed_data: build_rewards_claim_typed_data_value(state.profile_claim_chain_id, &challenge)
            .map_err(ApiError::internal)?,
    }))
}

async fn claim_rewards(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ClaimRewardsRequest>,
) -> ApiResult<ClaimRewardsResponse> {
    let database = state
        .database
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("database is not configured"))?;
    let redis = state
        .redis
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("redis is not configured"))?;
    let chain = state
        .chain
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("starknet chain is not configured"))?;
    let (_, wallet_address) = resolve_rewards_account_wallet(
        database,
        payload.user_id.as_deref(),
        payload.wallet_address.as_deref(),
    )
    .await?;
    let reward_kind = RewardKind::parse(&payload.reward_kind)
        .ok_or_else(|| ApiError::bad_request("reward kind is invalid"))?;
    let claim_id = Uuid::parse_str(&payload.claim_id)
        .map_err(|_| ApiError::bad_request("claim id is invalid"))?;
    let challenge = read_cached_json::<RewardsClaimChallengeState>(
        redis,
        rewards_claim_challenge_key(&payload.challenge_id),
        "rewards claim challenge",
    )
    .await
    .map_err(ApiError::internal)?
    .ok_or_else(|| ApiError::bad_request("rewards claim challenge is invalid or expired"))?;
    if challenge.expires_at_unix < runtime::now_unix() {
        delete_cached_key(
            redis,
            rewards_claim_challenge_key(&payload.challenge_id),
            "rewards claim challenge",
        )
        .await
        .map_err(ApiError::internal)?;
        return Err(ApiError::bad_request(
            "rewards claim challenge is invalid or expired",
        ));
    }
    if !addresses_match(&challenge.wallet_address, &wallet_address) {
        return Err(ApiError::bad_request(
            "rewards claim challenge does not match wallet",
        ));
    }
    if challenge.reward_kind != reward_kind.as_str() {
        return Err(ApiError::bad_request(
            "rewards claim challenge does not match reward kind",
        ));
    }
    if challenge.claim_id != payload.claim_id {
        return Err(ApiError::bad_request(
            "rewards claim challenge does not match claim id",
        ));
    }
    let typed_data = build_rewards_claim_typed_data(state.profile_claim_chain_id, &challenge)
        .map_err(ApiError::internal)?;
    let signature = parse_signature(&payload.signature)?;
    let wallet_felt = felt_from_dec_or_hex(&wallet_address).map_err(ApiError::internal)?;
    let message_hash = typed_data
        .message_hash(wallet_felt)
        .map_err(ApiError::internal)?;
    let signature_valid = chain
        .verify_message_signature(wallet_felt, message_hash, &signature)
        .await
        .map_err(signature_verification_error)?;
    if !signature_valid {
        return Err(ApiError::bad_request("rewards claim signature is invalid"));
    }
    delete_cached_key(
        redis,
        rewards_claim_challenge_key(&payload.challenge_id),
        "rewards claim challenge",
    )
    .await
    .map_err(ApiError::internal)?;

    let submission =
        rewards::begin_reward_claim_submission(database, claim_id, &wallet_address, reward_kind)
            .await
            .map_err(map_reward_error)?;
    let reserved = match submission {
        rewards::RewardClaimSubmissionState::Ready(reserved) => {
            let tx_hash = match chain
                .credit_rewards_to_vault(&wallet_address, reserved.amount_raw)
                .await
            {
                Ok(tx_hash) => tx_hash,
                Err(error) => {
                    let _ =
                        rewards::mark_reward_claim_failed(database, claim_id, &error.to_string())
                            .await;
                    return Err(ApiError::internal(error));
                }
            };
            rewards::mark_reward_claim_submitted(database, claim_id, &tx_hash)
                .await
                .map_err(ApiError::internal)?;
            match chain.wait_for_transaction_success(&tx_hash).await {
                Ok(()) => rewards::mark_reward_claim_confirmed(database, claim_id, &tx_hash)
                    .await
                    .map_err(ApiError::internal)?,
                Err(error) => {
                    if error.to_string().contains("reverted") {
                        let _ = rewards::mark_reward_claim_failed(
                            database,
                            claim_id,
                            &error.to_string(),
                        )
                        .await;
                    }
                    return Err(ApiError::internal(error));
                }
            }
        }
        rewards::RewardClaimSubmissionState::Submitted(reserved) => {
            let tx_hash = reserved.tx_hash.clone().ok_or_else(|| {
                ApiError::internal(anyhow::anyhow!("reward claim tx hash missing"))
            })?;
            match chain.wait_for_transaction_success(&tx_hash).await {
                Ok(()) => rewards::mark_reward_claim_confirmed(database, claim_id, &tx_hash)
                    .await
                    .map_err(ApiError::internal)?,
                Err(error) => {
                    if error.to_string().contains("reverted") {
                        let _ = rewards::mark_reward_claim_failed(
                            database,
                            claim_id,
                            &error.to_string(),
                        )
                        .await;
                    }
                    return Err(ApiError::internal(error));
                }
            }
        }
        rewards::RewardClaimSubmissionState::Claimed(reserved) => reserved,
    };
    let tx_hash = reserved.tx_hash.clone().unwrap_or_default();
    Ok(Json(ClaimRewardsResponse {
        reward_kind: reward_kind.as_str().to_string(),
        claim_id: reserved.claim_id.to_string(),
        amount_raw: reserved.amount_raw.to_string(),
        tx_hash,
        claim_rows: reserved.claim_rows.len(),
        status: reserved.status,
    }))
}

async fn create_reward_coupons(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<CreateRewardCouponRequest>,
) -> ApiResult<CreateRewardCouponResponse> {
    require_admin_token(&state, &headers)?;
    let database = state
        .database
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("database is not configured"))?;
    let amount_raw = parse_wager_u128(&payload.amount_raw, "coupon amount")?;
    if amount_raw == 0 {
        return Err(ApiError::bad_request(
            "coupon amount must be greater than zero",
        ));
    }
    let quantity = payload.quantity.unwrap_or(1);
    if quantity == 0 || quantity > 100 {
        return Err(ApiError::bad_request(
            "coupon quantity must be between 1 and 100",
        ));
    }
    if quantity > 1 && payload.code.is_some() {
        return Err(ApiError::bad_request(
            "explicit code can only be used with quantity 1",
        ));
    }
    let max_global_redemptions = payload.max_global_redemptions.unwrap_or(1);
    let max_per_user_redemptions = payload.max_per_user_redemptions.unwrap_or(1);
    let mut coupons = Vec::with_capacity(quantity);
    for index in 0..quantity {
        let coupon = rewards::create_reward_coupon(
            database,
            rewards::CreateRewardCouponInput {
                code: if quantity == 1 {
                    payload.code.clone()
                } else {
                    None
                },
                description: payload.description.clone(),
                amount_raw,
                max_global_redemptions,
                max_per_user_redemptions,
                starts_at_unix: payload.starts_at_unix,
                expires_at_unix: payload.expires_at_unix,
                active: payload.active.unwrap_or(true),
                created_by: payload.created_by.clone(),
                metadata: serde_json::json!({
                    "batch_quantity": quantity,
                    "batch_index": index,
                }),
            },
        )
        .await
        .map_err(map_reward_error)?;
        coupons.push(coupon);
    }
    Ok(Json(CreateRewardCouponResponse { coupons }))
}

async fn redeem_reward_coupon(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<RedeemRewardCouponRequest>,
) -> ApiResult<RedeemRewardCouponResponse> {
    let database = state
        .database
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("database is not configured"))?;
    let chain = state
        .chain
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("starknet chain is not configured"))?;
    let (account, player_id) = require_authorized_account_access(
        &state,
        &headers,
        payload.user_id.as_deref(),
        payload.wallet_address.as_deref(),
    )
    .await?;
    let wallet_address = account.wallet_address.as_deref().ok_or_else(|| {
        ApiError::bad_request("Moros account does not have an execution wallet linked")
    })?;
    let reserved = match rewards::reserve_reward_coupon_redemption(
        database,
        player_id,
        wallet_address,
        &payload.code,
    )
    .await
    .map_err(map_reward_error)?
    {
        rewards::RewardCouponRedemptionState::Ready(reserved) => {
            let tx_hash = match chain
                .credit_rewards_to_vault(&reserved.wallet_address, reserved.amount_raw)
                .await
            {
                Ok(tx_hash) => tx_hash,
                Err(error) => {
                    let _ = rewards::mark_reward_coupon_redemption_failed(
                        database,
                        reserved.redemption_id,
                        &error.to_string(),
                    )
                    .await;
                    return Err(ApiError::internal(error));
                }
            };
            rewards::mark_reward_coupon_redemption_submitted(
                database,
                reserved.redemption_id,
                &tx_hash,
            )
            .await
            .map_err(ApiError::internal)?;
            match chain.wait_for_transaction_success(&tx_hash).await {
                Ok(()) => rewards::mark_reward_coupon_redemption_confirmed(
                    database,
                    reserved.redemption_id,
                    &tx_hash,
                )
                .await
                .map_err(ApiError::internal)?,
                Err(error) => {
                    if error.to_string().contains("reverted") {
                        let _ = rewards::mark_reward_coupon_redemption_failed(
                            database,
                            reserved.redemption_id,
                            &error.to_string(),
                        )
                        .await;
                    }
                    return Err(ApiError::internal(error));
                }
            }
        }
        rewards::RewardCouponRedemptionState::Submitted(reserved) => {
            let tx_hash = reserved.tx_hash.clone().ok_or_else(|| {
                ApiError::internal(anyhow::anyhow!("coupon redemption tx hash missing"))
            })?;
            match chain.wait_for_transaction_success(&tx_hash).await {
                Ok(()) => rewards::mark_reward_coupon_redemption_confirmed(
                    database,
                    reserved.redemption_id,
                    &tx_hash,
                )
                .await
                .map_err(ApiError::internal)?,
                Err(error) => {
                    if error.to_string().contains("reverted") {
                        let _ = rewards::mark_reward_coupon_redemption_failed(
                            database,
                            reserved.redemption_id,
                            &error.to_string(),
                        )
                        .await;
                    }
                    return Err(ApiError::internal(error));
                }
            }
        }
        rewards::RewardCouponRedemptionState::Claimed(reserved) => reserved,
    };
    Ok(Json(RedeemRewardCouponResponse {
        redemption_id: reserved.redemption_id.to_string(),
        coupon_id: reserved.coupon_id.to_string(),
        code: reserved.code,
        amount_raw: reserved.amount_raw.to_string(),
        tx_hash: reserved.tx_hash.unwrap_or_default(),
        status: reserved.status,
    }))
}

async fn create_referrer_link_challenge(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateReferrerLinkChallengeRequest>,
) -> ApiResult<ReferrerLinkChallengeResponse> {
    let database = state
        .database
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("database is not configured"))?;
    let redis = state
        .redis
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("redis is not configured"))?;
    let (_, wallet_address) = resolve_rewards_account_wallet(
        database,
        payload.user_id.as_deref(),
        payload.wallet_address.as_deref(),
    )
    .await?;
    let referrer = payload.referrer.trim().to_ascii_lowercase();
    if referrer.is_empty() {
        return Err(ApiError::bad_request("referrer is required"));
    }
    let challenge = ReferrerLinkChallengeState {
        wallet_address: wallet_address.clone(),
        referrer: referrer.clone(),
        challenge_id: felt_to_hex(Felt::from(Uuid::new_v4().as_u128())),
        expires_at_unix: runtime::now_unix() + 300,
    };
    cache_json_with_ttl(
        redis,
        referrer_link_challenge_key(&challenge.challenge_id),
        &challenge,
        challenge.expires_at_unix,
        "referrer link challenge",
    )
    .await
    .map_err(ApiError::internal)?;
    Ok(Json(ReferrerLinkChallengeResponse {
        wallet_address: challenge.wallet_address.clone(),
        referrer: challenge.referrer.clone(),
        challenge_id: challenge.challenge_id.clone(),
        expires_at_unix: challenge.expires_at_unix,
        typed_data: build_referrer_link_typed_data_value(state.profile_claim_chain_id, &challenge)
            .map_err(ApiError::internal)?,
    }))
}

async fn set_referrer(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<SetReferrerRequest>,
) -> ApiResult<ReferralBindingView> {
    let database = state
        .database
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("database is not configured"))?;
    let redis = state
        .redis
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("redis is not configured"))?;
    let chain = state
        .chain
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("starknet chain is not configured"))?;
    let (_, wallet_address) = resolve_rewards_account_wallet(
        database,
        payload.user_id.as_deref(),
        payload.wallet_address.as_deref(),
    )
    .await?;
    let challenge = read_cached_json::<ReferrerLinkChallengeState>(
        redis,
        referrer_link_challenge_key(&payload.challenge_id),
        "referrer link challenge",
    )
    .await
    .map_err(ApiError::internal)?
    .ok_or_else(|| ApiError::bad_request("referrer link challenge is invalid or expired"))?;
    if challenge.expires_at_unix < runtime::now_unix() {
        delete_cached_key(
            redis,
            referrer_link_challenge_key(&payload.challenge_id),
            "referrer link challenge",
        )
        .await
        .map_err(ApiError::internal)?;
        return Err(ApiError::bad_request(
            "referrer link challenge is invalid or expired",
        ));
    }
    if !addresses_match(&challenge.wallet_address, &wallet_address) {
        return Err(ApiError::bad_request(
            "referrer link challenge does not match wallet",
        ));
    }
    if challenge.referrer != payload.referrer.trim().to_ascii_lowercase() {
        return Err(ApiError::bad_request(
            "referrer link challenge does not match referrer",
        ));
    }
    let typed_data = build_referrer_link_typed_data(state.profile_claim_chain_id, &challenge)
        .map_err(ApiError::internal)?;
    let signature = parse_signature(&payload.signature)?;
    let wallet_felt = felt_from_dec_or_hex(&wallet_address).map_err(ApiError::internal)?;
    let message_hash = typed_data
        .message_hash(wallet_felt)
        .map_err(ApiError::internal)?;
    let signature_valid = chain
        .verify_message_signature(wallet_felt, message_hash, &signature)
        .await
        .map_err(signature_verification_error)?;
    if !signature_valid {
        return Err(ApiError::bad_request("referrer link signature is invalid"));
    }
    delete_cached_key(
        redis,
        referrer_link_challenge_key(&payload.challenge_id),
        "referrer link challenge",
    )
    .await
    .map_err(ApiError::internal)?;

    let binding = rewards::bind_referrer(database, &wallet_address, &payload.referrer)
        .await
        .map_err(map_reward_error)?;
    Ok(Json(binding))
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: message.into(),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
        }
    }

    fn service_unavailable(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            message: message.into(),
        }
    }

    fn internal(error: impl Into<anyhow::Error>) -> Self {
        let error = error.into();
        tracing::error!(error = ?error, "coordinator request failed");
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: error.to_string(),
        }
    }
}

fn felt_from_dec_or_hex(value: &str) -> anyhow::Result<Felt> {
    if value.starts_with("0x") {
        Felt::from_hex(value).map_err(Into::into)
    } else {
        Felt::from_dec_str(value).map_err(Into::into)
    }
}

fn normalize_username(value: &str) -> Result<String, ApiError> {
    let normalized = value.trim().to_lowercase();
    let is_valid = !normalized.is_empty()
        && normalized.len() >= 3
        && normalized.len() <= 16
        && normalized.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        });

    if !is_valid {
        return Err(ApiError::bad_request(
            "username must be 3-16 chars and use only lowercase letters, digits, or underscores",
        ));
    }

    Ok(normalized)
}

fn fallback_feed_username(wallet_address: &str) -> String {
    let normalized = wallet_address.trim().to_ascii_lowercase();
    if normalized.len() <= 14 {
        return normalized;
    }

    if normalized.starts_with("0x") {
        return format!(
            "{}...{}",
            &normalized[..6],
            &normalized[normalized.len() - 4..]
        );
    }

    format!(
        "{}...{}",
        &normalized[..6],
        &normalized[normalized.len() - 4..]
    )
}

fn normalize_wallet_address(value: &str) -> Result<String, ApiError> {
    let felt = felt_from_dec_or_hex(value)
        .map_err(|_| ApiError::bad_request("wallet address is invalid"))?;
    Ok(felt_to_hex(felt))
}

fn felt_to_hex(value: Felt) -> String {
    format!("{value:#x}")
}

fn normalize_profile_provider(value: Option<&str>) -> String {
    match value
        .unwrap_or("wallet")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "privy" => "privy".to_string(),
        "google" => "google".to_string(),
        "email" => "email".to_string(),
        _ => "wallet".to_string(),
    }
}

fn normalize_linked_via(value: Option<&str>, fallback: &str) -> String {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback)
        .to_string()
}

fn profile_claim_chain_id(chain: &str) -> &'static str {
    if chain.eq_ignore_ascii_case("mainnet") {
        "SN_MAIN"
    } else {
        "SN_SEPOLIA"
    }
}

fn build_profile_claim_typed_data(
    chain_id: &str,
    challenge: &ProfileClaimChallengeState,
) -> anyhow::Result<TypedData> {
    serde_json::from_value(build_profile_claim_typed_data_value(chain_id, challenge)?)
        .map_err(Into::into)
}

fn build_profile_claim_typed_data_value(
    chain_id: &str,
    challenge: &ProfileClaimChallengeState,
) -> anyhow::Result<Value> {
    Ok(serde_json::json!({
        "types": {
            "StarknetDomain": [
                { "name": "name", "type": "shortstring" },
                { "name": "version", "type": "shortstring" },
                { "name": "chainId", "type": "shortstring" },
                { "name": "revision", "type": "shortstring" }
            ],
            "MorosProfileClaim": [
                { "name": "wallet", "type": "ContractAddress" },
                { "name": "username", "type": "string" },
                { "name": "challenge", "type": "felt" },
                { "name": "expiresAt", "type": "u128" },
                { "name": "authProvider", "type": "shortstring" }
            ]
        },
        "primaryType": "MorosProfileClaim",
        "domain": {
            "name": "Moros",
            "version": "1",
            "chainId": chain_id,
            "revision": "1"
        },
        "message": {
            "wallet": challenge.wallet_address,
            "username": challenge.username.clone().unwrap_or_default(),
            "challenge": challenge.challenge_id,
            "expiresAt": challenge.expires_at_unix.to_string(),
            "authProvider": challenge.auth_provider
        }
    }))
}

fn build_gameplay_session_typed_data(
    chain_id: &str,
    challenge: &GameplaySessionChallengeState,
) -> anyhow::Result<TypedData> {
    serde_json::from_value(build_gameplay_session_typed_data_value(
        chain_id, challenge,
    )?)
    .map_err(Into::into)
}

fn build_gameplay_session_typed_data_value(
    chain_id: &str,
    challenge: &GameplaySessionChallengeState,
) -> anyhow::Result<Value> {
    Ok(serde_json::json!({
        "types": {
            "StarknetDomain": [
                { "name": "name", "type": "shortstring" },
                { "name": "version", "type": "shortstring" },
                { "name": "chainId", "type": "shortstring" },
                { "name": "revision", "type": "shortstring" }
            ],
            "MorosGameplaySession": [
                { "name": "wallet", "type": "ContractAddress" },
                { "name": "challenge", "type": "felt" },
                { "name": "expiresAt", "type": "u128" },
                { "name": "scope", "type": "shortstring" }
            ]
        },
        "primaryType": "MorosGameplaySession",
        "domain": {
            "name": "Moros",
            "version": "1",
            "chainId": chain_id,
            "revision": "1"
        },
        "message": {
            "wallet": challenge.wallet_address,
            "challenge": challenge.challenge_id,
            "expiresAt": challenge.expires_at_unix.to_string(),
            "scope": "gameplay"
        }
    }))
}

fn build_account_resolve_typed_data(
    chain_id: &str,
    challenge: &AccountResolveChallengeState,
) -> anyhow::Result<TypedData> {
    serde_json::from_value(build_account_resolve_typed_data_value(chain_id, challenge)?)
        .map_err(Into::into)
}

fn build_account_resolve_typed_data_value(
    chain_id: &str,
    challenge: &AccountResolveChallengeState,
) -> anyhow::Result<Value> {
    Ok(serde_json::json!({
        "types": {
            "StarknetDomain": [
                { "name": "name", "type": "shortstring" },
                { "name": "version", "type": "shortstring" },
                { "name": "chainId", "type": "shortstring" },
                { "name": "revision", "type": "shortstring" }
            ],
            "MorosAccountResolve": [
                { "name": "wallet", "type": "ContractAddress" },
                { "name": "linkedVia", "type": "shortstring" },
                { "name": "mode", "type": "shortstring" },
                { "name": "challenge", "type": "felt" },
                { "name": "expiresAt", "type": "u128" }
            ]
        },
        "primaryType": "MorosAccountResolve",
        "domain": {
            "name": "Moros",
            "version": "1",
            "chainId": chain_id,
            "revision": "1"
        },
        "message": {
            "wallet": challenge.wallet_address,
            "linkedVia": challenge.linked_via,
            "mode": if challenge.make_primary { "primary" } else { "secondary" },
            "challenge": challenge.challenge_id,
            "expiresAt": challenge.expires_at_unix.to_string()
        }
    }))
}

fn build_rewards_claim_typed_data(
    chain_id: &str,
    challenge: &RewardsClaimChallengeState,
) -> anyhow::Result<TypedData> {
    serde_json::from_value(build_rewards_claim_typed_data_value(chain_id, challenge)?)
        .map_err(Into::into)
}

fn build_rewards_claim_typed_data_value(
    chain_id: &str,
    challenge: &RewardsClaimChallengeState,
) -> anyhow::Result<Value> {
    Ok(serde_json::json!({
        "types": {
            "StarknetDomain": [
                { "name": "name", "type": "shortstring" },
                { "name": "version", "type": "shortstring" },
                { "name": "chainId", "type": "shortstring" },
                { "name": "revision", "type": "shortstring" }
            ],
            "MorosRewardsClaim": [
                { "name": "wallet", "type": "ContractAddress" },
                { "name": "rewardKind", "type": "shortstring" },
                { "name": "claimId", "type": "string" },
                { "name": "amount", "type": "u128" },
                { "name": "challenge", "type": "felt" },
                { "name": "expiresAt", "type": "u128" }
            ]
        },
        "primaryType": "MorosRewardsClaim",
        "domain": {
            "name": "Moros",
            "version": "1",
            "chainId": chain_id,
            "revision": "1"
        },
        "message": {
            "wallet": challenge.wallet_address,
            "rewardKind": challenge.reward_kind,
            "claimId": challenge.claim_id,
            "amount": challenge.amount_raw,
            "challenge": challenge.challenge_id,
            "expiresAt": challenge.expires_at_unix.to_string()
        }
    }))
}

fn build_referrer_link_typed_data(
    chain_id: &str,
    challenge: &ReferrerLinkChallengeState,
) -> anyhow::Result<TypedData> {
    serde_json::from_value(build_referrer_link_typed_data_value(chain_id, challenge)?)
        .map_err(Into::into)
}

fn build_referrer_link_typed_data_value(
    chain_id: &str,
    challenge: &ReferrerLinkChallengeState,
) -> anyhow::Result<Value> {
    Ok(serde_json::json!({
        "types": {
            "StarknetDomain": [
                { "name": "name", "type": "shortstring" },
                { "name": "version", "type": "shortstring" },
                { "name": "chainId", "type": "shortstring" },
                { "name": "revision", "type": "shortstring" }
            ],
            "MorosReferrerLink": [
                { "name": "wallet", "type": "ContractAddress" },
                { "name": "referrer", "type": "string" },
                { "name": "challenge", "type": "felt" },
                { "name": "expiresAt", "type": "u128" }
            ]
        },
        "primaryType": "MorosReferrerLink",
        "domain": {
            "name": "Moros",
            "version": "1",
            "chainId": chain_id,
            "revision": "1"
        },
        "message": {
            "wallet": challenge.wallet_address,
            "referrer": challenge.referrer,
            "challenge": challenge.challenge_id,
            "expiresAt": challenge.expires_at_unix.to_string()
        }
    }))
}

fn parse_signature(values: &[String]) -> Result<Vec<Felt>, ApiError> {
    if values.is_empty() {
        return Err(ApiError::bad_request("signature is required"));
    }

    values
        .iter()
        .map(|value| {
            felt_from_dec_or_hex(value).map_err(|_| ApiError::bad_request("signature is invalid"))
        })
        .collect()
}

fn parse_wager_u128(value: &str, label: &str) -> Result<u128, ApiError> {
    value
        .parse::<u128>()
        .map_err(|error| ApiError::bad_request(format!("invalid {label}: {error}")))
}

fn signature_verification_error(error: anyhow::Error) -> ApiError {
    if is_contract_not_found_error(&error) {
        return ApiError::bad_request(
            "Moros execution wallet is not deployed yet. Complete one-time wallet setup before authorizing gameplay.",
        );
    }

    ApiError::internal(error)
}

fn is_contract_not_found_error(error: &anyhow::Error) -> bool {
    let message = error.to_string().to_ascii_lowercase();
    message.contains("contractnotfound")
        || message.contains("contract not found")
        || message.contains("contract_not_found")
}

fn map_financial_error(error: anyhow::Error) -> ApiError {
    let message = error.to_string();
    if message.contains("insufficient")
        || message.contains("must be greater than zero")
        || message.contains("is required")
        || message.contains("unsupported balance")
    {
        return ApiError::bad_request(message);
    }
    ApiError::internal(error)
}

fn map_reward_error(error: anyhow::Error) -> ApiError {
    let message = error.to_string();
    if message.contains("invalid")
        || message.contains("not found")
        || message.contains("already been set")
        || message.contains("already being")
        || message.contains("does not match")
        || message.contains("expired")
        || message.contains("coupon")
        || message.contains("cap reached")
        || message.contains("already redeemed")
        || message.contains("reservation failed")
        || message.contains("no ")
        || message.contains("self-referrals")
    {
        return ApiError::bad_request(message);
    }
    ApiError::internal(error)
}

fn parse_user_id(value: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(value).map_err(|_| ApiError::bad_request("invalid Moros user id"))
}

fn require_admin_token(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    let expected = state
        .admin_token
        .as_deref()
        .ok_or_else(|| ApiError::service_unavailable("admin token is not configured"))?;
    let provided = headers
        .get("x-moros-admin-token")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| ApiError::unauthorized("missing x-moros-admin-token"))?;
    if provided != expected {
        return Err(ApiError::unauthorized("invalid x-moros-admin-token"));
    }
    Ok(())
}

fn has_valid_admin_token(state: &AppState, headers: &HeaderMap) -> Result<bool, ApiError> {
    let Some(expected) = state.admin_token.as_deref() else {
        return Ok(false);
    };
    let Some(provided) = headers
        .get("x-moros-admin-token")
        .and_then(|value| value.to_str().ok())
    else {
        return Ok(false);
    };
    if provided != expected {
        return Err(ApiError::unauthorized("invalid x-moros-admin-token"));
    }
    Ok(true)
}

async fn resolve_existing_account_by_user_id(
    database: &PgPool,
    user_id: &str,
) -> Result<accounts::PlayerAccountRecord, ApiError> {
    let player_id = parse_user_id(user_id)?;
    accounts::get_player_account(database, player_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Moros account not found"))
}

async fn resolve_existing_account_by_wallet(
    database: &PgPool,
    wallet_address: &str,
) -> Result<accounts::PlayerAccountRecord, ApiError> {
    let normalized_wallet = normalize_wallet_address(wallet_address)?;
    accounts::resolve_player_account_by_wallet(database, &normalized_wallet)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Moros account not found"))
}

async fn resolve_rewards_account_wallet(
    database: &PgPool,
    user_id: Option<&str>,
    wallet_address: Option<&str>,
) -> Result<(accounts::PlayerAccountRecord, String), ApiError> {
    let account = if let Some(user_id) = user_id {
        resolve_existing_account_by_user_id(database, user_id).await?
    } else {
        let wallet_address = wallet_address
            .ok_or_else(|| ApiError::bad_request("user id or wallet address is required"))?;
        let normalized_wallet = normalize_wallet_address(wallet_address)?;
        accounts::resolve_player_account_by_wallet(database, &normalized_wallet)
            .await
            .map_err(ApiError::internal)?
            .ok_or_else(|| ApiError::not_found("Moros account not found"))?
    };
    let resolved_wallet = account.wallet_address.as_deref().ok_or_else(|| {
        ApiError::bad_request("Moros account does not have an execution wallet linked")
    })?;
    let resolved_wallet = normalize_wallet_address(resolved_wallet)?;
    if let Some(wallet_address) = wallet_address {
        let requested_wallet = normalize_wallet_address(wallet_address)?;
        if !addresses_match(&resolved_wallet, &requested_wallet) {
            return Err(ApiError::bad_request(
                "wallet address does not match Moros account",
            ));
        }
    }
    Ok((account, resolved_wallet))
}

async fn require_authorized_account_access(
    state: &AppState,
    headers: &HeaderMap,
    user_id: Option<&str>,
    wallet_address: Option<&str>,
) -> Result<(accounts::PlayerAccountRecord, Uuid), ApiError> {
    let database = state
        .database
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("database is not configured"))?;
    let account = if let Some(user_id) = user_id {
        resolve_existing_account_by_user_id(database, user_id).await?
    } else if let Some(wallet_address) = wallet_address {
        resolve_existing_account_by_wallet(database, wallet_address).await?
    } else {
        return Err(ApiError::bad_request(
            "user_id or wallet_address is required",
        ));
    };
    if !has_valid_admin_token(state, headers)? {
        let redis = state
            .redis
            .as_ref()
            .ok_or_else(|| ApiError::service_unavailable("redis is not configured"))?;
        let gameplay_session = require_gameplay_session(redis, headers).await?;
        let account_wallet = account.wallet_address.as_deref().ok_or_else(|| {
            ApiError::unauthorized("Moros account has no linked execution wallet")
        })?;
        if !addresses_match(&gameplay_session.wallet_address, account_wallet) {
            return Err(ApiError::unauthorized(
                "gameplay session does not match Moros account",
            ));
        }
    }
    let player_id = Uuid::parse_str(&account.user_id)
        .map_err(|error| ApiError::internal(anyhow::anyhow!(error)))?;
    Ok((account, player_id))
}

async fn require_authorized_hand_access(
    state: &AppState,
    headers: &HeaderMap,
    record: &persistence::BlackjackHandRecord,
) -> Result<(), ApiError> {
    if has_valid_admin_token(state, headers)? {
        return Ok(());
    }
    let redis = state
        .redis
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("redis is not configured"))?;
    let gameplay_session = require_gameplay_session(redis, headers).await?;
    if !addresses_match(&gameplay_session.wallet_address, &record.player) {
        return Err(ApiError::unauthorized(
            "gameplay session does not match blackjack hand",
        ));
    }
    Ok(())
}

async fn ensure_account_from_identity(
    database: &PgPool,
    wallet_address: Option<&str>,
    auth_provider: Option<&str>,
    auth_subject: Option<&str>,
    linked_via: &str,
    make_primary: bool,
) -> Result<accounts::PlayerAccountRecord, ApiError> {
    let wallet_address = wallet_address.unwrap_or_default().to_string();
    let auth_provider = auth_provider
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    let auth_subject = auth_subject
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    if wallet_address.trim().is_empty() && (auth_provider.is_none() || auth_subject.is_none()) {
        return Err(ApiError::bad_request(
            "wallet address or auth identity is required",
        ));
    }
    accounts::ensure_player_account(
        database,
        accounts::EnsurePlayerAccountInput {
            wallet_address,
            auth_provider,
            auth_subject,
            linked_via: Some(linked_via.to_string()),
            make_primary,
        },
    )
    .await
    .map_err(ApiError::internal)
}

async fn fetch_table_state_with_internal_balance(
    database: &PgPool,
    chain: &ChainService,
    table_id: u64,
    player_wallet: Option<&str>,
) -> anyhow::Result<BlackjackTableState> {
    let mut live_state = chain.fetch_blackjack_table_state(table_id, None).await?;
    let Some(player_wallet) = player_wallet else {
        return Ok(live_state);
    };
    let Some(player_id) = accounts::resolve_player_id_by_wallet(database, player_wallet).await?
    else {
        live_state.player_balance = Some("0".to_string());
        live_state.player_fully_covered_max_wager = Some("0".to_string());
        return Ok(live_state);
    };
    if let Some(account) = accounts::get_player_account(database, player_id).await? {
        if let Err(error) = reconcile_account_balance_from_chain(database, chain, &account).await {
            tracing::warn!(
                error = ?error,
                user_id = %account.user_id,
                "failed to reconcile table-state balance from onchain vault; using cached balance"
            );
        }
    }
    let balance = balances::get_balance_account(database, player_id).await?;
    let available = balance
        .as_ref()
        .map(|value| value.gambling_balance.parse::<u128>().unwrap_or(0))
        .unwrap_or(0);
    let max_wager = live_state.table.max_wager.parse::<u128>().unwrap_or(0);
    let house_dynamic_max = live_state
        .fully_covered_max_wager
        .parse::<u128>()
        .unwrap_or(0);
    let exposure_factor = match live_state.table.game_kind.as_str() {
        "blackjack" => 8,
        "roulette" => 1,
        "baccarat" => 1,
        "dice" => 1,
        _ => 1,
    };
    live_state.player_balance = Some(available.to_string());
    live_state.player_fully_covered_max_wager = Some(
        (available / exposure_factor)
            .min(max_wager)
            .min(house_dynamic_max)
            .to_string(),
    );
    Ok(live_state)
}

async fn reconcile_account_balance_from_chain(
    database: &PgPool,
    chain: &ChainService,
    account: &accounts::PlayerAccountRecord,
) -> anyhow::Result<Option<balances::BalanceAccountRecord>> {
    let Some(wallet_address) = account.wallet_address.as_deref() else {
        return Ok(None);
    };
    let player_id = Uuid::parse_str(&account.user_id).context("invalid canonical user id")?;
    let onchain = chain.fetch_player_vault_balances(wallet_address).await?;
    let gambling_balance = onchain
        .gambling_balance
        .parse::<u128>()
        .context("invalid onchain gambling balance")?;
    let vault_balance = onchain
        .vault_balance
        .parse::<u128>()
        .context("invalid onchain vault balance")?;
    let updated = balances::reconcile_onchain_vault_balances(
        database,
        player_id,
        gambling_balance,
        vault_balance,
        Some("bankroll_vault"),
        Some(wallet_address),
        serde_json::json!({
            "player": onchain.player,
            "source": "read_through_reconciliation",
        }),
    )
    .await?;
    Ok(Some(updated))
}

async fn load_account_balance_snapshot(
    database: &PgPool,
    chain: Option<&ChainService>,
    account: &accounts::PlayerAccountRecord,
    player_id: Uuid,
) -> Result<balances::BalanceAccountRecord, ApiError> {
    if let Some(chain) = chain {
        match reconcile_account_balance_from_chain(database, chain, account).await {
            Ok(Some(balance)) => return Ok(balance),
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(
                    error = ?error,
                    user_id = %account.user_id,
                    "failed to reconcile account balance from onchain vault; returning cached balance"
                );
            }
        }
    }

    let balance = balances::get_balance_account(database, player_id)
        .await
        .map_err(ApiError::internal)?
        .unwrap_or(balances::BalanceAccountRecord {
            user_id: account.user_id.clone(),
            gambling_balance: "0".to_string(),
            gambling_reserved: "0".to_string(),
            vault_balance: "0".to_string(),
            updated_at: "0".to_string(),
        });
    Ok(balance)
}

async fn cache_profile_claim_challenge(
    redis: &RedisClient,
    challenge: &ProfileClaimChallengeState,
) -> anyhow::Result<()> {
    let ttl_seconds = if challenge.expires_at_unix <= runtime::now_unix() {
        60
    } else {
        (challenge.expires_at_unix - runtime::now_unix()) as u64
    };
    let payload =
        serde_json::to_string(challenge).context("failed to serialize profile claim challenge")?;
    let key = profile_claim_challenge_key(&challenge.challenge_id);
    let mut connection = redis
        .get_multiplexed_async_connection()
        .await
        .context("failed to open redis connection")?;
    let _: () = connection
        .set_ex(key, payload, ttl_seconds)
        .await
        .context("failed to cache profile claim challenge")?;
    Ok(())
}

async fn get_profile_claim_challenge(
    redis: &RedisClient,
    challenge_id: &str,
) -> anyhow::Result<Option<ProfileClaimChallengeState>> {
    let key = profile_claim_challenge_key(challenge_id);
    let mut connection = redis
        .get_multiplexed_async_connection()
        .await
        .context("failed to open redis connection")?;
    let payload: Option<String> = connection
        .get::<_, Option<String>>(key)
        .await
        .context("failed to read profile claim challenge")?;

    payload
        .map(|payload| {
            serde_json::from_str::<ProfileClaimChallengeState>(&payload)
                .context("failed to deserialize profile claim challenge")
        })
        .transpose()
}

async fn delete_profile_claim_challenge(
    redis: &RedisClient,
    challenge_id: &str,
) -> anyhow::Result<()> {
    let key = profile_claim_challenge_key(challenge_id);
    let mut connection = redis
        .get_multiplexed_async_connection()
        .await
        .context("failed to open redis connection")?;
    let _: () = connection
        .del(key)
        .await
        .context("failed to delete profile claim challenge")?;
    Ok(())
}

fn profile_claim_challenge_key(challenge_id: &str) -> String {
    format!("moros:profile-claim:{challenge_id}")
}

fn account_resolve_challenge_key(challenge_id: &str) -> String {
    format!("moros:account-resolve:{challenge_id}")
}

fn rewards_claim_challenge_key(challenge_id: &str) -> String {
    format!("moros:rewards-claim:{challenge_id}")
}

fn referrer_link_challenge_key(challenge_id: &str) -> String {
    format!("moros:referrer-link:{challenge_id}")
}

async fn cache_gameplay_session_challenge(
    redis: &RedisClient,
    challenge: &GameplaySessionChallengeState,
) -> anyhow::Result<()> {
    cache_json_with_ttl(
        redis,
        gameplay_session_challenge_key(&challenge.challenge_id),
        challenge,
        challenge.expires_at_unix,
        "gameplay session challenge",
    )
    .await
}

async fn get_gameplay_session_challenge(
    redis: &RedisClient,
    challenge_id: &str,
) -> anyhow::Result<Option<GameplaySessionChallengeState>> {
    read_cached_json(
        redis,
        gameplay_session_challenge_key(challenge_id),
        "gameplay session challenge",
    )
    .await
}

async fn delete_gameplay_session_challenge(
    redis: &RedisClient,
    challenge_id: &str,
) -> anyhow::Result<()> {
    delete_cached_key(
        redis,
        gameplay_session_challenge_key(challenge_id),
        "gameplay session challenge",
    )
    .await
}

async fn cache_gameplay_session(
    redis: &RedisClient,
    session: &GameplaySessionState,
) -> anyhow::Result<()> {
    cache_json_with_ttl(
        redis,
        gameplay_session_key(&session.session_token),
        session,
        session.expires_at_unix,
        "gameplay session",
    )
    .await
}

async fn get_gameplay_session(
    redis: &RedisClient,
    session_token: &str,
) -> anyhow::Result<Option<GameplaySessionState>> {
    read_cached_json(
        redis,
        gameplay_session_key(session_token),
        "gameplay session",
    )
    .await
}

async fn cache_json_with_ttl<T: Serialize>(
    redis: &RedisClient,
    key: String,
    value: &T,
    expires_at_unix: i64,
    label: &str,
) -> anyhow::Result<()> {
    let ttl_seconds = if expires_at_unix <= runtime::now_unix() {
        60
    } else {
        (expires_at_unix - runtime::now_unix()) as u64
    };
    let payload =
        serde_json::to_string(value).with_context(|| format!("failed to serialize {label}"))?;
    let mut connection = redis
        .get_multiplexed_async_connection()
        .await
        .context("failed to open redis connection")?;
    let _: () = connection
        .set_ex(key, payload, ttl_seconds)
        .await
        .with_context(|| format!("failed to cache {label}"))?;
    Ok(())
}

async fn read_cached_json<T: for<'de> Deserialize<'de>>(
    redis: &RedisClient,
    key: String,
    label: &str,
) -> anyhow::Result<Option<T>> {
    let mut connection = redis
        .get_multiplexed_async_connection()
        .await
        .context("failed to open redis connection")?;
    let payload: Option<String> = connection
        .get::<_, Option<String>>(key)
        .await
        .with_context(|| format!("failed to read {label}"))?;
    payload
        .map(|payload| {
            serde_json::from_str::<T>(&payload)
                .with_context(|| format!("failed to deserialize {label}"))
        })
        .transpose()
}

async fn delete_cached_key(redis: &RedisClient, key: String, label: &str) -> anyhow::Result<()> {
    let mut connection = redis
        .get_multiplexed_async_connection()
        .await
        .context("failed to open redis connection")?;
    let _: () = connection
        .del(key)
        .await
        .with_context(|| format!("failed to delete {label}"))?;
    Ok(())
}

fn gameplay_session_challenge_key(challenge_id: &str) -> String {
    format!("moros:gameplay-session-challenge:{challenge_id}")
}

fn gameplay_session_key(session_token: &str) -> String {
    format!("moros:gameplay-session:{session_token}")
}

async fn require_gameplay_session(
    redis: &RedisClient,
    headers: &HeaderMap,
) -> Result<GameplaySessionState, ApiError> {
    let authorization = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| ApiError::unauthorized("gameplay session is required"))?;
    let token = authorization
        .strip_prefix("Bearer ")
        .or_else(|| authorization.strip_prefix("bearer "))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::unauthorized("gameplay session is required"))?;
    let session = get_gameplay_session(redis, token)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::unauthorized("gameplay session is invalid or expired"))?;
    if session.expires_at_unix <= runtime::now_unix() {
        return Err(ApiError::unauthorized(
            "gameplay session is invalid or expired",
        ));
    }
    Ok(session)
}

fn ensure_table_is_bettable(live_state: &BlackjackTableState, wager: &str) -> Result<(), ApiError> {
    if live_state.table.status != "active" {
        return Err(ApiError::bad_request(format!(
            "table {} is not active: {}",
            live_state.table.table_id, live_state.table.status
        )));
    }
    let wager = parse_wager_u128(wager, "wager")?;
    let min_wager = parse_wager_u128(&live_state.table.min_wager, "table min_wager")?;
    let max_wager = parse_wager_u128(&live_state.table.max_wager, "table max_wager")?;
    if wager < min_wager || wager > max_wager {
        return Err(ApiError::bad_request(format!(
            "wager must be between {} and {} wei",
            min_wager, max_wager
        )));
    }
    Ok(())
}

fn ensure_player_bankroll(live_state: &BlackjackTableState, wager: &str) -> Result<(), ApiError> {
    let wager = parse_wager_u128(wager, "wager")?;
    let player_balance = live_state
        .player_balance
        .as_deref()
        .unwrap_or("0")
        .parse::<u128>()
        .map_err(ApiError::internal)?;
    if player_balance < wager {
        return Err(ApiError::bad_request(format!(
            "player bankroll is too low for this wager: have {} wei, need {} wei",
            player_balance, wager
        )));
    }
    Ok(())
}

async fn ensure_operator_gameplay_session(
    chain: &ChainService,
    player: &str,
    gameplay_session_expires_at: i64,
    live_state: &BlackjackTableState,
) -> anyhow::Result<()> {
    let session_key = chain.operator_address_hex();
    let current = chain.fetch_session_grant(player, &session_key).await?;
    let current_max_wager = current.max_wager.parse::<u128>().unwrap_or(0);
    let required_max_wager = live_state.table.max_wager.parse::<u128>().unwrap_or(0);
    let required_expires_at = u64::try_from(gameplay_session_expires_at.max(0)).unwrap_or(0);
    if current.active
        && current_max_wager >= required_max_wager
        && current.expires_at >= required_expires_at
        && addresses_match(&current.player, player)
        && addresses_match(&current.session_key, &session_key)
    {
        return Ok(());
    }
    anyhow::bail!(
        "onchain gameplay session grant is missing or expired for player {} and session key {}",
        player,
        session_key,
    )
}

fn parse_u128(value: &str) -> u128 {
    value.parse::<u128>().unwrap_or(0)
}

fn multiplier_bps(wager: &str, payout: &str) -> String {
    let wager = parse_u128(wager);
    let payout = parse_u128(payout);
    if wager == 0 {
        return "0".to_string();
    }
    payout
        .saturating_mul(10_000)
        .checked_div(wager)
        .unwrap_or(0)
        .to_string()
}

fn addresses_match(left: &str, right: &str) -> bool {
    match (felt_from_dec_or_hex(left), felt_from_dec_or_hex(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => left.eq_ignore_ascii_case(right),
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(serde_json::json!({
                "error": self.message,
            })),
        )
            .into_response()
    }
}
