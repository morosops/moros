use anyhow::{Context, anyhow};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use ed25519_dalek::SigningKey as SolanaSigningKey;
use hex::FromHex;
use k256::{SecretKey, elliptic_curve::sec1::ToEncodedPoint};
use moros_common::{
    accounts::{self, EnsurePlayerAccountInput},
    config::ServiceConfig,
    deposits::{
        self, CreateDepositChannelInput, CreateDepositRecoveryInput, DepositChannelRecord,
        DepositRouteJobRecord, DepositSupportedAssetRecord, DepositSupportedAssetSeed,
        DepositTransferRecord, ObserveDepositTransferInput, ResolveDepositRecoveryInput,
        ResolveRiskFlagInput,
    },
    infra::{InfraSnapshot, ServiceInfra},
    telemetry,
    web::base_router,
};
use num_bigint::BigUint;
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha3::{Digest, Keccak256};
use sqlx::PgPool;
use starknet::{
    core::{
        types::Felt,
        utils::{get_contract_address, get_selector_from_name},
    },
    signers::SigningKey,
};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use uuid::Uuid;

const EVM_TRANSFER_TOPIC: &str = "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55aebfb3ef";
const STARKNET_OPENZEPPELIN_CLASS_HASH: &str =
    "0x01d1777db36cdd06dd62cfde77b1b6ae06412af95d57a13dc40ac77b8a702381";
const STARKNET_CURVE_ORDER_HEX: &str =
    "0800000000000011000000000000000000000000000000000000000000000001";
const STARKNET_EVENT_CHUNK_SIZE: u64 = 128;

#[derive(Clone)]
struct AppState {
    service: &'static str,
    infra: InfraSnapshot,
    database: Option<PgPool>,
    http: HttpClient,
    config: DepositRouterConfig,
}

#[derive(Debug, Clone)]
struct DepositRouterConfig {
    service: ServiceConfig,
    deposit_master_secret: Option<String>,
    route_executor_url: Option<String>,
    route_executor_token: Option<String>,
    admin_token: Option<String>,
    route_starknet_account_address: Option<String>,
    rpc_urls: HashMap<String, String>,
    supported_assets: Vec<DepositSupportedAssetSeed>,
    blocked_senders: HashMap<String, HashSet<String>>,
    watch_interval_ms: u64,
    route_interval_ms: u64,
}

#[derive(Debug, Serialize)]
struct RootResponse {
    service: &'static str,
    role: &'static str,
    status: &'static str,
    infra: InfraSnapshot,
    deposit_master_configured: bool,
    executor_configured: bool,
    supported_asset_count: usize,
}

#[derive(Debug, Deserialize)]
struct CreateDepositRequest {
    wallet_address: String,
    asset_id: String,
    chain_key: String,
}

#[derive(Debug, Serialize)]
struct CreateDepositResponse {
    channel: DepositChannelRecord,
    asset: DepositSupportedAssetRecord,
    status_url: String,
}

#[derive(Debug, Serialize)]
struct DepositStatusResponse {
    channel: DepositChannelRecord,
    transfers: Vec<DepositTransferRecord>,
    route_jobs: Vec<DepositRouteJobRecord>,
    risk_flags: Vec<deposits::DepositRiskFlagRecord>,
    recoveries: Vec<deposits::DepositRecoveryRecord>,
}

fn filter_user_visible_deposit_status(
    transfers: Vec<DepositTransferRecord>,
    route_jobs: Vec<DepositRouteJobRecord>,
    risk_flags: Vec<deposits::DepositRiskFlagRecord>,
    recoveries: Vec<deposits::DepositRecoveryRecord>,
) -> (
    Vec<DepositTransferRecord>,
    Vec<DepositRouteJobRecord>,
    Vec<deposits::DepositRiskFlagRecord>,
    Vec<deposits::DepositRecoveryRecord>,
) {
    let hidden_transfer_ids: HashSet<String> = risk_flags
        .iter()
        .filter(|flag| flag.code == "INTERNAL_ROUTE_WALLET_TRANSFER")
        .map(|flag| flag.transfer_id.clone())
        .collect();

    if hidden_transfer_ids.is_empty() {
        return (transfers, route_jobs, risk_flags, recoveries);
    }

    let transfers = transfers
        .into_iter()
        .filter(|transfer| !hidden_transfer_ids.contains(&transfer.transfer_id))
        .collect();
    let route_jobs = route_jobs
        .into_iter()
        .filter(|job| !hidden_transfer_ids.contains(&job.transfer_id))
        .collect();
    let risk_flags = risk_flags
        .into_iter()
        .filter(|flag| !hidden_transfer_ids.contains(&flag.transfer_id))
        .collect();
    let recoveries = recoveries
        .into_iter()
        .filter(|recovery| !hidden_transfer_ids.contains(&recovery.transfer_id))
        .collect();

    (transfers, route_jobs, risk_flags, recoveries)
}

#[derive(Debug, Deserialize)]
struct ExecutorCallbackRequest {
    status: String,
    response: Option<Value>,
    destination_tx_hash: Option<String>,
    error: Option<String>,
    retryable: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct RouteJobListQuery {
    status: Option<String>,
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
    telemetry::init("moros_deposit_router");
    let config = DepositRouterConfig::from_env()?;
    let bind_address = configure_bind_address(&config);
    let infra = ServiceInfra::from_config(&config.service)?;
    let readiness = infra.prepare().await?;

    if let Some(database) = &infra.database {
        if !config.supported_assets.is_empty() {
            deposits::seed_supported_assets(database, &config.supported_assets).await?;
        }
    }

    let state = Arc::new(AppState {
        service: "moros-deposit-router",
        infra: infra.snapshot(&config.service, readiness),
        database: infra.database.clone(),
        http: HttpClient::builder()
            .timeout(Duration::from_secs(20))
            .build()
            .context("failed to construct HTTP client")?,
        config,
    });

    if state.database.is_some() {
        tokio::spawn(watch_worker(state.clone()));
        tokio::spawn(route_worker(state.clone()));
    }

    let app = base_router::<Arc<AppState>>("moros-deposit-router")
        .route("/", get(root))
        .route("/v1/deposits", post(create_deposit_channel))
        .route("/v1/deposits/supported-assets", get(get_supported_assets))
        .route(
            "/v1/deposits/channels/{channel_id}",
            get(get_deposit_channel),
        )
        .route(
            "/v1/deposits/status/{deposit_address}",
            get(get_deposit_status),
        )
        .route("/v1/deposits/route-jobs", get(list_route_jobs))
        .route(
            "/v1/deposits/route-jobs/{job_id}/retry",
            post(retry_route_job),
        )
        .route("/v1/deposits/risk-flags", get(list_open_risk_flags))
        .route(
            "/v1/deposits/risk-flags/{flag_id}/resolve",
            post(resolve_risk_flag),
        )
        .route(
            "/v1/deposits/transfers/{transfer_id}/recoveries",
            post(create_recovery_request),
        )
        .route("/v1/deposits/recoveries", get(list_recoveries))
        .route(
            "/v1/deposits/recoveries/{recovery_id}",
            get(get_recovery_request),
        )
        .route(
            "/v1/deposits/recoveries/{recovery_id}/resolve",
            post(resolve_recovery_request),
        )
        .route(
            "/v1/deposits/route-jobs/{job_id}/callback",
            post(handle_route_job_callback),
        )
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(bind_address).await?;
    tracing::info!(
        "moros-deposit-router listening on {}",
        listener.local_addr()?
    );
    axum::serve(listener, app).await?;
    Ok(())
}

fn configure_bind_address(config: &DepositRouterConfig) -> String {
    config.service.bind_address()
}

async fn root(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(serde_json::json!(RootResponse {
        service: state.service,
        role: "deposit address issuance, monitoring, routing, and reconciliation",
        status: "ready",
        infra: state.infra.clone(),
        deposit_master_configured: state.config.deposit_master_secret.is_some(),
        executor_configured: state.config.route_executor_url.is_some(),
        supported_asset_count: state.config.supported_assets.len(),
    }))
}

async fn get_supported_assets(
    State(state): State<Arc<AppState>>,
) -> ApiResult<Vec<DepositSupportedAssetRecord>> {
    let database = require_database(&state)?;
    let assets = deposits::list_supported_assets(database)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(
        assets
            .into_iter()
            .filter(|asset| asset.status == "enabled")
            .collect(),
    ))
}

async fn create_deposit_channel(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<CreateDepositRequest>,
) -> ApiResult<CreateDepositResponse> {
    let database = require_database(&state)?;
    let master_secret =
        state.config.deposit_master_secret.as_ref().ok_or_else(|| {
            ApiError::service_unavailable("deposit master secret is not configured")
        })?;
    let asset = deposits::get_supported_asset(database, &payload.asset_id, &payload.chain_key)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("supported asset not found"))?;

    if asset.status != "enabled" {
        return Err(ApiError::bad_request(
            "selected deposit asset is not enabled",
        ));
    }
    let _ = headers;
    let beneficiary_wallet = normalize_starknet_beneficiary_wallet(&payload.wallet_address)
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    let account = accounts::ensure_player_account(
        database,
        EnsurePlayerAccountInput {
            wallet_address: beneficiary_wallet.clone(),
            auth_provider: None,
            auth_subject: None,
            linked_via: Some("deposit".to_string()),
            make_primary: false,
        },
    )
    .await
    .map_err(ApiError::internal)?;
    let player_id =
        Uuid::parse_str(&account.user_id).map_err(|error| ApiError::internal(anyhow!(error)))?;
    let deposit_address = select_deposit_address(
        master_secret,
        &account.user_id,
        &payload.chain_key,
        &asset,
    )
    .map_err(ApiError::internal)?;
    let chain_assets = deposits::list_supported_assets(database)
        .await
        .map_err(ApiError::internal)?
        .into_iter()
        .filter(|candidate| {
            candidate.status == "enabled" && candidate.chain_key == payload.chain_key
        })
        .collect::<Vec<_>>();
    let mut watch_block_by_chain = HashMap::<String, Option<i64>>::new();
    let mut selected_channel = None;
    for chain_asset in chain_assets {
        let watch_from_block = match watch_block_by_chain.get(&chain_asset.chain_key) {
            Some(value) => *value,
            None => {
                let value = fetch_chain_watch_from_block(&state, &chain_asset).await?;
                watch_block_by_chain.insert(chain_asset.chain_key.clone(), value);
                value
            }
        };
        let channel = deposits::get_or_create_deposit_channel(
            database,
            &CreateDepositChannelInput {
                player_id,
                wallet_address: Some(beneficiary_wallet.clone()),
                asset_id: chain_asset.id.clone(),
                chain_key: chain_asset.chain_key.clone(),
                deposit_address: deposit_address.clone(),
                qr_payload: build_deposit_uri(&chain_asset, &deposit_address),
                route_kind: chain_asset.route_kind.clone(),
                watch_from_block,
                last_scanned_block: watch_from_block,
            },
        )
        .await
        .map_err(ApiError::internal)?;
        if chain_asset.id == payload.asset_id {
            selected_channel = Some(channel);
        }
    }
    let channel = selected_channel.ok_or_else(|| {
        ApiError::internal(anyhow!(
            "selected enabled deposit asset was not provisioned for chain"
        ))
    })?;

    Ok(Json(CreateDepositResponse {
        status_url: format!("/v1/deposits/status/{}", channel.deposit_address),
        channel,
        asset,
    }))
}

