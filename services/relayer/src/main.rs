use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use moros_common::{
    blackjack,
    chain::ChainService,
    config::ServiceConfig,
    infra::{InfraSnapshot, ServiceInfra},
    persistence, runtime, telemetry,
    web::base_router,
};
use redis::Client as RedisClient;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use uuid::Uuid;

#[derive(Clone)]
struct AppState {
    service: &'static str,
    infra: InfraSnapshot,
    database: Option<PgPool>,
    redis: Option<RedisClient>,
    chain: Option<ChainService>,
}

#[derive(Debug, Deserialize)]
struct RelayActionRequest {
    hand_id: String,
    action: String,
    #[serde(alias = "session_key")]
    relay_token: String,
}

#[derive(Debug, Serialize)]
struct RelayActionResponse {
    relay_id: String,
    status: &'static str,
    hand: moros_common::blackjack::BlackjackHandView,
}

#[derive(Debug, Deserialize)]
struct BlackjackTimeoutRequest {
    hand_id: String,
    action: String,
}

#[derive(Debug, Serialize)]
struct BlackjackTimeoutResponse {
    relay_id: String,
    status: &'static str,
    action: String,
    hand_id: String,
    chain_hand_id: u64,
}

#[derive(Debug, Serialize)]
struct RootResponse {
    service: &'static str,
    role: &'static str,
    status: &'static str,
    infra: InfraSnapshot,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

type ApiResult<T> = Result<Json<T>, ApiError>;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    telemetry::init("moros_relayer");
    let config = ServiceConfig::from_env("moros-relayer", 8082);
    let infra = ServiceInfra::from_config(&config)?;
    let readiness = infra.prepare().await?;
    let state = Arc::new(AppState {
        service: "moros-relayer",
        infra: infra.snapshot(&config, readiness),
        database: infra.database.clone(),
        redis: infra.redis.clone(),
        chain: ChainService::from_config(&config)?,
    });

    let app = base_router::<Arc<AppState>>("moros-relayer")
        .route("/", get(root))
        .route("/v1/actions", post(relay_action))
        .route("/v1/blackjack/timeouts", post(handle_blackjack_timeout))
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
        role: "gameplay relayer",
        status: "ready",
        infra: state.infra.clone(),
    }))
}

