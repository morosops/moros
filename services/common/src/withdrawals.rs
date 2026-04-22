use crate::{accounts, balances};
use anyhow::{Context, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WithdrawalRequestRecord {
    pub withdrawal_id: String,
    pub user_id: String,
    pub requested_by_wallet: Option<String>,
    pub source_balance: String,
    pub destination_chain_key: String,
    pub destination_asset_symbol: String,
    pub destination_address: String,
    pub amount_raw: String,
    pub route_kind: String,
    pub status: String,
    pub route_job_id: Option<String>,
    pub destination_tx_hash: Option<String>,
    pub failure_reason: Option<String>,
    pub metadata: Value,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CreateWithdrawalInput {
    pub player_id: Uuid,
    pub requested_by_wallet: Option<String>,
    pub source_balance: String,
    pub destination_chain_key: String,
    pub destination_asset_symbol: String,
    pub destination_address: String,
    pub amount_raw: u128,
    pub destination_tx_hash: Option<String>,
    pub metadata: Value,
}

pub async fn create_withdrawal_request(
    pool: &PgPool,
    input: CreateWithdrawalInput,
) -> anyhow::Result<WithdrawalRequestRecord> {
    if input.amount_raw == 0 {
        return Err(anyhow!("withdrawal amount must be greater than zero"));
    }

    let source_balance = normalize_source_balance(&input.source_balance)?;
    let destination_chain_key =
        normalize_required(&input.destination_chain_key, "destination chain")?;
    let destination_asset_symbol =
        normalize_required(&input.destination_asset_symbol, "destination asset")?.to_uppercase();
    let destination_address =
        normalize_required(&input.destination_address, "destination address")?;
    let requested_by_wallet = input
        .requested_by_wallet
        .as_deref()
        .map(accounts::normalize_wallet_address)
        .filter(|value| !value.is_empty());

    let withdrawal_id = Uuid::now_v7();
    let amount_raw = input.amount_raw.to_string();
    let route_kind = route_kind_for(&destination_chain_key, &destination_asset_symbol);
    let destination_tx_hash = input
        .destination_tx_hash
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let status = if destination_tx_hash.is_some() {
        "submitted"
    } else {
        "queued"
    };
    let mut tx = pool
        .begin()
        .await
        .context("failed to open withdrawal transaction")?;

    balances::debit_balance_tx(
        &mut tx,
        input.player_id,
        &source_balance,
        input.amount_raw,
        "withdrawal_requested",
        Some("withdrawal_request"),
        Some(&withdrawal_id.to_string()),
        serde_json::json!({
            "destination_chain_key": destination_chain_key.clone(),
            "destination_asset_symbol": destination_asset_symbol.clone(),
            "destination_address": destination_address.clone(),
            "requested_by_wallet": requested_by_wallet.clone(),
            "route_kind": route_kind.clone(),
        }),
    )
    .await?;

    sqlx::query(
        r#"
        INSERT INTO withdrawal_requests (
            id,
            player_id,
            requested_by_wallet,
            source_balance,
            destination_chain_key,
            destination_asset_symbol,
            destination_address,
            amount_raw,
            route_kind,
            status,
            destination_tx_hash,
            completed_at,
            metadata
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, NULL, $12::jsonb)
        "#,
    )
    .bind(withdrawal_id)
    .bind(input.player_id)
    .bind(requested_by_wallet.as_deref())
    .bind(&source_balance)
    .bind(&destination_chain_key)
    .bind(&destination_asset_symbol)
    .bind(&destination_address)
    .bind(&amount_raw)
    .bind(&route_kind)
    .bind(status)
    .bind(destination_tx_hash.as_deref())
    .bind(input.metadata.to_string())
    .execute(&mut *tx)
    .await
    .context("failed to insert withdrawal request")?;

    insert_withdrawal_event_tx(
        &mut tx,
        withdrawal_id,
        if status == "submitted" {
            "withdrawal_submitted"
        } else {
            "withdrawal_queued"
        },
        serde_json::json!({
            "amount_raw": amount_raw,
            "source_balance": source_balance,
            "destination_chain_key": destination_chain_key,
            "destination_asset_symbol": destination_asset_symbol,
            "destination_address": destination_address,
            "route_kind": route_kind,
            "destination_tx_hash": destination_tx_hash,
        }),
    )
    .await?;

    let record = get_withdrawal_request_tx(&mut tx, withdrawal_id).await?;
    tx.commit()
        .await
        .context("failed to commit withdrawal transaction")?;
    Ok(record)
}

pub async fn list_withdrawal_requests(
    pool: &PgPool,
    player_id: Uuid,
    limit: i64,
) -> anyhow::Result<Vec<WithdrawalRequestRecord>> {
    let rows = sqlx::query(
        r#"
        SELECT
            id,
            player_id,
            requested_by_wallet,
            source_balance,
            destination_chain_key,
            destination_asset_symbol,
            destination_address,
            amount_raw,
            route_kind,
            status,
            route_job_id,
            destination_tx_hash,
            failure_reason,
            metadata,
            TO_CHAR(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS created_at,
            TO_CHAR(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS updated_at,
            TO_CHAR(completed_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS completed_at
        FROM withdrawal_requests
        WHERE player_id = $1
        ORDER BY created_at DESC
        LIMIT $2
        "#,
    )
    .bind(player_id)
    .bind(limit.clamp(1, 100))
    .fetch_all(pool)
    .await
    .context("failed to list withdrawal requests")?;

    rows.into_iter().map(hydrate_withdrawal_request).collect()
}

pub async fn mark_withdrawal_submitted(
    pool: &PgPool,
    withdrawal_id: Uuid,
    destination_tx_hash: &str,
    metadata: Value,
) -> anyhow::Result<WithdrawalRequestRecord> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to open withdrawal submission transaction")?;
    sqlx::query(
        r#"
        UPDATE withdrawal_requests
        SET status = 'submitted',
            destination_tx_hash = $2,
            metadata = metadata || $3::jsonb,
            updated_at = NOW()
        WHERE id = $1
          AND status IN ('queued', 'processing')
        "#,
    )
    .bind(withdrawal_id)
    .bind(destination_tx_hash)
    .bind(metadata.to_string())
    .execute(&mut *tx)
    .await
    .context("failed to mark withdrawal submitted")?;
    insert_withdrawal_event_tx(
        &mut tx,
        withdrawal_id,
        "withdrawal_submitted",
        serde_json::json!({
            "destination_tx_hash": destination_tx_hash,
        }),
    )
    .await?;
    let record = get_withdrawal_request_tx(&mut tx, withdrawal_id).await?;
    tx.commit()
        .await
        .context("failed to commit withdrawal submission transaction")?;
    Ok(record)
}

pub async fn fail_withdrawal_and_refund(
    pool: &PgPool,
    withdrawal_id: Uuid,
    failure_reason: &str,
) -> anyhow::Result<WithdrawalRequestRecord> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to open withdrawal refund transaction")?;
    let request = get_withdrawal_request_for_update_tx(&mut tx, withdrawal_id).await?;
    if request.status != "failed" && request.status != "completed" {
        let amount = request
            .amount_raw
            .parse::<u128>()
            .context("invalid withdrawal amount")?;
        balances::credit_balance_tx(
            &mut tx,
            Uuid::parse_str(&request.user_id).context("invalid withdrawal user_id")?,
            &request.source_balance,
            amount,
            "withdrawal_refunded",
            Some("withdrawal_request"),
            Some(&request.withdrawal_id),
            serde_json::json!({
                "failure_reason": failure_reason,
            }),
        )
        .await?;
    }

    sqlx::query(
        r#"
        UPDATE withdrawal_requests
        SET status = 'failed',
            failure_reason = $2,
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(withdrawal_id)
    .bind(failure_reason)
    .execute(&mut *tx)
    .await
    .context("failed to mark withdrawal failed")?;
    insert_withdrawal_event_tx(
        &mut tx,
        withdrawal_id,
        "withdrawal_failed_refunded",
        serde_json::json!({
            "failure_reason": failure_reason,
        }),
    )
    .await?;
    let record = get_withdrawal_request_tx(&mut tx, withdrawal_id).await?;
    tx.commit()
        .await
        .context("failed to commit withdrawal refund transaction")?;
    Ok(record)
}

async fn insert_withdrawal_event_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    withdrawal_id: Uuid,
    event_type: &str,
    payload: Value,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO withdrawal_events (
            id,
            withdrawal_id,
            event_type,
            payload
        )
        VALUES ($1, $2, $3, $4::jsonb)
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(withdrawal_id)
    .bind(event_type)
    .bind(payload.to_string())
    .execute(&mut **tx)
    .await
    .context("failed to insert withdrawal event")?;
    Ok(())
}

async fn get_withdrawal_request_for_update_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    withdrawal_id: Uuid,
) -> anyhow::Result<WithdrawalRequestRecord> {
    let row = sqlx::query(
        r#"
        SELECT
            id,
            player_id,
            requested_by_wallet,
            source_balance,
            destination_chain_key,
            destination_asset_symbol,
            destination_address,
            amount_raw,
            route_kind,
            status,
            route_job_id,
            destination_tx_hash,
            failure_reason,
            metadata,
            TO_CHAR(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS created_at,
            TO_CHAR(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS updated_at,
            TO_CHAR(completed_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS completed_at
        FROM withdrawal_requests
        WHERE id = $1
        FOR UPDATE
        "#,
    )
    .bind(withdrawal_id)
    .fetch_one(&mut **tx)
    .await
    .context("failed to lock withdrawal request")?;
    hydrate_withdrawal_request(row)
}

async fn get_withdrawal_request_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    withdrawal_id: Uuid,
) -> anyhow::Result<WithdrawalRequestRecord> {
    let row = sqlx::query(
        r#"
        SELECT
            id,
            player_id,
            requested_by_wallet,
            source_balance,
            destination_chain_key,
            destination_asset_symbol,
            destination_address,
            amount_raw,
            route_kind,
            status,
            route_job_id,
            destination_tx_hash,
            failure_reason,
            metadata,
            TO_CHAR(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS created_at,
            TO_CHAR(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS updated_at,
            TO_CHAR(completed_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS completed_at
        FROM withdrawal_requests
        WHERE id = $1
        "#,
    )
    .bind(withdrawal_id)
    .fetch_one(&mut **tx)
    .await
    .context("failed to fetch withdrawal request")?;
    hydrate_withdrawal_request(row)
}

fn normalize_source_balance(value: &str) -> anyhow::Result<String> {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "vault" | "gambling" => Ok(normalized),
        _ => Err(anyhow!("source balance must be vault or gambling")),
    }
}

fn normalize_required(value: &str, label: &str) -> anyhow::Result<String> {
    let normalized = value.trim().to_string();
    if normalized.is_empty() {
        return Err(anyhow!("{label} is required"));
    }
    Ok(normalized)
}

fn route_kind_for(destination_chain_key: &str, destination_asset_symbol: &str) -> String {
    if destination_chain_key.eq_ignore_ascii_case("starknet")
        && destination_asset_symbol.eq_ignore_ascii_case("STRK")
    {
        "starknet_strk_transfer".to_string()
    } else {
        "starkzap_withdraw_route".to_string()
    }
}

fn hydrate_withdrawal_request(
    row: sqlx::postgres::PgRow,
) -> anyhow::Result<WithdrawalRequestRecord> {
    Ok(WithdrawalRequestRecord {
        withdrawal_id: row
            .try_get::<Uuid, _>("id")
            .context("missing withdrawal id")?
            .to_string(),
        user_id: row
            .try_get::<Uuid, _>("player_id")
            .context("missing withdrawal player_id")?
            .to_string(),
        requested_by_wallet: row
            .try_get::<Option<String>, _>("requested_by_wallet")
            .context("missing requested_by_wallet")?,
        source_balance: row
            .try_get::<String, _>("source_balance")
            .context("missing source_balance")?,
        destination_chain_key: row
            .try_get::<String, _>("destination_chain_key")
            .context("missing destination_chain_key")?,
        destination_asset_symbol: row
            .try_get::<String, _>("destination_asset_symbol")
            .context("missing destination_asset_symbol")?,
        destination_address: row
            .try_get::<String, _>("destination_address")
            .context("missing destination_address")?,
        amount_raw: row
            .try_get::<String, _>("amount_raw")
            .context("missing amount_raw")?,
        route_kind: row
            .try_get::<String, _>("route_kind")
            .context("missing route_kind")?,
        status: row
            .try_get::<String, _>("status")
            .context("missing status")?,
        route_job_id: row
            .try_get::<Option<Uuid>, _>("route_job_id")
            .context("missing route_job_id")?
            .map(|value| value.to_string()),
        destination_tx_hash: row
            .try_get::<Option<String>, _>("destination_tx_hash")
            .context("missing destination_tx_hash")?,
        failure_reason: row
            .try_get::<Option<String>, _>("failure_reason")
            .context("missing failure_reason")?,
        metadata: row
            .try_get::<Value, _>("metadata")
            .context("missing withdrawal metadata")?,
        created_at: row
            .try_get::<String, _>("created_at")
            .context("missing created_at")?,
        updated_at: row
            .try_get::<String, _>("updated_at")
            .context("missing updated_at")?,
        completed_at: row
            .try_get::<Option<String>, _>("completed_at")
            .context("missing completed_at")?,
    })
}