fn normalize_starknet_beneficiary_wallet(value: &str) -> anyhow::Result<String> {
    let normalized = accounts::normalize_wallet_address(value);
    anyhow::ensure!(
        !normalized.is_empty(),
        "starknet beneficiary wallet address is required"
    );
    anyhow::ensure!(
        normalized.starts_with("0x"),
        "starknet beneficiary wallet must be a 0x-prefixed address"
    );
    let felt =
        Felt::from_hex(&normalized).context("starknet beneficiary wallet is not a valid felt")?;
    anyhow::ensure!(
        felt != Felt::ZERO,
        "starknet beneficiary wallet cannot be 0x0"
    );
    Ok(format!("{felt:#x}"))
}

async fn fetch_chain_watch_from_block(
    state: &AppState,
    asset: &DepositSupportedAssetRecord,
) -> Result<Option<i64>, ApiError> {
    match asset.chain_family.as_str() {
        "evm" => match state.config.source_rpc_url(asset) {
            Some(rpc_url) => Ok(Some(
                fetch_latest_block_number(&state.http, rpc_url)
                    .await
                    .map_err(ApiError::internal)? as i64,
            )),
            None => Ok(None),
        },
        "starknet" => match state.config.source_rpc_url(asset) {
            Some(rpc_url) => Ok(Some(
                fetch_latest_starknet_block_number(&state.http, rpc_url)
                    .await
                    .map_err(ApiError::internal)? as i64,
            )),
            None => Ok(None),
        },
        "solana" => match state.config.source_rpc_url(asset) {
            Some(rpc_url) => Ok(Some(
                fetch_latest_solana_slot(&state.http, rpc_url)
                    .await
                    .map_err(ApiError::internal)? as i64,
            )),
            None => Ok(None),
        },
        _ => Err(ApiError::bad_request(
            "selected deposit asset is not supported in this build",
        )),
    }
}

async fn get_deposit_channel(
    State(state): State<Arc<AppState>>,
    Path(channel_id): Path<String>,
) -> ApiResult<DepositChannelRecord> {
    let database = require_database(&state)?;
    let channel_id = parse_uuid(&channel_id, "channel_id")?;
    let channel = deposits::get_deposit_channel(database, channel_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("deposit channel not found"))?;
    Ok(Json(channel))
}

async fn get_deposit_status(
    State(state): State<Arc<AppState>>,
    Path(deposit_address): Path<String>,
) -> ApiResult<DepositStatusResponse> {
    let database = require_database(&state)?;
    let channel = deposits::get_deposit_channel_by_address(database, &deposit_address)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("deposit channel not found"))?;
    let transfers = deposits::list_transfers_by_deposit_address(database, &deposit_address)
        .await
        .map_err(ApiError::internal)?;
    let route_jobs = deposits::list_route_jobs_by_deposit_address(database, &deposit_address)
        .await
        .map_err(ApiError::internal)?;
    let risk_flags = deposits::list_risk_flags_by_deposit_address(database, &deposit_address)
        .await
        .map_err(ApiError::internal)?;
    let recoveries = deposits::list_recoveries_by_deposit_address(database, &deposit_address)
        .await
        .map_err(ApiError::internal)?;
    let (transfers, route_jobs, risk_flags, recoveries) =
        filter_user_visible_deposit_status(transfers, route_jobs, risk_flags, recoveries);
    Ok(Json(DepositStatusResponse {
        channel,
        transfers,
        route_jobs,
        risk_flags,
        recoveries,
    }))
}

async fn list_route_jobs(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<RouteJobListQuery>,
) -> ApiResult<Vec<DepositRouteJobRecord>> {
    authorize_admin(&state, &headers)?;
    let database = require_database(&state)?;
    let jobs =
        deposits::list_route_jobs(database, query.status.as_deref(), query.limit.unwrap_or(50))
            .await
            .map_err(ApiError::internal)?;
    Ok(Json(jobs))
}

async fn retry_route_job(
    State(state): State<Arc<AppState>>,
    Path(job_id): Path<String>,
    headers: HeaderMap,
) -> ApiResult<DepositRouteJobRecord> {
    authorize_admin(&state, &headers)?;
    let database = require_database(&state)?;
    let job_id = parse_uuid(&job_id, "job_id")?;
    let job = deposits::retry_route_job(database, job_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("route job not found or not retryable"))?;
    Ok(Json(job))
}

async fn list_open_risk_flags(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResult<Vec<moros_common::deposits::DepositRiskFlagRecord>> {
    authorize_admin(&state, &headers)?;
    let database = require_database(&state)?;
    let flags = deposits::list_open_risk_flags(database)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(flags))
}

async fn resolve_risk_flag(
    State(state): State<Arc<AppState>>,
    Path(flag_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<ResolveRiskFlagInput>,
) -> ApiResult<moros_common::deposits::DepositRiskFlagRecord> {
    authorize_admin(&state, &headers)?;
    let database = require_database(&state)?;
    let flag_id = parse_uuid(&flag_id, "flag_id")?;
    let flag = deposits::resolve_risk_flag(database, flag_id, &payload)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("risk flag not found"))?;
    Ok(Json(flag))
}

async fn create_recovery_request(
    State(state): State<Arc<AppState>>,
    Path(transfer_id): Path<String>,
    Json(mut payload): Json<CreateDepositRecoveryInput>,
) -> ApiResult<moros_common::deposits::DepositRecoveryRecord> {
    let database = require_database(&state)?;
    payload.transfer_id = parse_uuid(&transfer_id, "transfer_id")?;
    let recovery = deposits::create_recovery_request(database, &payload)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(recovery))
}

async fn get_recovery_request(
    State(state): State<Arc<AppState>>,
    Path(recovery_id): Path<String>,
) -> ApiResult<moros_common::deposits::DepositRecoveryRecord> {
    let database = require_database(&state)?;
    let recovery_id = parse_uuid(&recovery_id, "recovery_id")?;
    let recovery = deposits::get_recovery_request(database, recovery_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("recovery request not found"))?;
    Ok(Json(recovery))
}

async fn list_recoveries(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<RouteJobListQuery>,
) -> ApiResult<Vec<moros_common::deposits::DepositRecoveryRecord>> {
    authorize_admin(&state, &headers)?;
    let database = require_database(&state)?;
    let recoveries =
        deposits::list_recoveries(database, query.status.as_deref(), query.limit.unwrap_or(50))
            .await
            .map_err(ApiError::internal)?;
    Ok(Json(recoveries))
}

async fn resolve_recovery_request(
    State(state): State<Arc<AppState>>,
    Path(recovery_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<ResolveDepositRecoveryInput>,
) -> ApiResult<moros_common::deposits::DepositRecoveryRecord> {
    authorize_admin(&state, &headers)?;
    let database = require_database(&state)?;
    let recovery_id = parse_uuid(&recovery_id, "recovery_id")?;
    let recovery = deposits::resolve_recovery_request(database, recovery_id, &payload)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("recovery request not found"))?;
    Ok(Json(recovery))
}

