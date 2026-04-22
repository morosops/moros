use anyhow::{Context, anyhow};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use moros_common::{
    accounts, balances, blackjack,
    chain::ChainService,
    config::ServiceConfig,
    infra::{InfraSnapshot, ServiceInfra},
    persistence, telemetry,
    web::base_router,
};
use serde::Serialize;
use serde_json::{Value, json};
use sha3::{Digest, Keccak256};
use sqlx::{PgPool, Postgres, Transaction};
use starknet::core::{
    types::{EmittedEvent, Felt},
    utils::get_selector_from_name,
};
use std::{sync::Arc, time::Duration};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use uuid::Uuid;

const INDEXER_SERVICE_NAME: &str = "moros-indexer";
const VAULT_EVENT_STREAM_NAME: &str = "bankroll_vault";
const DICE_VAULT_ID_OFFSET: u64 = 1_000_000_000;
const ROULETTE_VAULT_ID_OFFSET: u64 = 2_000_000_000;
const BACCARAT_VAULT_ID_OFFSET: u64 = 3_000_000_000;

#[derive(Clone)]
struct AppState {
    infra: InfraSnapshot,
    database: Option<PgPool>,
    chain: Option<ChainService>,
    vault_reconcile_interval_ms: u64,
    vault_reconcile_batch_size: i64,
    vault_event_index_interval_ms: u64,
    vault_event_chunk_size: u64,
    vault_event_start_block: u64,
}

#[derive(Debug, Serialize)]
struct RootResponse {
    service: &'static str,
    role: &'static str,
    status: &'static str,
    infra: InfraSnapshot,
    vault_reconcile_interval_ms: u64,
    vault_reconcile_batch_size: i64,
    vault_event_index_interval_ms: u64,
    vault_event_chunk_size: u64,
    vault_event_start_block: u64,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

type ApiResult<T> = Result<Json<T>, ApiError>;

#[derive(Debug, Clone)]
struct VaultEventRecordInput {
    event_fingerprint: String,
    block_number: u64,
    transaction_hash: String,
    event_name: &'static str,
    player_wallet: Option<String>,
    reference_kind: Option<String>,
    reference_id: Option<String>,
    payload: Value,
}

#[derive(Debug, Clone)]
struct ParsedVaultEvent {
    event_name: &'static str,
    player_wallet: Option<String>,
    hand_id: Option<u64>,
    payout: Option<u128>,
    payload: Value,
}

#[derive(Debug, Clone)]
struct VaultHandReference {
    game_kind: &'static str,
    reference_id: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    telemetry::init("moros_indexer");
    let config = ServiceConfig::from_env("moros-indexer", 8083);
    let infra = ServiceInfra::from_config(&config)?;
    let readiness = infra.prepare().await?;
    let vault_reconcile_interval_ms = std::env::var("MOROS_VAULT_RECONCILE_INTERVAL_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(30_000);
    let vault_reconcile_batch_size = std::env::var("MOROS_VAULT_RECONCILE_BATCH_SIZE")
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(250)
        .max(1);
    let vault_event_index_interval_ms = std::env::var("MOROS_VAULT_EVENT_INDEX_INTERVAL_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(7_500);
    let vault_event_chunk_size = std::env::var("MOROS_VAULT_EVENT_CHUNK_SIZE")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(128)
        .max(1);
    let vault_event_start_block = std::env::var("MOROS_VAULT_EVENT_START_BLOCK")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    let state = Arc::new(AppState {
        infra: infra.snapshot(&config, readiness),
        database: infra.database.clone(),
        chain: ChainService::from_config(&config)?,
        vault_reconcile_interval_ms,
        vault_reconcile_batch_size,
        vault_event_index_interval_ms,
        vault_event_chunk_size,
        vault_event_start_block,
    });

    if state.database.is_some() && state.chain.is_some() && state.vault_event_index_interval_ms > 0
    {
        tokio::spawn(index_vault_events_worker(state.clone()));
    }

    if state.database.is_some() && state.chain.is_some() && state.vault_reconcile_interval_ms > 0 {
        tokio::spawn(reconcile_vault_worker(state.clone()));
    }

    let app = base_router::<Arc<AppState>>("moros-indexer")
        .route("/", get(root))
        .route("/v1/hands/{hand_id}", get(get_hand))
        .route("/v1/hands/{hand_id}/view", get(get_hand_view))
        .route("/v1/hands/{hand_id}/fairness", get(get_hand_fairness))
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
        service: "moros-indexer",
        role: "read model and analytics",
        status: "ready",
        infra: state.infra.clone(),
        vault_reconcile_interval_ms: state.vault_reconcile_interval_ms,
        vault_reconcile_batch_size: state.vault_reconcile_batch_size,
        vault_event_index_interval_ms: state.vault_event_index_interval_ms,
        vault_event_chunk_size: state.vault_event_chunk_size,
        vault_event_start_block: state.vault_event_start_block,
    }))
}