async fn relay_action(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RelayActionRequest>,
) -> ApiResult<RelayActionResponse> {
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
    let hand_id = Uuid::parse_str(&payload.hand_id)
        .map_err(|error| ApiError::bad_request(format!("invalid hand_id: {error}")))?;

    let session = runtime::get_session_by_hand(redis, hand_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("session runtime not found"))?;
    if session.relay_token != payload.relay_token {
        return Err(ApiError::bad_request(
            "relay token does not match active runtime",
        ));
    }
    if !session
        .allowed_actions
        .iter()
        .any(|allowed| allowed == &payload.action)
    {
        return Err(ApiError::bad_request(
            "action is not allowed in the active runtime",
        ));
    }

    let snapshot = persistence::get_blackjack_snapshot(database, hand_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("hand snapshot not found"))?;
    let record = persistence::get_blackjack_hand(database, hand_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("hand not found"))?;
    let chain_hand_id = record
        .chain_hand_id
        .ok_or_else(|| ApiError::not_found("chain hand is not attached"))?
        as u64;

    let (next_snapshot, action_plan) =
        blackjack::plan_action_submission(&snapshot, &payload.action)
            .map_err(ApiError::internal)?;
    let dealer_blackjack =
        snapshot.transcript_artifact.dealer_peek.outcome.as_str() == "dealer_blackjack";
    let action_result = match action_plan.action.as_str() {
        "hit" => chain
            .submit_hit_verified(
                &record.player,
                chain_hand_id,
                action_plan.seat_index,
                *action_plan.player_draws.first().ok_or_else(|| {
                    ApiError::internal(anyhow::anyhow!("hit is missing drawn card"))
                })?,
                action_plan.player_draw_proofs.first().ok_or_else(|| {
                    ApiError::internal(anyhow::anyhow!("hit is missing drawn-card proof"))
                })?,
            )
            .await
            .map(|_| ()),
        "stand" => chain
            .submit_stand(&record.player, chain_hand_id, action_plan.seat_index)
            .await
            .map(|_| ()),
        "double" => chain
            .submit_double_verified(
                &record.player,
                chain_hand_id,
                action_plan.seat_index,
                *action_plan.player_draws.first().ok_or_else(|| {
                    ApiError::internal(anyhow::anyhow!("double is missing drawn card"))
                })?,
                action_plan.player_draw_proofs.first().ok_or_else(|| {
                    ApiError::internal(anyhow::anyhow!("double is missing drawn-card proof"))
                })?,
            )
            .await
            .map(|_| ()),
        "split" => chain
            .submit_split_verified(
                &record.player,
                chain_hand_id,
                action_plan.seat_index,
                *action_plan.player_draws.first().ok_or_else(|| {
                    ApiError::internal(anyhow::anyhow!("split left draw missing"))
                })?,
                action_plan.player_draw_proofs.first().ok_or_else(|| {
                    ApiError::internal(anyhow::anyhow!("split left proof missing"))
                })?,
                *action_plan.player_draws.get(1).ok_or_else(|| {
                    ApiError::internal(anyhow::anyhow!("split right draw missing"))
                })?,
                action_plan.player_draw_proofs.get(1).ok_or_else(|| {
                    ApiError::internal(anyhow::anyhow!("split right proof missing"))
                })?,
            )
            .await
            .map(|_| ()),
        "take_insurance" => chain
            .submit_take_insurance(&record.player, chain_hand_id, dealer_blackjack)
            .await
            .map(|_| ()),
        "decline_insurance" => chain
            .submit_decline_insurance(&record.player, chain_hand_id, dealer_blackjack)
            .await
            .map(|_| ()),
        other => {
            return Err(ApiError::bad_request(format!(
                "unsupported action: {other}"
            )));
        }
    };
    if let Err(error) = action_result {
        return Err(ApiError::internal(error));
    }

    if action_plan.should_finalize {
        chain
            .wait_for_hand_state(
                chain_hand_id,
                "awaiting_dealer",
                "dealer_turn",
                next_snapshot.action_count,
                next_snapshot.seat_count,
                1,
            )
            .await
            .map_err(ApiError::internal)?;
        for (index, card) in action_plan.dealer_reveals.iter().enumerate() {
            let proof = action_plan.dealer_reveal_proofs.get(index).ok_or_else(|| {
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
                    next_snapshot.action_count,
                    next_snapshot.seat_count,
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
            &next_snapshot.status,
            &next_snapshot.phase,
            next_snapshot.action_count,
            next_snapshot.seat_count,
            if action_plan.should_finalize {
                action_plan.dealer_reveals.len() + 1
            } else {
                1
            },
        )
        .await
        .map_err(ApiError::internal)?;
    persistence::store_blackjack_snapshot(database, hand_id, &next_snapshot)
        .await
        .map_err(ApiError::internal)?;
    persistence::append_blackjack_event(
        database,
        hand_id,
        Uuid::parse_str(&session.session_id).ok(),
        &format!("player.{}", payload.action),
        serde_json::json!({
            "action": payload.action,
            "phase": next_snapshot.phase,
            "status": next_snapshot.status,
            "chain_hand_id": chain_hand_id,
        }),
    )
    .await
    .map_err(ApiError::internal)?;
    let view = blackjack::reconcile_view_with_chain(&next_snapshot, &chain_hand)
        .map_err(ApiError::internal)?;

    runtime::cache_session(
        redis,
        &runtime::SessionRuntimeState {
            session_id: session.session_id.clone(),
            hand_id: session.hand_id.clone(),
            player: session.player.clone(),
            relay_token: session.relay_token.clone(),
            table_id: session.table_id,
            transcript_root: session.transcript_root.clone(),
            status: view.status.clone(),
            phase: view.phase.clone(),
            allowed_actions: view.allowed_actions.clone(),
            expires_at_unix: session.expires_at_unix,
        },
    )
    .await
    .map_err(ApiError::internal)?;

    let relay_id = Uuid::now_v7().to_string();
    Ok(Json(RelayActionResponse {
        relay_id,
        status: "completed",
        hand: view,
    }))
}

async fn handle_blackjack_timeout(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<BlackjackTimeoutRequest>,
) -> ApiResult<BlackjackTimeoutResponse> {
    let database = state
        .database
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("database is not configured"))?;
    let chain = state
        .chain
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("starknet chain is not configured"))?;
    let hand_id = Uuid::parse_str(&payload.hand_id)
        .map_err(|error| ApiError::bad_request(format!("invalid hand_id: {error}")))?;

    let record = persistence::get_blackjack_hand(database, hand_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("hand not found"))?;
    let snapshot = persistence::get_blackjack_snapshot(database, hand_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("hand snapshot not found"))?;
    let chain_hand_id = record
        .chain_hand_id
        .ok_or_else(|| ApiError::not_found("chain hand is not attached"))?
        as u64;
    let (next_snapshot, timeout_plan) =
        blackjack::plan_timeout_submission(&snapshot, &payload.action)
            .map_err(|error| ApiError::bad_request(error.to_string()))?;

    match timeout_plan.action.as_str() {
        "force_insurance_decline" => chain
            .force_expired_insurance_decline(chain_hand_id)
            .await
            .map(|_| ()),
        "force_stand" => chain.force_expired_stand(chain_hand_id).await.map(|_| ()),
        "void_expired_hand" => chain
            .void_expired_blackjack_hand(chain_hand_id)
            .await
            .map(|_| ()),
        other => Err(anyhow::anyhow!("unsupported timeout action: {other}")),
    }
    .map_err(ApiError::internal)?;

    let chain_hand = chain
        .wait_for_hand(chain_hand_id, |hand| {
            hand.status == next_snapshot.status
                && hand.phase == next_snapshot.phase
                && hand.action_count >= next_snapshot.action_count
        })
        .await
        .map_err(ApiError::internal)?;
    persistence::store_blackjack_snapshot(database, hand_id, &next_snapshot)
        .await
        .map_err(ApiError::internal)?;
    persistence::append_blackjack_event(
        database,
        hand_id,
        None,
        &format!("timeout.{}", timeout_plan.action),
        serde_json::json!({
            "action": timeout_plan.action,
            "chain_hand_id": chain_hand_id,
            "chain_status": chain_hand.status,
            "chain_phase": chain_hand.phase,
        }),
    )
    .await
    .map_err(ApiError::internal)?;

    if let Some(redis) = &state.redis {
        if let Ok(Some(session)) = runtime::get_session_by_hand(redis, hand_id).await {
            if let Ok(view) = blackjack::reconcile_view_with_chain(&next_snapshot, &chain_hand) {
                let _ = runtime::cache_session(
                    redis,
                    &runtime::SessionRuntimeState {
                        session_id: session.session_id,
                        hand_id: session.hand_id,
                        player: session.player,
                        relay_token: session.relay_token,
                        table_id: session.table_id,
                        transcript_root: session.transcript_root,
                        status: view.status,
                        phase: view.phase,
                        allowed_actions: view.allowed_actions,
                        expires_at_unix: session.expires_at_unix,
                    },
                )
                .await;
            }
        }
    }

    Ok(Json(BlackjackTimeoutResponse {
        relay_id: Uuid::now_v7().to_string(),
        status: "completed",
        action: timeout_plan.action,
        hand_id: hand_id.to_string(),
        chain_hand_id,
    }))
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
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
        tracing::error!(error = ?error, "relayer request failed");
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: error.to_string(),
        }
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