async fn handle_route_job_callback(
    State(state): State<Arc<AppState>>,
    Path(job_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<ExecutorCallbackRequest>,
) -> ApiResult<serde_json::Value> {
    authorize_executor(&state, &headers)?;
    let database = require_database(&state)?;
    let job_id = parse_uuid(&job_id, "job_id")?;

    match payload.status.as_str() {
        "processing" => {
            deposits::mark_route_job_processing(
                database,
                job_id,
                payload
                    .response
                    .unwrap_or_else(|| json!({ "status": "processing" })),
            )
            .await
            .map_err(ApiError::internal)?;
        }
        "completed" => {
            deposits::mark_route_job_completed(
                database,
                job_id,
                payload
                    .response
                    .unwrap_or_else(|| json!({ "status": "completed" })),
                payload.destination_tx_hash.as_deref(),
            )
            .await
            .map_err(ApiError::internal)?;
        }
        "failed" => {
            let error = payload
                .error
                .unwrap_or_else(|| "route execution failed".to_string());
            deposits::mark_route_job_failed(
                database,
                job_id,
                &error,
                payload.retryable.unwrap_or(false),
            )
            .await
            .map_err(ApiError::internal)?;
        }
        _ => {
            return Err(ApiError::bad_request(
                "callback status must be processing, completed, or failed",
            ));
        }
    }

    Ok(Json(json!({ "ok": true })))
}

async fn watch_worker(state: Arc<AppState>) {
    loop {
        if let Err(error) = watch_once(state.clone()).await {
            tracing::error!(error = ?error, "deposit watch cycle failed");
        }
        tokio::time::sleep(Duration::from_millis(state.config.watch_interval_ms)).await;
    }
}

async fn watch_once(state: Arc<AppState>) -> anyhow::Result<()> {
    let database = match &state.database {
        Some(database) => database,
        None => return Ok(()),
    };
    let channels = deposits::list_watchable_channels(database).await?;
    if channels.is_empty() {
        return Ok(());
    }

    let assets = deposits::list_supported_assets(database)
        .await?
        .into_iter()
        .filter(|asset| asset.status == "enabled")
        .map(|asset| ((asset.id.clone(), asset.chain_key.clone()), asset))
        .collect::<HashMap<_, _>>();

    for channel in channels {
        let asset = match assets.get(&(channel.asset_id.clone(), channel.chain_key.clone())) {
            Some(asset) => asset,
            None => continue,
        };
        match (asset.chain_family.as_str(), asset.watch_mode.as_str()) {
            ("evm", "erc20_transfer") => {
                watch_evm_erc20_channel(database, &state, asset, &channel).await?;
            }
            ("evm", "native_transfer") => {
                watch_evm_native_channel(database, &state, asset, &channel).await?;
            }
            ("starknet", "starknet_transfer") => {
                watch_starknet_transfer_channel(database, &state, asset, &channel).await?;
            }
            ("solana", "native_transfer") => {
                watch_solana_native_channel(database, &state, asset, &channel).await?;
            }
            _ => continue,
        }
    }

    Ok(())
}

async fn watch_evm_erc20_channel(
    database: &PgPool,
    state: &AppState,
    asset: &DepositSupportedAssetRecord,
    channel: &DepositChannelRecord,
) -> anyhow::Result<()> {
    let Some(rpc_url) = state.config.source_rpc_url(asset) else {
        return Ok(());
    };
    let latest_block = fetch_latest_block_number(&state.http, rpc_url).await?;
    let fallback_from = latest_block.saturating_sub(asset.confirmations_required.max(1) as u64);
    let from_block = channel
        .last_scanned_block
        .or(channel.watch_from_block)
        .map(|value| value.max(0) as u64)
        .unwrap_or(fallback_from);
    let query_from = from_block.saturating_add(1);
    if query_from > latest_block {
        return Ok(());
    }

    let transfers = fetch_erc20_logs(
        &state.http,
        rpc_url,
        &asset.asset_address,
        query_from,
        latest_block,
        &[channel.deposit_address.clone()],
    )
    .await?;

    for transfer in transfers {
        observe_chain_transfer(
            database,
            &state.config,
            asset,
            channel,
            transfer,
            latest_block,
        )
        .await?;
    }

    let rescan_floor = latest_block.saturating_sub(asset.confirmations_required.max(1) as u64);
    deposits::update_channel_scan_block(
        database,
        parse_uuid(&channel.channel_id, "channel_id").map_err(|error| anyhow!(error.message))?,
        rescan_floor as i64,
    )
    .await?;

    Ok(())
}

async fn watch_evm_native_channel(
    database: &PgPool,
    state: &AppState,
    asset: &DepositSupportedAssetRecord,
    channel: &DepositChannelRecord,
) -> anyhow::Result<()> {
    let Some(rpc_url) = state.config.source_rpc_url(asset) else {
        return Ok(());
    };
    let latest_block = fetch_latest_block_number(&state.http, rpc_url).await?;
    let fallback_from = latest_block.saturating_sub(asset.confirmations_required.max(1) as u64);
    let from_block = channel
        .last_scanned_block
        .or(channel.watch_from_block)
        .map(|value| value.max(0) as u64)
        .unwrap_or(fallback_from);
    let query_from = from_block.saturating_add(1);
    if query_from > latest_block {
        return Ok(());
    }

    let transfers = fetch_evm_native_transfers(
        &state.http,
        rpc_url,
        query_from,
        latest_block,
        &channel.deposit_address,
    )
    .await?;

    for transfer in transfers {
        observe_chain_transfer(
            database,
            &state.config,
            asset,
            channel,
            transfer,
            latest_block,
        )
        .await?;
    }

    let rescan_floor = latest_block.saturating_sub(asset.confirmations_required.max(1) as u64);
    deposits::update_channel_scan_block(
        database,
        parse_uuid(&channel.channel_id, "channel_id").map_err(|error| anyhow!(error.message))?,
        rescan_floor as i64,
    )
    .await?;

    Ok(())
}

async fn watch_starknet_transfer_channel(
    database: &PgPool,
    state: &AppState,
    asset: &DepositSupportedAssetRecord,
    channel: &DepositChannelRecord,
) -> anyhow::Result<()> {
    let Some(rpc_url) = state.config.source_rpc_url(asset) else {
        return Ok(());
    };
    let latest_block = fetch_latest_starknet_block_number(&state.http, rpc_url).await?;
    let fallback_from = latest_block.saturating_sub(asset.confirmations_required.max(1) as u64);
    let from_block = channel
        .last_scanned_block
        .or(channel.watch_from_block)
        .map(|value| value.max(0) as u64)
        .unwrap_or(fallback_from);
    let query_from = from_block.saturating_add(1);
    if query_from > latest_block {
        return Ok(());
    }

    let transfers = fetch_starknet_transfer_events(
        &state.http,
        rpc_url,
        &asset.asset_address,
        query_from,
        latest_block,
        &channel.deposit_address,
    )
    .await?;

    for transfer in transfers {
        observe_chain_transfer(
            database,
            &state.config,
            asset,
            channel,
            transfer,
            latest_block,
        )
        .await?;
    }

    let rescan_floor = latest_block.saturating_sub(asset.confirmations_required.max(1) as u64);
    deposits::update_channel_scan_block(
        database,
        parse_uuid(&channel.channel_id, "channel_id").map_err(|error| anyhow!(error.message))?,
        rescan_floor as i64,
    )
    .await?;

    Ok(())
}

async fn watch_solana_native_channel(
    database: &PgPool,
    state: &AppState,
    asset: &DepositSupportedAssetRecord,
    channel: &DepositChannelRecord,
) -> anyhow::Result<()> {
    let Some(rpc_url) = state.config.source_rpc_url(asset) else {
        return Ok(());
    };
    let latest_slot = fetch_latest_solana_slot(&state.http, rpc_url).await?;
    let fallback_from = latest_slot.saturating_sub(asset.confirmations_required.max(1) as u64);
    let from_slot = channel
        .last_scanned_block
        .or(channel.watch_from_block)
        .map(|value| value.max(0) as u64)
        .unwrap_or(fallback_from);
    let query_from = from_slot.saturating_add(1);
    if query_from > latest_slot {
        return Ok(());
    }

    let transfers = fetch_solana_native_transfers(
        &state.http,
        rpc_url,
        query_from,
        latest_slot,
        &channel.deposit_address,
    )
    .await?;

    for transfer in transfers {
        observe_chain_transfer(
            database,
            &state.config,
            asset,
            channel,
            transfer,
            latest_slot,
        )
        .await?;
    }

    let rescan_floor = latest_slot.saturating_sub(asset.confirmations_required.max(1) as u64);
    deposits::update_channel_scan_block(
        database,
        parse_uuid(&channel.channel_id, "channel_id").map_err(|error| anyhow!(error.message))?,
        rescan_floor as i64,
    )
    .await?;

    Ok(())
}

async fn observe_chain_transfer(
    database: &PgPool,
    config: &DepositRouterConfig,
    asset: &DepositSupportedAssetRecord,
    channel: &DepositChannelRecord,
    transfer: ParsedObservedTransfer,
    latest_block: u64,
) -> anyhow::Result<()> {
    let sender_address = transfer.sender_address.clone();
    let transfer = deposits::observe_deposit_transfer(
        database,
        &ObserveDepositTransferInput {
            channel_id: parse_uuid(&channel.channel_id, "channel_id")
                .map_err(|error| anyhow!(error.message))?,
            asset_id: channel.asset_id.clone(),
            chain_key: channel.chain_key.clone(),
            deposit_address: channel.deposit_address.clone(),
            sender_address: transfer.sender_address,
            tx_hash: transfer.tx_hash,
            block_number: Some(transfer.block_number as i64),
            block_hash: transfer.block_hash,
            amount_raw: transfer.amount_raw,
            confirmations: (latest_block.saturating_sub(transfer.block_number) + 1) as i32,
            required_confirmations: asset.confirmations_required,
            credit_target: channel.wallet_address.clone(),
        },
    )
    .await?;

    if transfer.risk_state == "clear" {
        if let Some(reason) =
            evaluate_transfer_risk(config, asset, &transfer, sender_address.as_deref())
        {
            let transfer_id = parse_uuid(&transfer.transfer_id, "transfer_id")
                .map_err(|error| anyhow!(error.message))?;
            let _ = deposits::flag_deposit_transfer(
                database,
                transfer_id,
                &reason.code,
                &reason.severity,
                &reason.description,
            )
            .await?;
        }
    }

    Ok(())
}

async fn route_worker(state: Arc<AppState>) {
    loop {
        if let Err(error) = route_once(state.clone()).await {
            tracing::error!(error = ?error, "deposit route cycle failed");
        }
        tokio::time::sleep(Duration::from_millis(state.config.route_interval_ms)).await;
    }
}

async fn route_once(state: Arc<AppState>) -> anyhow::Result<()> {
    let database = match &state.database {
        Some(database) => database,
        None => return Ok(()),
    };
    let assets = deposits::list_supported_assets(database)
        .await?
        .into_iter()
        .map(|asset| ((asset.id.clone(), asset.chain_key.clone()), asset))
        .collect::<HashMap<_, _>>();

    let ready_transfers = deposits::list_transfers_ready_for_route(database, 64).await?;
    for transfer in &ready_transfers {
        if let Some(asset) = assets.get(&(transfer.asset_id.clone(), transfer.chain_key.clone())) {
            let transfer_id = parse_uuid(&transfer.transfer_id, "transfer_id")
                .map_err(|error| anyhow!(error.message))?;
            deposits::queue_route_job(
                database,
                transfer_id,
                &asset.route_kind,
                build_route_payload(transfer, asset),
            )
            .await?;
        }
    }

    let jobs = deposits::list_dispatchable_route_jobs(database, 32).await?;
    let executor_url = match &state.config.route_executor_url {
        Some(url) => url.clone(),
        None => return Ok(()),
    };

    for job in jobs {
        dispatch_route_job(database, &state.http, &state.config, &executor_url, job).await?;
    }

    Ok(())
}

async fn dispatch_route_job(
    database: &PgPool,
    http: &HttpClient,
    config: &DepositRouterConfig,
    executor_url: &str,
    job: DepositRouteJobRecord,
) -> anyhow::Result<()> {
    let job_id = parse_uuid(&job.job_id, "job_id").map_err(|error| anyhow!(error.message))?;
    let job = deposits::mark_route_job_dispatching(database, job_id).await?;
    let dispatch_url = resolve_executor_dispatch_url(executor_url);
    let mut request = http.post(dispatch_url).json(&json!({
        "job_id": job.job_id,
        "transfer_id": job.transfer_id,
        "job_type": job.job_type,
        "payload": job.payload,
    }));

    if let Some(token) = &config.route_executor_token {
        request = request.header("x-moros-executor-token", token);
    }

    let response = match request.send().await {
        Ok(response) => response,
        Err(error) => {
            deposits::mark_route_job_failed(database, job_id, &error.to_string(), true).await?;
            return Ok(());
        }
    };

    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        deposits::mark_route_job_failed(
            database,
            job_id,
            &format!("executor returned {status}: {text}"),
            status.is_server_error(),
        )
        .await?;
        return Ok(());
    }

    let body = serde_json::from_str::<Value>(&text).unwrap_or_else(|_| json!({ "message": text }));
    match body.get("status").and_then(Value::as_str) {
        Some("completed") => {
            let destination_tx_hash = body
                .get("destination_tx_hash")
                .and_then(Value::as_str)
                .map(str::to_string);
            deposits::mark_route_job_completed(
                database,
                job_id,
                body,
                destination_tx_hash.as_deref(),
            )
            .await?;
        }
        Some("failed") => {
            let error_message = body
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("executor reported failure");
            deposits::mark_route_job_failed(database, job_id, error_message, false).await?;
        }
        _ => {
            deposits::mark_route_job_processing(database, job_id, body).await?;
        }
    }

    Ok(())
}