async fn index_vault_events_worker(state: Arc<AppState>) {
    loop {
        if let Err(error) = index_vault_events_once(&state).await {
            tracing::error!(error = ?error, "vault event index cycle failed");
        }
        tokio::time::sleep(Duration::from_millis(state.vault_event_index_interval_ms)).await;
    }
}

async fn index_vault_events_once(state: &AppState) -> anyhow::Result<()> {
    let Some(database) = &state.database else {
        return Ok(());
    };
    let Some(chain) = &state.chain else {
        return Ok(());
    };

    let latest_block = chain.fetch_latest_block_number().await?;
    let next_block =
        load_chain_sync_cursor(database, INDEXER_SERVICE_NAME, VAULT_EVENT_STREAM_NAME)
            .await?
            .map(|block| block.saturating_add(1))
            .unwrap_or(state.vault_event_start_block);
    if next_block > latest_block {
        return Ok(());
    }

    let selector_pairs = bankroll_vault_selector_pairs()?;
    let key_filter = Some(vec![
        selector_pairs
            .iter()
            .map(|(selector, _)| *selector)
            .collect::<Vec<_>>(),
    ]);
    let bankroll_vault = chain.contract_addresses().bankroll_vault;
    let mut continuation_token = None;

    loop {
        let page = chain
            .fetch_events(
                bankroll_vault,
                next_block,
                latest_block,
                key_filter.clone(),
                continuation_token.clone(),
                state.vault_event_chunk_size,
            )
            .await?;
        for event in page.events {
            process_vault_event(database, chain, &selector_pairs, event).await?;
        }
        continuation_token = page.continuation_token;
        if continuation_token.is_none() {
            break;
        }
    }

    store_chain_sync_cursor(
        database,
        INDEXER_SERVICE_NAME,
        VAULT_EVENT_STREAM_NAME,
        latest_block,
    )
    .await?;

    Ok(())
}

async fn reconcile_vault_worker(state: Arc<AppState>) {
    loop {
        if let Err(error) = reconcile_vault_once(&state).await {
            tracing::error!(error = ?error, "vault reconciliation cycle failed");
        }
        tokio::time::sleep(Duration::from_millis(state.vault_reconcile_interval_ms)).await;
    }
}

async fn reconcile_vault_once(state: &AppState) -> anyhow::Result<()> {
    let Some(database) = &state.database else {
        return Ok(());
    };
    let Some(chain) = &state.chain else {
        return Ok(());
    };

    let wallets =
        accounts::list_primary_execution_wallets(database, state.vault_reconcile_batch_size)
            .await?;
    for wallet in wallets {
        let user_id = Uuid::parse_str(&wallet.user_id)?;
        let onchain = match chain
            .fetch_player_vault_balances(&wallet.wallet_address)
            .await
        {
            Ok(balances) => balances,
            Err(error) => {
                tracing::warn!(
                    error = ?error,
                    user_id = %wallet.user_id,
                    wallet = %wallet.wallet_address,
                    "failed to fetch onchain vault balance for reconciliation"
                );
                continue;
            }
        };
        let gambling_balance = onchain.gambling_balance.parse::<u128>()?;
        let vault_balance = onchain.vault_balance.parse::<u128>()?;
        balances::reconcile_onchain_vault_balances(
            database,
            user_id,
            gambling_balance,
            vault_balance,
            Some("vault_reconcile"),
            Some(&wallet.wallet_address),
            serde_json::json!({
                "source": "indexer_background_worker",
                "wallet_address": wallet.wallet_address,
                "onchain_player": onchain.player,
            }),
        )
        .await?;
    }

    Ok(())
}