fn resolve_executor_dispatch_url(executor_url: &str) -> String {
    let trimmed = executor_url.trim();
    let Ok(mut parsed) = reqwest::Url::parse(trimmed) else {
        return trimmed.to_string();
    };

    match parsed.path() {
        "" | "/" => {
            parsed.set_path("/v1/route-jobs");
            parsed.to_string()
        }
        _ => trimmed.to_string(),
    }
}

fn build_route_payload(
    transfer: &DepositTransferRecord,
    asset: &DepositSupportedAssetRecord,
) -> Value {
    let destination_wallet = transfer
        .credit_target
        .clone()
        .or_else(|| transfer.wallet_address.clone());
    json!({
        "source": {
            "chain_key": transfer.chain_key,
            "asset_id": transfer.asset_id,
            "asset_symbol": asset.asset_symbol,
            "asset_address": asset.asset_address,
            "asset_decimals": asset.asset_decimals,
            "amount_raw": transfer.amount_raw,
            "tx_hash": transfer.tx_hash,
            "deposit_address": transfer.deposit_address,
            "sender_address": transfer.sender_address,
            "confirmations": transfer.confirmations,
        },
        "destination": {
            "user_id": transfer.user_id,
            "chain_key": "starknet",
            "asset_symbol": "STRK",
            "wallet_address": destination_wallet,
        },
        "route_kind": asset.route_kind,
        "watch_mode": asset.watch_mode,
    })
}

#[derive(Debug)]
struct RiskDecision {
    code: String,
    severity: String,
    description: String,
}

fn evaluate_transfer_risk(
    config: &DepositRouterConfig,
    asset: &DepositSupportedAssetRecord,
    transfer: &DepositTransferRecord,
    sender_address: Option<&str>,
) -> Option<RiskDecision> {
    if asset.chain_family == "starknet" {
        if let (Some(route_wallet), Some(sender_address)) = (
            config.route_starknet_account_address.as_deref(),
            sender_address,
        ) {
            let normalized_sender = normalize_starknet_address(sender_address);
            if normalized_sender == route_wallet {
                return Some(RiskDecision {
                    code: "INTERNAL_ROUTE_WALLET_TRANSFER".to_string(),
                    severity: "low".to_string(),
                    description: "internal route-wallet fee top-up is not a user deposit"
                        .to_string(),
                });
            }
        }
    }

    let amount = parse_decimal_biguint(&transfer.amount_raw)?;
    let min = parse_decimal_biguint(&asset.min_amount).unwrap_or_else(BigUint::default);
    let max = parse_decimal_biguint(&asset.max_amount).unwrap_or_else(BigUint::default);
    if min > BigUint::default() && amount < min {
        return Some(RiskDecision {
            code: "AMOUNT_BELOW_MIN".to_string(),
            severity: "medium".to_string(),
            description: format!(
                "observed amount {} is below configured minimum {} {}",
                transfer.amount_display, asset.min_amount, asset.asset_symbol
            ),
        });
    }
    if max > BigUint::default() && amount > max {
        return Some(RiskDecision {
            code: "AMOUNT_ABOVE_MAX".to_string(),
            severity: "high".to_string(),
            description: format!(
                "observed amount {} is above configured maximum {} {}",
                transfer.amount_display, asset.max_amount, asset.asset_symbol
            ),
        });
    }
    if let Some(sender_address) = sender_address {
        let normalized = normalize_sender_address_for_asset(asset, sender_address);
        if config
            .blocked_senders
            .get(&asset.chain_key)
            .map(|blocked| blocked.contains(&normalized))
            .unwrap_or(false)
        {
            return Some(RiskDecision {
                code: "BLOCKED_SENDER".to_string(),
                severity: "critical".to_string(),
                description: format!("sender {normalized} is blocked for {}", asset.chain_key),
            });
        }
    }

    None
}

#[derive(Debug, Deserialize)]
struct JsonRpcResponse<T> {
    result: T,
}

#[derive(Debug, Deserialize)]
struct EvmLog {
    topics: Vec<String>,
    data: String,
    #[serde(rename = "blockNumber")]
    block_number: String,
    #[serde(rename = "blockHash")]
    block_hash: Option<String>,
    #[serde(rename = "transactionHash")]
    transaction_hash: String,
}

#[derive(Debug, Clone)]
struct ParsedObservedTransfer {
    sender_address: Option<String>,
    tx_hash: String,
    block_number: u64,
    block_hash: Option<String>,
    amount_raw: String,
}

#[derive(Debug, Deserialize)]
struct EvmBlock {
    hash: Option<String>,
    transactions: Vec<EvmTransaction>,
}

#[derive(Debug, Deserialize)]
struct EvmTransaction {
    hash: String,
    from: String,
    to: Option<String>,
    value: String,
    #[serde(rename = "blockHash")]
    block_hash: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StarknetEventsPage {
    events: Vec<StarknetEvent>,
    continuation_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StarknetEvent {
    keys: Vec<String>,
    data: Vec<String>,
    #[serde(rename = "block_hash")]
    block_hash: Option<String>,
    #[serde(rename = "block_number")]
    block_number: Option<u64>,
    #[serde(rename = "transaction_hash")]
    transaction_hash: String,
}

#[derive(Debug, Deserialize)]
struct SolanaSignatureInfo {
    signature: String,
    slot: u64,
    err: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct SolanaTransactionEnvelope {
    slot: u64,
    transaction: SolanaTransactionData,
    meta: Option<SolanaTransactionMeta>,
}

#[derive(Debug, Deserialize)]
struct SolanaTransactionData {
    message: SolanaMessage,
}

#[derive(Debug, Deserialize)]
struct SolanaMessage {
    #[serde(rename = "accountKeys")]
    account_keys: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct SolanaTransactionMeta {
    #[serde(rename = "preBalances")]
    pre_balances: Vec<u64>,
    #[serde(rename = "postBalances")]
    post_balances: Vec<u64>,
    err: Option<Value>,
}

async fn fetch_latest_block_number(http: &HttpClient, rpc_url: &str) -> anyhow::Result<u64> {
    let response = http
        .post(rpc_url)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "eth_blockNumber",
            "params": [],
        }))
        .send()
        .await
        .context("failed to request latest EVM block number")?;
    let body: JsonRpcResponse<String> = response
        .json()
        .await
        .context("failed to parse latest EVM block response")?;
    parse_hex_u64(&body.result)
}

async fn fetch_erc20_logs(
    http: &HttpClient,
    rpc_url: &str,
    token_address: &str,
    from_block: u64,
    to_block: u64,
    recipients: &[String],
) -> anyhow::Result<Vec<ParsedObservedTransfer>> {
    if recipients.is_empty() {
        return Ok(Vec::new());
    }
    let recipient_topics = recipients
        .iter()
        .map(|address| pad_topic_address(address))
        .collect::<Vec<_>>();
    let response = http
        .post(rpc_url)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "eth_getLogs",
            "params": [{
                "address": normalize_evm_address(token_address),
                "fromBlock": format!("0x{from_block:x}"),
                "toBlock": format!("0x{to_block:x}"),
                "topics": [EVM_TRANSFER_TOPIC, Value::Null, recipient_topics],
            }],
        }))
        .send()
        .await
        .context("failed to fetch ERC20 transfer logs")?;
    let body: JsonRpcResponse<Vec<EvmLog>> = response
        .json()
        .await
        .context("failed to parse ERC20 transfer logs response")?;

    body.result
        .into_iter()
        .map(|log| {
            let sender_address = log.topics.get(1).map(|topic| topic_to_address(topic));
            Ok(ParsedObservedTransfer {
                sender_address,
                tx_hash: log.transaction_hash,
                block_number: parse_hex_u64(&log.block_number)?,
                block_hash: log.block_hash,
                amount_raw: parse_hex_biguint_decimal(&log.data)?,
            })
        })
        .collect()
}

async fn fetch_evm_native_transfers(
    http: &HttpClient,
    rpc_url: &str,
    from_block: u64,
    to_block: u64,
    recipient: &str,
) -> anyhow::Result<Vec<ParsedObservedTransfer>> {
    let normalized_recipient = normalize_evm_address(recipient);
    let mut transfers = Vec::new();

    for block_number in from_block..=to_block {
        let response = http
            .post(rpc_url)
            .json(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "eth_getBlockByNumber",
                "params": [format!("0x{block_number:x}"), true],
            }))
            .send()
            .await
            .with_context(|| format!("failed to fetch EVM block {block_number}"))?;
        let body: JsonRpcResponse<Option<EvmBlock>> = response
            .json()
            .await
            .with_context(|| format!("failed to parse EVM block {block_number} response"))?;
        let Some(block) = body.result else {
            continue;
        };

        for tx in block.transactions {
            let Some(to_address) = tx.to.as_deref() else {
                continue;
            };
            if normalize_evm_address(to_address) != normalized_recipient {
                continue;
            }

            let amount_raw = parse_hex_biguint_decimal(&tx.value)?;
            if parse_decimal_biguint(&amount_raw).unwrap_or_default() == BigUint::default() {
                continue;
            }

            transfers.push(ParsedObservedTransfer {
                sender_address: Some(normalize_evm_address(&tx.from)),
                tx_hash: tx.hash,
                block_number,
                block_hash: tx.block_hash.or(block.hash.clone()),
                amount_raw,
            });
        }
    }

    Ok(transfers)
}

async fn fetch_latest_starknet_block_number(
    http: &HttpClient,
    rpc_url: &str,
) -> anyhow::Result<u64> {
    let response = http
        .post(rpc_url)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "starknet_blockNumber",
            "params": [],
        }))
        .send()
        .await
        .context("failed to request latest Starknet block number")?;
    let body: JsonRpcResponse<u64> = response
        .json()
        .await
        .context("failed to parse latest Starknet block response")?;
    Ok(body.result)
}

async fn fetch_latest_solana_slot(http: &HttpClient, rpc_url: &str) -> anyhow::Result<u64> {
    let response = http
        .post(rpc_url)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getSlot",
            "params": [{
                "commitment": "confirmed",
            }],
        }))
        .send()
        .await
        .context("failed to request latest Solana slot")?;
    let body: JsonRpcResponse<u64> = response
        .json()
        .await
        .context("failed to parse latest Solana slot response")?;
    Ok(body.result)
}

async fn fetch_starknet_transfer_events(
    http: &HttpClient,
    rpc_url: &str,
    token_address: &str,
    from_block: u64,
    to_block: u64,
    recipient: &str,
) -> anyhow::Result<Vec<ParsedObservedTransfer>> {
    let selector = format!("{:#x}", get_selector_from_name("Transfer")?);
    let normalized_token = normalize_starknet_address(token_address);
    let normalized_recipient = normalize_starknet_address(recipient);
    let mut continuation_token = None;
    let mut transfers = Vec::new();

    loop {
        let mut filter = json!({
            "from_block": { "block_number": from_block },
            "to_block": { "block_number": to_block },
            "address": normalized_token,
            "keys": [[selector], [], [normalized_recipient]],
            "chunk_size": STARKNET_EVENT_CHUNK_SIZE,
        });
        if let Some(token) = &continuation_token {
            filter["continuation_token"] = json!(token);
        }
        let response = http
            .post(rpc_url)
            .json(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "starknet_getEvents",
                "params": [filter],
            }))
            .send()
            .await
            .context("failed to fetch Starknet transfer events")?;
        let body: JsonRpcResponse<StarknetEventsPage> = response
            .json()
            .await
            .context("failed to parse Starknet transfer events response")?;

        for event in body.result.events {
            let Some(block_number) = event.block_number else {
                continue;
            };
            let Some(sender_address) = event.keys.get(1) else {
                continue;
            };

            transfers.push(ParsedObservedTransfer {
                sender_address: Some(normalize_starknet_address(sender_address)),
                tx_hash: normalize_starknet_address(&event.transaction_hash),
                block_number,
                block_hash: event
                    .block_hash
                    .map(|value| normalize_starknet_address(&value)),
                amount_raw: parse_starknet_u256_decimal(&event.data)?,
            });
        }

        continuation_token = body.result.continuation_token;
        if continuation_token.is_none() {
            break;
        }
    }

    Ok(transfers)
}

async fn fetch_solana_native_transfers(
    http: &HttpClient,
    rpc_url: &str,
    from_slot: u64,
    to_slot: u64,
    recipient: &str,
) -> anyhow::Result<Vec<ParsedObservedTransfer>> {
    let normalized_recipient = normalize_solana_address(recipient);
    let response = http
        .post(rpc_url)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getSignaturesForAddress",
            "params": [
                normalized_recipient,
                {
                    "limit": 512,
                    "commitment": "confirmed",
                }
            ],
        }))
        .send()
        .await
        .context("failed to fetch Solana signatures for deposit address")?;
    let body: JsonRpcResponse<Vec<SolanaSignatureInfo>> = response
        .json()
        .await
        .context("failed to parse Solana signature response")?;

    let mut relevant = body
        .result
        .into_iter()
        .filter(|signature| signature.err.is_none())
        .filter(|signature| signature.slot >= from_slot && signature.slot <= to_slot)
        .collect::<Vec<_>>();
    relevant.sort_by_key(|signature| signature.slot);

    let mut transfers = Vec::new();
    for signature in relevant {
        let response = http
            .post(rpc_url)
            .json(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "getTransaction",
                "params": [
                    signature.signature,
                    {
                        "commitment": "confirmed",
                        "encoding": "json",
                        "maxSupportedTransactionVersion": 0,
                    }
                ],
            }))
            .send()
            .await
            .with_context(|| {
                format!("failed to fetch Solana transaction {}", signature.signature)
            })?;
        let body: JsonRpcResponse<Option<SolanaTransactionEnvelope>> =
            response.json().await.with_context(|| {
                format!(
                    "failed to parse Solana transaction {} response",
                    signature.signature
                )
            })?;
        let Some(transaction) = body.result else {
            continue;
        };
        let Some(meta) = transaction.meta else {
            continue;
        };
        if meta.err.is_some() {
            continue;
        }
        let recipient_index = transaction
            .transaction
            .message
            .account_keys
            .iter()
            .position(|value| normalize_solana_address(value) == normalized_recipient);
        let Some(recipient_index) = recipient_index else {
            continue;
        };
        let pre_balance = meta
            .pre_balances
            .get(recipient_index)
            .copied()
            .unwrap_or_default();
        let post_balance = meta
            .post_balances
            .get(recipient_index)
            .copied()
            .unwrap_or_default();
        if post_balance <= pre_balance {
            continue;
        }

        transfers.push(ParsedObservedTransfer {
            sender_address: transaction
                .transaction
                .message
                .account_keys
                .first()
                .map(|value| normalize_solana_address(value)),
            tx_hash: signature.signature,
            block_number: transaction.slot,
            block_hash: None,
            amount_raw: (post_balance - pre_balance).to_string(),
        });
    }

    Ok(transfers)
}

fn parse_hex_u64(value: &str) -> anyhow::Result<u64> {
    let value = value.strip_prefix("0x").unwrap_or(value);
    u64::from_str_radix(value, 16).context("failed to parse hex u64")
}

fn parse_hex_biguint_decimal(value: &str) -> anyhow::Result<String> {
    let value = value.strip_prefix("0x").unwrap_or(value);
    let bytes = Vec::from_hex(if value.len() % 2 == 0 {
        value.to_string()
    } else {
        format!("0{value}")
    })
    .context("failed to decode hex amount")?;
    Ok(BigUint::from_bytes_be(&bytes).to_str_radix(10))
}

fn parse_decimal_biguint(value: &str) -> Option<BigUint> {
    BigUint::parse_bytes(value.as_bytes(), 10)
}

fn normalize_evm_address(value: &str) -> String {
    let value = value.trim().to_ascii_lowercase();
    if value.starts_with("0x") {
        value
    } else {
        format!("0x{value}")
    }
}

fn normalize_starknet_address(value: &str) -> String {
    Felt::from_hex(value)
        .map(|felt| format!("{:#x}", felt))
        .unwrap_or_else(|_| {
            let normalized = value.trim().to_ascii_lowercase();
            if normalized.starts_with("0x") {
                normalized
            } else {
                format!("0x{normalized}")
            }
        })
}

fn normalize_solana_address(value: &str) -> String {
    let normalized = value.trim();
    bs58::decode(normalized)
        .into_vec()
        .ok()
        .filter(|bytes| bytes.len() == 32)
        .map(|bytes| bs58::encode(bytes).into_string())
        .unwrap_or_else(|| normalized.to_string())
}

fn normalize_sender_address_for_asset(asset: &DepositSupportedAssetRecord, value: &str) -> String {
    match asset.chain_family.as_str() {
        "starknet" => normalize_starknet_address(value),
        "solana" => normalize_solana_address(value),
        _ => normalize_evm_address(value),
    }
}

fn topic_to_address(topic: &str) -> String {
    let normalized = topic.strip_prefix("0x").unwrap_or(topic);
    let truncated = if normalized.len() >= 40 {
        &normalized[normalized.len() - 40..]
    } else {
        normalized
    };
    format!("0x{}", truncated.to_ascii_lowercase())
}

fn pad_topic_address(address: &str) -> String {
    let normalized = normalize_evm_address(address);
    let body = normalized.trim_start_matches("0x");
    format!("0x{:0>64}", body)
}

fn select_deposit_address(
    master_secret: &str,
    account_key: &str,
    chain_key: &str,
    asset: &DepositSupportedAssetRecord,
) -> anyhow::Result<String> {
    match asset.chain_family.as_str() {
        "evm" => derive_evm_deposit_address(master_secret, account_key, chain_key),
        "starknet" => derive_starknet_deposit_address(master_secret, account_key, chain_key),
        "solana" => derive_solana_deposit_address(master_secret, account_key, chain_key),
        _ => unreachable!("unsupported deposit asset family should already return"),
    }
}

fn derive_evm_deposit_address(
    master_secret: &str,
    account_key: &str,
    chain_key: &str,
) -> anyhow::Result<String> {
    for nonce in 0u32..1024 {
        let mut hash = Keccak256::new();
        hash.update(master_secret.as_bytes());
        hash.update([0u8]);
        hash.update(account_key.to_ascii_lowercase().as_bytes());
        hash.update([0u8]);
        hash.update(chain_key.as_bytes());
        hash.update([0u8]);
        hash.update(nonce.to_be_bytes());
        let secret_bytes = hash.finalize();
        if let Ok(secret_key) = SecretKey::from_slice(&secret_bytes) {
            let public_key = secret_key.public_key();
            let encoded = public_key.to_encoded_point(false);
            let address_hash = Keccak256::digest(&encoded.as_bytes()[1..]);
            return Ok(format!("0x{}", hex::encode(&address_hash[12..])));
        }
    }

    Err(anyhow!("failed to derive a valid EVM custody key"))
}