async fn process_vault_event(
    database: &PgPool,
    chain: &ChainService,
    selector_pairs: &[(Felt, &'static str)],
    event: EmittedEvent,
) -> anyhow::Result<()> {
    let parsed = parse_vault_event(selector_pairs, &event)?;
    let reference = match parsed.hand_id {
        Some(hand_id) => resolve_vault_hand_reference(database, chain, hand_id).await?,
        None => None,
    };
    let player_id = match parsed.player_wallet.as_deref() {
        Some(wallet) => accounts::resolve_player_id_by_wallet(database, wallet).await?,
        None => None,
    };

    let onchain_balances = match parsed.player_wallet.as_deref() {
        Some(wallet) if player_id.is_some() => {
            Some(chain.fetch_player_vault_balances(wallet).await?)
        }
        _ => None,
    };
    let record = VaultEventRecordInput {
        event_fingerprint: fingerprint_vault_event(&event),
        block_number: event
            .block_number
            .context("vault event missing block number")?,
        transaction_hash: format!("{:#x}", event.transaction_hash),
        event_name: parsed.event_name,
        player_wallet: parsed.player_wallet.clone(),
        reference_kind: reference.as_ref().map(|value| value.game_kind.to_string()),
        reference_id: reference.as_ref().map(|value| value.reference_id.clone()),
        payload: parsed.payload.clone(),
    };

    let mut tx = database
        .begin()
        .await
        .context("failed to open vault event index transaction")?;
    let inserted = insert_vault_indexed_event_tx(&mut tx, &record).await?;
    if !inserted {
        tx.rollback()
            .await
            .context("failed to rollback duplicate vault event transaction")?;
        return Ok(());
    }

    let Some(player_id) = player_id else {
        tx.commit()
            .await
            .context("failed to commit unbound vault event record")?;
        return Ok(());
    };

    if let Some(reference) = reference.as_ref() {
        match parsed.event_name {
            "HandReserved" => {
                balances::sync_gambling_reservation_from_chain_tx(
                    &mut tx,
                    player_id,
                    reference.game_kind,
                    &reference.reference_id,
                    parsed
                        .payout
                        .context("hand reserved event missing amount")?,
                    json!({
                        "source": "vault_event_indexer",
                        "event_name": parsed.event_name,
                        "transaction_hash": record.transaction_hash,
                        "block_number": record.block_number,
                        "payload": parsed.payload,
                    }),
                )
                .await?;
            }
            "HandSettled" => {
                balances::settle_gambling_reservation_from_chain_tx(
                    &mut tx,
                    player_id,
                    reference.game_kind,
                    &reference.reference_id,
                    parsed.payout.context("hand settled event missing payout")?,
                    json!({
                        "source": "vault_event_indexer",
                        "event_name": parsed.event_name,
                        "transaction_hash": record.transaction_hash,
                        "block_number": record.block_number,
                        "payload": parsed.payload,
                    }),
                )
                .await?;
            }
            "HandVoided" => {
                balances::release_gambling_reservation_from_chain_tx(
                    &mut tx,
                    player_id,
                    reference.game_kind,
                    &reference.reference_id,
                    json!({
                        "source": "vault_event_indexer",
                        "event_name": parsed.event_name,
                        "transaction_hash": record.transaction_hash,
                        "block_number": record.block_number,
                        "payload": parsed.payload,
                    }),
                )
                .await?;
            }
            _ => {}
        }
    }

    if let Some(onchain_balances) = onchain_balances {
        balances::reconcile_onchain_vault_balances_tx(
            &mut tx,
            player_id,
            onchain_balances
                .gambling_balance
                .parse::<u128>()
                .context("invalid onchain gambling balance")?,
            onchain_balances
                .vault_balance
                .parse::<u128>()
                .context("invalid onchain vault balance")?,
            record.reference_kind.as_deref(),
            record.reference_id.as_deref(),
            json!({
                "source": "vault_event_indexer",
                "event_name": record.event_name,
                "transaction_hash": record.transaction_hash,
                "block_number": record.block_number,
                "player_wallet": record.player_wallet,
                "payload": record.payload,
            }),
        )
        .await?;
    }

    tx.commit()
        .await
        .context("failed to commit vault event index transaction")?;
    Ok(())
}

fn parse_vault_event(
    selector_pairs: &[(Felt, &'static str)],
    event: &EmittedEvent,
) -> anyhow::Result<ParsedVaultEvent> {
    let selector = event
        .keys
        .first()
        .context("vault event missing selector key")?;
    let event_name = selector_pairs
        .iter()
        .find_map(|(candidate, name)| (*candidate == *selector).then_some(*name))
        .ok_or_else(|| anyhow!("unsupported bankroll vault event selector {selector:#x}"))?;

    match event_name {
        "PublicDeposited" | "VaultDeposited" | "PublicWithdrawn" => {
            let player_wallet = felt_at_as_hex(&event.data, 0, "player")?;
            let amount = felt_at_as_u128(&event.data, 1, "amount")?;
            Ok(ParsedVaultEvent {
                event_name,
                player_wallet: Some(player_wallet.clone()),
                hand_id: None,
                payout: Some(amount),
                payload: json!({
                    "player": player_wallet,
                    "amount": amount.to_string(),
                }),
            })
        }
        "BalanceMoved" => {
            let player_wallet = felt_at_as_hex(&event.data, 0, "player")?;
            let amount = felt_at_as_u128(&event.data, 1, "amount")?;
            let to_vault = felt_at_as_bool(&event.data, 2, "to_vault")?;
            let gambling_balance = felt_at_as_u128(&event.data, 3, "gambling_balance")?;
            let vault_balance = felt_at_as_u128(&event.data, 4, "vault_balance")?;
            Ok(ParsedVaultEvent {
                event_name,
                player_wallet: Some(player_wallet.clone()),
                hand_id: None,
                payout: Some(amount),
                payload: json!({
                    "player": player_wallet,
                    "amount": amount.to_string(),
                    "to_vault": to_vault,
                    "gambling_balance": gambling_balance.to_string(),
                    "vault_balance": vault_balance.to_string(),
                }),
            })
        }
        "HandReserved" => {
            let player_wallet = felt_at_as_hex(&event.data, 0, "player")?;
            let hand_id = felt_at_as_u64(&event.data, 1, "hand_id")?;
            let amount = felt_at_as_u128(&event.data, 2, "amount")?;
            Ok(ParsedVaultEvent {
                event_name,
                player_wallet: Some(player_wallet.clone()),
                hand_id: Some(hand_id),
                payout: Some(amount),
                payload: json!({
                    "player": player_wallet,
                    "hand_id": hand_id,
                    "amount": amount.to_string(),
                }),
            })
        }
        "HandSettled" => {
            let player_wallet = felt_at_as_hex(&event.data, 0, "player")?;
            let hand_id = felt_at_as_u64(&event.data, 1, "hand_id")?;
            let payout = felt_at_as_u128(&event.data, 2, "payout")?;
            Ok(ParsedVaultEvent {
                event_name,
                player_wallet: Some(player_wallet.clone()),
                hand_id: Some(hand_id),
                payout: Some(payout),
                payload: json!({
                    "player": player_wallet,
                    "hand_id": hand_id,
                    "payout": payout.to_string(),
                }),
            })
        }
        "HandVoided" => {
            let player_wallet = felt_at_as_hex(&event.data, 0, "player")?;
            let hand_id = felt_at_as_u64(&event.data, 1, "hand_id")?;
            let refunded = felt_at_as_u128(&event.data, 2, "refunded")?;
            Ok(ParsedVaultEvent {
                event_name,
                player_wallet: Some(player_wallet.clone()),
                hand_id: Some(hand_id),
                payout: Some(refunded),
                payload: json!({
                    "player": player_wallet,
                    "hand_id": hand_id,
                    "refunded": refunded.to_string(),
                }),
            })
        }
        other => Err(anyhow!("unsupported vault event {other}")),
    }
}

async fn resolve_vault_hand_reference(
    database: &PgPool,
    chain: &ChainService,
    vault_hand_id: u64,
) -> anyhow::Result<Option<VaultHandReference>> {
    if let Some(hand) =
        persistence::get_blackjack_hand_by_chain_hand_id(database, vault_hand_id as i64).await?
    {
        return Ok(Some(VaultHandReference {
            game_kind: "blackjack",
            reference_id: hand.hand_id,
        }));
    }

    if let Some(reference) = resolve_dice_vault_hand_reference(chain, vault_hand_id).await {
        return Ok(Some(reference));
    }
    if let Some(reference) = resolve_roulette_vault_hand_reference(chain, vault_hand_id).await {
        return Ok(Some(reference));
    }
    if let Some(reference) = resolve_baccarat_vault_hand_reference(chain, vault_hand_id).await {
        return Ok(Some(reference));
    }

    tracing::warn!(
        vault_hand_id,
        "unable to map bankroll vault hand id to a Moros reservation"
    );
    Ok(None)
}

async fn resolve_dice_vault_hand_reference(
    chain: &ChainService,
    vault_hand_id: u64,
) -> Option<VaultHandReference> {
    let round_id = vault_hand_id % DICE_VAULT_ID_OFFSET;
    if round_id == 0 {
        return None;
    }
    let round = chain.fetch_dice_round(round_id).await.ok()?;
    let expected = DICE_VAULT_ID_OFFSET
        .checked_add(round.table_id.checked_mul(DICE_VAULT_ID_OFFSET)?)?
        .checked_add(round.round_id)?;
    (expected == vault_hand_id).then_some(VaultHandReference {
        game_kind: "dice",
        reference_id: round.commitment_id.to_string(),
    })
}

async fn resolve_roulette_vault_hand_reference(
    chain: &ChainService,
    vault_hand_id: u64,
) -> Option<VaultHandReference> {
    let spin_id = vault_hand_id % ROULETTE_VAULT_ID_OFFSET;
    if spin_id == 0 {
        return None;
    }
    let spin = chain.fetch_roulette_spin(spin_id).await.ok()?;
    let expected = ROULETTE_VAULT_ID_OFFSET
        .checked_add(spin.table_id.checked_mul(ROULETTE_VAULT_ID_OFFSET)?)?
        .checked_add(spin.spin_id)?;
    (expected == vault_hand_id).then_some(VaultHandReference {
        game_kind: "roulette",
        reference_id: spin.commitment_id.to_string(),
    })
}

async fn resolve_baccarat_vault_hand_reference(
    chain: &ChainService,
    vault_hand_id: u64,
) -> Option<VaultHandReference> {
    let round_id = vault_hand_id % BACCARAT_VAULT_ID_OFFSET;
    if round_id == 0 {
        return None;
    }
    let round = chain.fetch_baccarat_round(round_id).await.ok()?;
    let expected = BACCARAT_VAULT_ID_OFFSET
        .checked_add(round.table_id.checked_mul(BACCARAT_VAULT_ID_OFFSET)?)?
        .checked_add(round.round_id)?;
    (expected == vault_hand_id).then_some(VaultHandReference {
        game_kind: "baccarat",
        reference_id: round.commitment_id.to_string(),
    })
}

async fn load_chain_sync_cursor(
    pool: &PgPool,
    service_name: &str,
    stream_name: &str,
) -> anyhow::Result<Option<u64>> {
    let cursor = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT cursor_block
        FROM chain_sync_cursors
        WHERE service_name = $1
          AND stream_name = $2
        "#,
    )
    .bind(service_name)
    .bind(stream_name)
    .fetch_optional(pool)
    .await
    .context("failed to read chain sync cursor")?;

    cursor
        .map(|value| u64::try_from(value).context("negative chain sync cursor"))
        .transpose()
}

async fn store_chain_sync_cursor(
    pool: &PgPool,
    service_name: &str,
    stream_name: &str,
    cursor_block: u64,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO chain_sync_cursors (service_name, stream_name, cursor_block)
        VALUES ($1, $2, $3)
        ON CONFLICT (service_name, stream_name) DO UPDATE
        SET cursor_block = EXCLUDED.cursor_block,
            updated_at = NOW()
        "#,
    )
    .bind(service_name)
    .bind(stream_name)
    .bind(i64::try_from(cursor_block).context("chain sync cursor does not fit in i64")?)
    .execute(pool)
    .await
    .context("failed to store chain sync cursor")?;
    Ok(())
}

async fn insert_vault_indexed_event_tx(
    tx: &mut Transaction<'_, Postgres>,
    record: &VaultEventRecordInput,
) -> anyhow::Result<bool> {
    let result = sqlx::query(
        r#"
        INSERT INTO vault_indexed_events (
            id,
            stream_name,
            event_fingerprint,
            block_number,
            transaction_hash,
            event_name,
            player_wallet,
            reference_kind,
            reference_id,
            payload
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10::jsonb)
        ON CONFLICT (stream_name, event_fingerprint) DO NOTHING
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(VAULT_EVENT_STREAM_NAME)
    .bind(&record.event_fingerprint)
    .bind(i64::try_from(record.block_number).context("block number does not fit in i64")?)
    .bind(&record.transaction_hash)
    .bind(record.event_name)
    .bind(record.player_wallet.as_deref())
    .bind(record.reference_kind.as_deref())
    .bind(record.reference_id.as_deref())
    .bind(record.payload.to_string())
    .execute(&mut **tx)
    .await
    .context("failed to insert indexed vault event")?;
    Ok(result.rows_affected() > 0)
}

fn bankroll_vault_selector_pairs() -> anyhow::Result<Vec<(Felt, &'static str)>> {
    Ok(vec![
        (
            get_selector_from_name("PublicDeposited")
                .context("missing selector for PublicDeposited")?,
            "PublicDeposited",
        ),
        (
            get_selector_from_name("VaultDeposited")
                .context("missing selector for VaultDeposited")?,
            "VaultDeposited",
        ),
        (
            get_selector_from_name("BalanceMoved").context("missing selector for BalanceMoved")?,
            "BalanceMoved",
        ),
        (
            get_selector_from_name("HandReserved").context("missing selector for HandReserved")?,
            "HandReserved",
        ),
        (
            get_selector_from_name("HandSettled").context("missing selector for HandSettled")?,
            "HandSettled",
        ),
        (
            get_selector_from_name("HandVoided").context("missing selector for HandVoided")?,
            "HandVoided",
        ),
        (
            get_selector_from_name("PublicWithdrawn")
                .context("missing selector for PublicWithdrawn")?,
            "PublicWithdrawn",
        ),
    ])
}

fn fingerprint_vault_event(event: &EmittedEvent) -> String {
    let mut digest = Keccak256::new();
    digest.update(format!("{:#x}", event.from_address));
    digest.update(b"|");
    digest.update(format!("{:#x}", event.transaction_hash));
    digest.update(b"|");
    digest.update(
        event
            .block_number
            .unwrap_or_default()
            .to_string()
            .as_bytes(),
    );
    for key in &event.keys {
        digest.update(b"|k:");
        digest.update(format!("{key:#x}"));
    }
    for value in &event.data {
        digest.update(b"|d:");
        digest.update(format!("{value:#x}"));
    }
    hex::encode(digest.finalize())
}

fn felt_at_as_hex(values: &[Felt], index: usize, label: &str) -> anyhow::Result<String> {
    Ok(format!(
        "{:#x}",
        values
            .get(index)
            .with_context(|| format!("vault event missing {label}"))?
    ))
}

fn felt_at_as_u64(values: &[Felt], index: usize, label: &str) -> anyhow::Result<u64> {
    values
        .get(index)
        .with_context(|| format!("vault event missing {label}"))?
        .to_string()
        .parse::<u64>()
        .with_context(|| format!("failed to parse vault event {label} as u64"))
}

fn felt_at_as_u128(values: &[Felt], index: usize, label: &str) -> anyhow::Result<u128> {
    values
        .get(index)
        .with_context(|| format!("vault event missing {label}"))?
        .to_string()
        .parse::<u128>()
        .with_context(|| format!("failed to parse vault event {label} as u128"))
}

fn felt_at_as_bool(values: &[Felt], index: usize, label: &str) -> anyhow::Result<bool> {
    Ok(values
        .get(index)
        .with_context(|| format!("vault event missing {label}"))?
        != &Felt::ZERO)
}

async fn get_hand(
    State(state): State<Arc<AppState>>,
    Path(hand_id): Path<String>,
) -> ApiResult<persistence::BlackjackHandRecord> {
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

    Ok(Json(hand))
}

async fn get_hand_view(
    State(state): State<Arc<AppState>>,
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
    Path(hand_id): Path<String>,
) -> ApiResult<moros_common::blackjack::BlackjackFairnessArtifactView> {
    let database = state
        .database
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("database is not configured"))?;
    let hand_id = Uuid::parse_str(&hand_id)
        .map_err(|error| ApiError::bad_request(format!("invalid hand_id: {error}")))?;

    let snapshot = persistence::get_blackjack_snapshot(database, hand_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("hand snapshot not found"))?;

    Ok(Json(blackjack::fairness_artifact_view(&snapshot)))
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
        tracing::error!(error = ?error, "indexer request failed");
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