fn derive_starknet_private_key(master_secret: &str, account_key: &str, chain_key: &str) -> String {
    let mut hash = Keccak256::new();
    hash.update(master_secret.as_bytes());
    hash.update([0u8]);
    hash.update(account_key.to_ascii_lowercase().as_bytes());
    hash.update([0u8]);
    hash.update(chain_key.as_bytes());
    let digest = hash.finalize();
    let modulus =
        BigUint::parse_bytes(STARKNET_CURVE_ORDER_HEX.as_bytes(), 16).expect("valid curve order");
    let mut secret_scalar = BigUint::from_bytes_be(&digest) % modulus;
    if secret_scalar == BigUint::default() {
        secret_scalar = BigUint::from(1u32);
    }
    format!("0x{}", secret_scalar.to_str_radix(16))
}

fn derive_starknet_deposit_address(
    master_secret: &str,
    account_key: &str,
    chain_key: &str,
) -> anyhow::Result<String> {
    let private_key = derive_starknet_private_key(master_secret, account_key, chain_key);
    let secret_scalar =
        Felt::from_hex(&private_key).context("invalid derived Starknet private key")?;
    let signing_key = SigningKey::from_secret_scalar(secret_scalar);
    let public_key = signing_key.verifying_key().scalar();
    let class_hash = Felt::from_hex(STARKNET_OPENZEPPELIN_CLASS_HASH)
        .context("invalid Starknet OpenZeppelin class hash")?;
    let constructor_calldata = vec![public_key];
    Ok(format!(
        "{:#x}",
        get_contract_address(public_key, class_hash, &constructor_calldata, Felt::ZERO)
    ))
}

fn derive_solana_private_key_seed(
    master_secret: &str,
    account_key: &str,
    chain_key: &str,
) -> [u8; 32] {
    let mut hash = Keccak256::new();
    hash.update(master_secret.as_bytes());
    hash.update([0u8]);
    hash.update(account_key.to_ascii_lowercase().as_bytes());
    hash.update([0u8]);
    hash.update(chain_key.as_bytes());
    let digest = hash.finalize();
    let mut seed = [0u8; 32];
    seed.copy_from_slice(&digest[..32]);
    seed
}

fn derive_solana_deposit_address(
    master_secret: &str,
    account_key: &str,
    chain_key: &str,
) -> anyhow::Result<String> {
    let seed = derive_solana_private_key_seed(master_secret, account_key, chain_key);
    let signing_key = SolanaSigningKey::from_bytes(&seed);
    Ok(bs58::encode(signing_key.verifying_key().to_bytes()).into_string())
}

fn build_deposit_uri(asset: &DepositSupportedAssetRecord, deposit_address: &str) -> String {
    if asset.chain_family == "evm" {
        if asset.watch_mode == "native_transfer" {
            format!(
                "ethereum:{}@{}",
                normalize_evm_address(deposit_address),
                asset.chain_id,
            )
        } else {
            format!(
                "ethereum:{}@{}/transfer?address={}",
                normalize_evm_address(&asset.asset_address),
                asset.chain_id,
                normalize_evm_address(deposit_address),
            )
        }
    } else if asset.chain_family == "starknet" {
        format!("starknet:{}", normalize_starknet_address(deposit_address))
    } else if asset.chain_family == "solana" {
        format!("solana:{}", normalize_solana_address(deposit_address))
    } else {
        deposit_address.to_string()
    }
}

fn parse_starknet_u256_decimal(data: &[String]) -> anyhow::Result<String> {
    let low = data
        .first()
        .context("missing Starknet transfer amount low limb")?;
    let high = data
        .get(1)
        .context("missing Starknet transfer amount high limb")?;
    let low = parse_hex_biguint(low.trim_start_matches("0x"))?;
    let high = parse_hex_biguint(high.trim_start_matches("0x"))?;
    Ok((low + (high << 128usize)).to_str_radix(10))
}

fn parse_hex_biguint(value: &str) -> anyhow::Result<BigUint> {
    let bytes = Vec::from_hex(if value.len() % 2 == 0 {
        value.to_string()
    } else {
        format!("0{value}")
    })
    .context("failed to decode hex biguint")?;
    Ok(BigUint::from_bytes_be(&bytes))
}

fn require_database(state: &AppState) -> Result<&PgPool, ApiError> {
    state
        .database
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("database is not configured"))
}

fn parse_uuid(value: &str, field: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(value)
        .map_err(|error| ApiError::bad_request(format!("invalid {field}: {error}")))
}

fn authorize_admin(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    let expected = state
        .config
        .admin_token
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("admin token is not configured"))?;
    let actual = headers
        .get("x-moros-admin-token")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| ApiError::unauthorized("missing x-moros-admin-token"))?;
    if actual != expected {
        return Err(ApiError::unauthorized("invalid x-moros-admin-token"));
    }
    Ok(())
}

fn authorize_executor(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    let expected = state.config.route_executor_token.as_ref().ok_or_else(|| {
        ApiError::service_unavailable("executor callback token is not configured")
    })?;
    let actual = headers
        .get("x-moros-executor-token")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| ApiError::unauthorized("missing x-moros-executor-token"))?;
    if actual != expected {
        return Err(ApiError::unauthorized("invalid x-moros-executor-token"));
    }
    Ok(())
}

impl DepositRouterConfig {
    fn from_env() -> anyhow::Result<Self> {
        let mut service = ServiceConfig::from_env("moros-deposit-router", 8084);
        service.redis_url = None;
        let configured_supported_assets: Vec<DepositSupportedAssetSeed> =
            parse_json_env("MOROS_DEPOSIT_SUPPORTED_ASSETS").unwrap_or_default();
        let supported_assets = if configured_supported_assets.is_empty() {
            default_supported_assets_for_environment(&service.environment)
        } else {
            configured_supported_assets
        };

        Ok(Self {
            deposit_master_secret: std::env::var("MOROS_DEPOSIT_MASTER_SECRET").ok(),
            route_executor_url: std::env::var("MOROS_DEPOSIT_EXECUTOR_URL").ok(),
            route_executor_token: std::env::var("MOROS_DEPOSIT_EXECUTOR_TOKEN").ok(),
            admin_token: std::env::var("MOROS_ADMIN_TOKEN").ok(),
            route_starknet_account_address: std::env::var(
                "MOROS_DEPOSIT_ROUTE_STARKNET_ACCOUNT_ADDRESS",
            )
            .ok()
            .map(|value| normalize_starknet_address(&value)),
            rpc_urls: parse_json_env("MOROS_DEPOSIT_RPC_URLS").unwrap_or_default(),
            supported_assets,
            blocked_senders: parse_json_env::<HashMap<String, Vec<String>>>(
                "MOROS_DEPOSIT_BLOCKED_SENDERS",
            )
            .unwrap_or_default()
            .into_iter()
            .map(|(chain, values)| {
                let is_starknet = chain.starts_with("starknet");
                (
                    chain,
                    values
                        .into_iter()
                        .map(|value| {
                            if is_starknet {
                                normalize_starknet_address(&value)
                            } else {
                                normalize_evm_address(&value)
                            }
                        })
                        .collect::<HashSet<_>>(),
                )
            })
            .collect(),
            watch_interval_ms: parse_u64_env("MOROS_DEPOSIT_POLL_INTERVAL_MS", 12_000),
            route_interval_ms: parse_u64_env("MOROS_DEPOSIT_ROUTE_INTERVAL_MS", 5_000),
            service,
        })
    }

    fn source_rpc_url<'a>(&'a self, asset: &DepositSupportedAssetRecord) -> Option<&'a str> {
        match asset.chain_family.as_str() {
            "evm" => self.rpc_urls.get(&asset.chain_key).map(String::as_str),
            "starknet" => self
                .rpc_urls
                .get(&asset.chain_key)
                .map(String::as_str)
                .or(self.service.starknet_rpc_url.as_deref()),
            "solana" => self
                .rpc_urls
                .get(&asset.chain_key)
                .map(String::as_str)
                .or(default_solana_rpc_url(&asset.chain_key)),
            _ => None,
        }
    }
}

fn default_solana_rpc_url(chain_key: &str) -> Option<&'static str> {
    match chain_key {
        "solana-mainnet" => Some("https://api.mainnet-beta.solana.com"),
        "solana-testnet" => Some("https://api.testnet.solana.com"),
        _ => None,
    }
}

fn default_supported_assets_for_environment(environment: &str) -> Vec<DepositSupportedAssetSeed> {
    if environment == "production" {
        return Vec::new();
    }

    vec![
        asset_seed(
            "strk",
            "starknet-sepolia",
            "starknet",
            "sepolia",
            "SN_SEPOLIA",
            "STRK",
            "0x04718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d",
            18,
            "starknet_credit_to_strk",
            "starknet_transfer",
            "100000000000000000",
            "100000000000000000000000",
            2,
            "STRK on Starknet",
            "Starknet",
        ),
        asset_seed(
            "eth",
            "starknet-sepolia",
            "starknet",
            "sepolia",
            "SN_SEPOLIA",
            "ETH",
            "0x049d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7",
            18,
            "starknet_swap_to_strk",
            "starknet_transfer",
            "100000000000000",
            "100000000000000000000",
            2,
            "ETH on Starknet",
            "Starknet",
        ),
        asset_seed(
            "usdc",
            "starknet-sepolia",
            "starknet",
            "sepolia",
            "SN_SEPOLIA",
            "USDC",
            "0x0512feac6339ff7889822cb5aa2a86c848e9d392bb0e3e237c008674feed8343",
            6,
            "starknet_swap_to_strk",
            "starknet_transfer",
            "1000000",
            "50000000000",
            2,
            "USDC on Starknet",
            "Starknet",
        ),
        asset_seed(
            "eth",
            "ethereum-mainnet",
            "evm",
            "mainnet",
            "1",
            "ETH",
            "0x0000000000000000000000000000000000455448",
            18,
            "bridge_and_swap_to_strk",
            "native_transfer",
            "500000000000000",
            "100000000000000000000",
            12,
            "ETH on Ethereum",
            "Ethereum",
        ),
        asset_seed(
            "usdc",
            "ethereum-mainnet",
            "evm",
            "mainnet",
            "1",
            "USDC",
            "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
            6,
            "bridge_and_swap_to_strk",
            "erc20_transfer",
            "1000000",
            "50000000000",
            12,
            "USDC on Ethereum",
            "Ethereum",
        ),
        asset_seed(
            "sol",
            "solana-testnet",
            "solana",
            "testnet",
            "4uhcVJyU9pJkvQyS88uRDiswHXSCkY3z",
            "SOL",
            "native",
            9,
            "solana_bridge_and_swap_to_strk",
            "native_transfer",
            "20000000",
            "100000000000",
            16,
            "SOL on Solana",
            "Solana",
        ),
    ]
}

#[allow(clippy::too_many_arguments)]
fn asset_seed(
    id: &str,
    chain_key: &str,
    chain_family: &str,
    network: &str,
    chain_id: &str,
    asset_symbol: &str,
    asset_address: &str,
    asset_decimals: i32,
    route_kind: &str,
    watch_mode: &str,
    min_amount: &str,
    max_amount: &str,
    confirmations_required: i32,
    label: &str,
    chain_label: &str,
) -> DepositSupportedAssetSeed {
    DepositSupportedAssetSeed {
        id: id.to_string(),
        chain_key: chain_key.to_string(),
        chain_family: chain_family.to_string(),
        network: network.to_string(),
        chain_id: chain_id.to_string(),
        asset_symbol: asset_symbol.to_string(),
        asset_address: asset_address.to_string(),
        asset_decimals,
        route_kind: route_kind.to_string(),
        watch_mode: watch_mode.to_string(),
        min_amount: min_amount.to_string(),
        max_amount: max_amount.to_string(),
        confirmations_required,
        status: "enabled".to_string(),
        metadata: json!({
            "label": label,
            "chain_label": chain_label,
            "source_category": "live"
        }),
    }
}

fn parse_u64_env(key: &str, default_value: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(default_value)
}

fn parse_json_env<T>(key: &str) -> Option<T>
where
    T: serde::de::DeserializeOwned,
{
    std::env::var(key)
        .ok()
        .and_then(|value| serde_json::from_str::<T>(&value).ok())
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

    fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
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
        tracing::error!(error = ?error, "deposit router request failed");
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
            Json(json!({
                "error": self.message,
            })),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DepositRouterConfig, RiskDecision, build_deposit_uri, build_route_payload,
        derive_evm_deposit_address, derive_solana_deposit_address, derive_starknet_deposit_address,
        evaluate_transfer_risk, filter_user_visible_deposit_status, normalize_evm_address,
        normalize_starknet_address, select_deposit_address,
    };
    use moros_common::{
        config::ServiceConfig,
        deposits::{
            DepositRecoveryRecord, DepositRiskFlagRecord, DepositRouteJobRecord,
            DepositSupportedAssetRecord, DepositTransferRecord,
        },
    };
    use serde_json::json;
    use std::collections::{HashMap, HashSet};

    fn sample_asset() -> DepositSupportedAssetRecord {
        DepositSupportedAssetRecord {
            id: "usdc".to_string(),
            chain_key: "ethereum-mainnet".to_string(),
            chain_family: "evm".to_string(),
            network: "mainnet".to_string(),
            chain_id: "1".to_string(),
            asset_symbol: "USDC".to_string(),
            asset_address: normalize_evm_address("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
            asset_decimals: 6,
            route_kind: "bridge_and_swap_to_strk".to_string(),
            watch_mode: "erc20_transfer".to_string(),
            min_amount: "1000000".to_string(),
            max_amount: "50000000000".to_string(),
            confirmations_required: 12,
            status: "enabled".to_string(),
            metadata: json!({}),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        }
    }

    fn sample_starknet_asset() -> DepositSupportedAssetRecord {
        DepositSupportedAssetRecord {
            id: "strk".to_string(),
            chain_key: "starknet-sepolia".to_string(),
            chain_family: "starknet".to_string(),
            network: "sepolia".to_string(),
            chain_id: "SN_SEPOLIA".to_string(),
            asset_symbol: "STRK".to_string(),
            asset_address: "0x04718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d"
                .to_string(),
            asset_decimals: 18,
            route_kind: "starknet_credit_to_strk".to_string(),
            watch_mode: "starknet_transfer".to_string(),
            min_amount: "100000000000000000".to_string(),
            max_amount: "100000000000000000000000".to_string(),
            confirmations_required: 2,
            status: "enabled".to_string(),
            metadata: json!({}),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        }
    }

    fn sample_solana_asset() -> DepositSupportedAssetRecord {
        DepositSupportedAssetRecord {
            id: "sol".to_string(),
            chain_key: "solana-testnet".to_string(),
            chain_family: "solana".to_string(),
            network: "testnet".to_string(),
            chain_id: "4uhcVJyU9pJkvQyS88uRDiswHXSCkY3z".to_string(),
            asset_symbol: "SOL".to_string(),
            asset_address: "native".to_string(),
            asset_decimals: 9,
            route_kind: "solana_bridge_and_swap_to_strk".to_string(),
            watch_mode: "native_transfer".to_string(),
            min_amount: "1000000".to_string(),
            max_amount: "100000000000".to_string(),
            confirmations_required: 16,
            status: "enabled".to_string(),
            metadata: json!({}),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        }
    }

    fn sample_transfer() -> DepositTransferRecord {
        DepositTransferRecord {
            transfer_id: "8f848649-9540-499a-aeb2-e0185f091855".to_string(),
            channel_id: "b7187d52-90d4-42e5-a3bc-e6f4a6284a6c".to_string(),
            user_id: "0d50fb4e-1cd7-48fa-878a-c94283229c59".to_string(),
            wallet_address: Some(
                "0x0643e1766bc860d19ce81c8c7c315d62e5396f2f404d441d0ae9f75e7ed7548a".to_string(),
            ),
            username: Some("flow".to_string()),
            asset_id: "usdc".to_string(),
            chain_key: "ethereum-mainnet".to_string(),
            asset_symbol: "USDC".to_string(),
            deposit_address: "0x1111111111111111111111111111111111111111".to_string(),
            sender_address: Some("0x2222222222222222222222222222222222222222".to_string()),
            tx_hash: "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_string(),
            block_number: Some(22_222_222),
            amount_raw: "2500000".to_string(),
            amount_display: "2.5".to_string(),
            confirmations: 12,
            required_confirmations: 12,
            status: "ORIGIN_CONFIRMED".to_string(),
            risk_state: "clear".to_string(),
            credit_target: Some(
                "0x0643e1766bc860d19ce81c8c7c315d62e5396f2f404d441d0ae9f75e7ed7548a".to_string(),
            ),
            destination_tx_hash: None,
            detected_at: "now".to_string(),
            confirmed_at: Some("now".to_string()),
            completed_at: None,
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        }
    }

    fn sample_route_job() -> DepositRouteJobRecord {
        DepositRouteJobRecord {
            job_id: "c3aecc7b-1a3e-4547-9b6a-8fd98d80d1ef".to_string(),
            transfer_id: sample_transfer().transfer_id,
            job_type: "starknet_credit_to_strk".to_string(),
            status: "processing".to_string(),
            attempts: 1,
            payload: json!({}),
            response: None,
            last_error: None,
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        }
    }

    fn sample_config() -> DepositRouterConfig {
        DepositRouterConfig {
            service: ServiceConfig::from_env("moros-deposit-router", 8084),
            deposit_master_secret: Some("moros-secret".to_string()),
            route_executor_url: Some("http://127.0.0.1:19085/v1/route-jobs".to_string()),
            route_executor_token: Some("executor-token".to_string()),
            admin_token: Some("admin-token".to_string()),
            route_starknet_account_address: None,
            rpc_urls: HashMap::new(),
            supported_assets: Vec::new(),
            blocked_senders: HashMap::new(),
            watch_interval_ms: 12_000,
            route_interval_ms: 5_000,
        }
    }

    fn expect_risk(decision: Option<RiskDecision>, expected_code: &str) {
        let decision = decision.expect("expected risk decision");
        assert_eq!(decision.code, expected_code);
    }

    #[test]
    fn derives_deterministic_evm_deposit_addresses() {
        let first = derive_evm_deposit_address("moros-secret", "0x1234", "ethereum-mainnet")
            .expect("address derivation should succeed");
        let second = derive_evm_deposit_address("moros-secret", "0x1234", "ethereum-mainnet")
            .expect("address derivation should succeed");
        let third = derive_evm_deposit_address("moros-secret", "0xabcd", "ethereum-mainnet")
            .expect("address derivation should succeed");
        let same_chain_different_token =
            derive_evm_deposit_address("moros-secret", "0x1234", "ethereum-mainnet")
                .expect("address derivation should succeed");

        assert_eq!(first, second);
        assert_eq!(first, same_chain_different_token);
        assert_ne!(first, third);
        assert!(first.starts_with("0x"));
        assert_eq!(first.len(), 42);
    }

    #[test]
    fn builds_eip_681_transfer_uri_for_evm_assets() {
        let asset = sample_asset();

        let uri = build_deposit_uri(&asset, "0x1111111111111111111111111111111111111111");
        assert!(uri.contains("ethereum:"));
        assert!(uri.contains("@1/transfer?address=0x1111111111111111111111111111111111111111"));
    }

    #[test]
    fn derives_deterministic_starknet_deposit_addresses() {
        let first = derive_starknet_deposit_address("moros-secret", "0x1234", "starknet-sepolia")
            .expect("address derivation should succeed");
        let second = derive_starknet_deposit_address("moros-secret", "0x1234", "starknet-sepolia")
            .expect("address derivation should succeed");
        let third = derive_starknet_deposit_address("moros-secret", "0xabcd", "starknet-sepolia")
            .expect("address derivation should succeed");

        assert_eq!(first, second);
        assert_ne!(first, third);
        assert!(first.starts_with("0x"));
    }

    #[test]
    fn builds_starknet_uri_for_starknet_assets() {
        let asset = sample_starknet_asset();

        let uri = build_deposit_uri(
            &asset,
            "0x05b8b9f11ca9321fd0ff056c8156ba2578dd1b3ea3d8b1e61160a13b5b13b72",
        );
        assert_eq!(
            uri,
            "starknet:0x5b8b9f11ca9321fd0ff056c8156ba2578dd1b3ea3d8b1e61160a13b5b13b72"
        );
    }

    #[test]
    fn derives_deterministic_solana_deposit_addresses() {
        let first = derive_solana_deposit_address("moros-secret", "0x1234", "solana-testnet")
            .expect("address derivation should succeed");
        let second = derive_solana_deposit_address("moros-secret", "0x1234", "solana-testnet")
            .expect("address derivation should succeed");
        let third = derive_solana_deposit_address("moros-secret", "0xabcd", "solana-testnet")
            .expect("address derivation should succeed");

        assert_eq!(first, second);
        assert_ne!(first, third);
        assert!(first.len() >= 32);
    }

    #[test]
    fn builds_solana_uri_for_solana_assets() {
        let asset = sample_solana_asset();
        let uri = build_deposit_uri(&asset, "G9aZ2hPGd4xMnnnL2zP7Y78iWY6gWQBaLtgBoQxGc1qn");

        assert_eq!(uri, "solana:G9aZ2hPGd4xMnnnL2zP7Y78iWY6gWQBaLtgBoQxGc1qn");
    }

    #[test]
    fn flags_transfers_outside_amount_limits() {
        let asset = sample_asset();
        let mut transfer = sample_transfer();
        transfer.amount_raw = "999999".to_string();
        transfer.amount_display = "0.999999".to_string();

        expect_risk(
            evaluate_transfer_risk(
                &sample_config(),
                &asset,
                &transfer,
                transfer.sender_address.as_deref(),
            ),
            "AMOUNT_BELOW_MIN",
        );

        transfer.amount_raw = "50000000001".to_string();
        transfer.amount_display = "50000.000001".to_string();

        expect_risk(
            evaluate_transfer_risk(
                &sample_config(),
                &asset,
                &transfer,
                transfer.sender_address.as_deref(),
            ),
            "AMOUNT_ABOVE_MAX",
        );
    }

    #[test]
    fn flags_blocked_senders_after_normalization() {
        let asset = sample_asset();
        let transfer = sample_transfer();
        let sender = "2222222222222222222222222222222222222222";
        let mut config = sample_config();
        config.blocked_senders.insert(
            "ethereum-mainnet".to_string(),
            HashSet::from([normalize_evm_address(sender)]),
        );

        expect_risk(
            evaluate_transfer_risk(&config, &asset, &transfer, Some(sender)),
            "BLOCKED_SENDER",
        );
    }

    #[test]
    fn flags_starknet_route_wallet_fee_topups_as_internal() {
        let asset = sample_starknet_asset();
        let mut transfer = sample_transfer();
        transfer.amount_raw = "100000000000000000".to_string();
        transfer.amount_display = "0.1".to_string();
        let route_wallet = "0x071c57fb19f9e0ca28e7e341eebb1415e78ec72e4045c637e78329b654912a81";
        let mut config = sample_config();
        config.route_starknet_account_address = Some(normalize_starknet_address(route_wallet));

        expect_risk(
            evaluate_transfer_risk(&config, &asset, &transfer, Some(route_wallet)),
            "INTERNAL_ROUTE_WALLET_TRANSFER",
        );
    }

    #[test]
    fn builds_route_payload_with_origin_and_destination_context() {
        let asset = sample_asset();
        let transfer = sample_transfer();
        let payload = build_route_payload(&transfer, &asset);

        assert_eq!(
            payload["source"]["deposit_address"],
            json!("0x1111111111111111111111111111111111111111")
        );
        assert_eq!(payload["source"]["confirmations"], json!(12));
        assert_eq!(payload["destination"]["chain_key"], json!("starknet"));
        assert_eq!(payload["destination"]["asset_symbol"], json!("STRK"));
        assert_eq!(
            payload["destination"]["wallet_address"],
            json!("0x0643e1766bc860d19ce81c8c7c315d62e5396f2f404d441d0ae9f75e7ed7548a")
        );
    }

    #[test]
    fn builds_route_payload_from_linked_wallet_when_detection_preceded_wallet_resolution() {
        let asset = sample_asset();
        let mut transfer = sample_transfer();
        transfer.credit_target = None;
        let payload = build_route_payload(&transfer, &asset);

        assert_eq!(
            payload["destination"]["wallet_address"],
            json!("0x0643e1766bc860d19ce81c8c7c315d62e5396f2f404d441d0ae9f75e7ed7548a")
        );
    }

    #[test]
    fn evm_issue_always_uses_deterministic_user_chain_route_address() {
        let asset = sample_asset();
        let selected = select_deposit_address("moros-secret", "user-1", "ethereum-mainnet", &asset)
            .expect("address should resolve");
        let same_user_same_chain =
            select_deposit_address("moros-secret", "user-1", "ethereum-mainnet", &asset)
                .expect("address should resolve");
        let different_chain =
            select_deposit_address("moros-secret", "user-1", "starknet-sepolia", &asset)
                .expect("address should resolve");

        assert_eq!(selected, same_user_same_chain);
        assert_ne!(selected, different_chain);
    }

    #[test]
    fn normalizes_and_validates_starknet_beneficiary_wallets() {
        let normalized = crate::normalize_starknet_beneficiary_wallet(
            " 0x0643E1766BC860D19CE81C8C7C315D62E5396F2F404D441D0AE9F75E7ED7548A ",
        )
        .expect("beneficiary wallet should normalize");
        assert_eq!(
            normalized,
            "0x643e1766bc860d19ce81c8c7c315d62e5396f2f404d441d0ae9f75e7ed7548a"
        );
        assert!(
            crate::normalize_starknet_beneficiary_wallet("").is_err(),
            "empty beneficiary wallet should fail"
        );
        assert!(
            crate::normalize_starknet_beneficiary_wallet("not-a-wallet").is_err(),
            "non-felt beneficiary wallet should fail"
        );
        assert!(
            crate::normalize_starknet_beneficiary_wallet("0x0").is_err(),
            "zero beneficiary wallet should fail"
        );
    }

    #[test]
    fn normalizes_bare_executor_base_url_to_route_jobs_endpoint() {
        assert_eq!(
            crate::resolve_executor_dispatch_url("http://127.0.0.1:18085"),
            "http://127.0.0.1:18085/v1/route-jobs"
        );
        assert_eq!(
            crate::resolve_executor_dispatch_url("http://127.0.0.1:18085/"),
            "http://127.0.0.1:18085/v1/route-jobs"
        );
        assert_eq!(
            crate::resolve_executor_dispatch_url("http://127.0.0.1:18085/v1/route-jobs"),
            "http://127.0.0.1:18085/v1/route-jobs"
        );
    }

    #[test]
    fn hides_internal_route_wallet_topups_from_user_visible_status() {
        let visible_transfer = sample_transfer();
        let hidden_transfer = DepositTransferRecord {
            transfer_id: "hidden-transfer".to_string(),
            ..sample_transfer()
        };
        let visible_route_job = DepositRouteJobRecord {
            transfer_id: visible_transfer.transfer_id.clone(),
            ..sample_route_job()
        };
        let hidden_route_job = DepositRouteJobRecord {
            transfer_id: hidden_transfer.transfer_id.clone(),
            ..sample_route_job()
        };
        let visible_flag = DepositRiskFlagRecord {
            flag_id: "visible-flag".to_string(),
            transfer_id: visible_transfer.transfer_id.clone(),
            code: "AMOUNT_ABOVE_MAX".to_string(),
            severity: "medium".to_string(),
            description: "user-visible".to_string(),
            resolution_status: "open".to_string(),
            resolution_notes: None,
            created_at: "2026-04-21T00:00:00.000Z".to_string(),
            resolved_at: None,
        };
        let hidden_flag = DepositRiskFlagRecord {
            flag_id: "hidden-flag".to_string(),
            transfer_id: hidden_transfer.transfer_id.clone(),
            code: "INTERNAL_ROUTE_WALLET_TRANSFER".to_string(),
            severity: "low".to_string(),
            description: "internal topup".to_string(),
            resolution_status: "open".to_string(),
            resolution_notes: None,
            created_at: "2026-04-21T00:00:00.000Z".to_string(),
            resolved_at: None,
        };
        let visible_recovery = DepositRecoveryRecord {
            recovery_id: "visible-recovery".to_string(),
            transfer_id: visible_transfer.transfer_id.clone(),
            reason: "manual_review".to_string(),
            notes: None,
            requested_by: None,
            status: "open".to_string(),
            resolution_notes: None,
            created_at: "2026-04-21T00:00:00.000Z".to_string(),
            updated_at: "2026-04-21T00:00:00.000Z".to_string(),
            resolved_at: None,
        };
        let hidden_recovery = DepositRecoveryRecord {
            recovery_id: "hidden-recovery".to_string(),
            transfer_id: hidden_transfer.transfer_id.clone(),
            reason: "internal".to_string(),
            notes: None,
            requested_by: None,
            status: "open".to_string(),
            resolution_notes: None,
            created_at: "2026-04-21T00:00:00.000Z".to_string(),
            updated_at: "2026-04-21T00:00:00.000Z".to_string(),
            resolved_at: None,
        };

        let (transfers, route_jobs, risk_flags, recoveries) = filter_user_visible_deposit_status(
            vec![hidden_transfer, visible_transfer.clone()],
            vec![hidden_route_job, visible_route_job],
            vec![hidden_flag, visible_flag],
            vec![hidden_recovery, visible_recovery],
        );

        assert_eq!(transfers.len(), 1);
        assert_eq!(transfers[0].transfer_id, visible_transfer.transfer_id);
        assert_eq!(route_jobs.len(), 1);
        assert_eq!(route_jobs[0].transfer_id, visible_transfer.transfer_id);
        assert_eq!(risk_flags.len(), 1);
        assert_eq!(risk_flags[0].transfer_id, visible_transfer.transfer_id);
        assert_eq!(recoveries.len(), 1);
        assert_eq!(recoveries[0].transfer_id, visible_transfer.transfer_id);
    }
}
