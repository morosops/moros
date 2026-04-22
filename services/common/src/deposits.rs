use crate::balances;
use anyhow::{Context, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepositSupportedAssetSeed {
    pub id: String,
    pub chain_key: String,
    pub chain_family: String,
    pub network: String,
    pub chain_id: String,
    pub asset_symbol: String,
    pub asset_address: String,
    pub asset_decimals: i32,
    pub route_kind: String,
    pub watch_mode: String,
    pub min_amount: String,
    pub max_amount: String,
    pub confirmations_required: i32,
    pub status: String,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepositSupportedAssetRecord {
    pub id: String,
    pub chain_key: String,
    pub chain_family: String,
    pub network: String,
    pub chain_id: String,
    pub asset_symbol: String,
    pub asset_address: String,
    pub asset_decimals: i32,
    pub route_kind: String,
    pub watch_mode: String,
    pub min_amount: String,
    pub max_amount: String,
    pub confirmations_required: i32,
    pub status: String,
    pub metadata: Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateDepositChannelInput {
    pub player_id: Uuid,
    pub wallet_address: Option<String>,
    pub asset_id: String,
    pub chain_key: String,
    pub deposit_address: String,
    pub qr_payload: String,
    pub route_kind: String,
    pub watch_from_block: Option<i64>,
    pub last_scanned_block: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepositChannelRecord {
    pub channel_id: String,
    pub user_id: String,
    pub wallet_address: Option<String>,
    pub username: Option<String>,
    pub asset_id: String,
    pub chain_key: String,
    pub asset_symbol: String,
    pub deposit_address: String,
    pub qr_payload: String,
    pub route_kind: String,
    pub status: String,
    pub watch_from_block: Option<i64>,
    pub last_scanned_block: Option<i64>,
    pub last_seen_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObserveDepositTransferInput {
    pub channel_id: Uuid,
    pub asset_id: String,
    pub chain_key: String,
    pub deposit_address: String,
    pub sender_address: Option<String>,
    pub tx_hash: String,
    pub block_number: Option<i64>,
    pub block_hash: Option<String>,
    pub amount_raw: String,
    pub confirmations: i32,
    pub required_confirmations: i32,
    pub credit_target: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepositTransferRecord {
    pub transfer_id: String,
    pub channel_id: String,
    pub user_id: String,
    pub wallet_address: Option<String>,
    pub username: Option<String>,
    pub asset_id: String,
    pub chain_key: String,
    pub asset_symbol: String,
    pub deposit_address: String,
    pub sender_address: Option<String>,
    pub tx_hash: String,
    pub block_number: Option<i64>,
    pub amount_raw: String,
    pub amount_display: String,
    pub confirmations: i32,
    pub required_confirmations: i32,
    pub status: String,
    pub risk_state: String,
    pub credit_target: Option<String>,
    pub destination_tx_hash: Option<String>,
    pub detected_at: String,
    pub confirmed_at: Option<String>,
    pub completed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepositRouteJobRecord {
    pub job_id: String,
    pub transfer_id: String,
    pub job_type: String,
    pub status: String,
    pub attempts: i32,
    pub payload: Value,
    pub response: Option<Value>,
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepositRiskFlagRecord {
    pub flag_id: String,
    pub transfer_id: String,
    pub code: String,
    pub severity: String,
    pub description: String,
    pub resolution_status: String,
    pub resolution_notes: Option<String>,
    pub created_at: String,
    pub resolved_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepositRecoveryRecord {
    pub recovery_id: String,
    pub transfer_id: String,
    pub reason: String,
    pub notes: Option<String>,
    pub requested_by: Option<String>,
    pub status: String,
    pub resolution_notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub resolved_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateDepositRecoveryInput {
    pub transfer_id: Uuid,
    pub reason: String,
    pub notes: Option<String>,
    pub requested_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveDepositRecoveryInput {
    pub resolution_notes: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveRiskFlagInput {
    pub resolution_status: String,
    pub resolution_notes: Option<String>,
}

pub async fn seed_supported_assets(
    pool: &PgPool,
    assets: &[DepositSupportedAssetSeed],
) -> anyhow::Result<()> {
    for asset in assets {
        sqlx::query(
            r#"
            INSERT INTO deposit_supported_assets (
                id,
                chain_key,
                chain_family,
                network,
                chain_id,
                asset_symbol,
                asset_address,
                asset_decimals,
                route_kind,
                watch_mode,
                min_amount,
                max_amount,
                confirmations_required,
                status,
                metadata
            )
            VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15::jsonb
            )
            ON CONFLICT (id, chain_key) DO UPDATE
            SET
                chain_family = EXCLUDED.chain_family,
                network = EXCLUDED.network,
                chain_id = EXCLUDED.chain_id,
                asset_symbol = EXCLUDED.asset_symbol,
                asset_address = EXCLUDED.asset_address,
                asset_decimals = EXCLUDED.asset_decimals,
                route_kind = EXCLUDED.route_kind,
                watch_mode = EXCLUDED.watch_mode,
                min_amount = EXCLUDED.min_amount,
                max_amount = EXCLUDED.max_amount,
                confirmations_required = EXCLUDED.confirmations_required,
                status = EXCLUDED.status,
                metadata = EXCLUDED.metadata,
                updated_at = NOW()
            "#,
        )
        .bind(&asset.id)
        .bind(&asset.chain_key)
        .bind(&asset.chain_family)
        .bind(&asset.network)
        .bind(&asset.chain_id)
        .bind(&asset.asset_symbol)
        .bind(&asset.asset_address)
        .bind(asset.asset_decimals)
        .bind(&asset.route_kind)
        .bind(&asset.watch_mode)
        .bind(&asset.min_amount)
        .bind(&asset.max_amount)
        .bind(asset.confirmations_required)
        .bind(&asset.status)
        .bind(asset.metadata.to_string())
        .execute(pool)
        .await
        .context("failed to upsert supported deposit asset")?;
    }

    Ok(())
}

pub async fn list_supported_assets(
    pool: &PgPool,
) -> anyhow::Result<Vec<DepositSupportedAssetRecord>> {
    let rows = sqlx::query(
        r#"
        SELECT
            id,
            chain_key,
            chain_family,
            network,
            chain_id,
            asset_symbol,
            asset_address,
            asset_decimals,
            route_kind,
            watch_mode,
            min_amount,
            max_amount,
            confirmations_required,
            status,
            metadata,
            TO_CHAR(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS created_at,
            TO_CHAR(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS updated_at
        FROM deposit_supported_assets
        ORDER BY chain_key ASC, asset_symbol ASC
        "#,
    )
    .fetch_all(pool)
    .await
    .context("failed to list supported deposit assets")?;

    rows.into_iter().map(hydrate_supported_asset).collect()
}

pub async fn get_supported_asset(
    pool: &PgPool,
    asset_id: &str,
    chain_key: &str,
) -> anyhow::Result<Option<DepositSupportedAssetRecord>> {
    let row = sqlx::query(
        r#"
        SELECT
            id,
            chain_key,
            chain_family,
            network,
            chain_id,
            asset_symbol,
            asset_address,
            asset_decimals,
            route_kind,
            watch_mode,
            min_amount,
            max_amount,
            confirmations_required,
            status,
            metadata,
            TO_CHAR(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS created_at,
            TO_CHAR(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS updated_at
        FROM deposit_supported_assets
        WHERE id = $1 AND chain_key = $2
        "#,
    )
    .bind(asset_id)
    .bind(chain_key)
    .fetch_optional(pool)
    .await
    .context("failed to read supported deposit asset")?;

    row.map(hydrate_supported_asset).transpose()
}

pub async fn get_or_create_deposit_channel(
    pool: &PgPool,
    input: &CreateDepositChannelInput,
) -> anyhow::Result<DepositChannelRecord> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to open deposit channel transaction")?;
    balances::ensure_balance_account_tx(&mut tx, input.player_id).await?;

    if let Some(existing) = sqlx::query(
        r#"
        SELECT
            c.id,
            p.id AS player_id,
            p.wallet_address,
            pp.username,
            c.asset_id,
            c.chain_key,
            a.asset_symbol,
            c.deposit_address,
            c.qr_payload,
            c.route_kind,
            c.status,
            c.watch_from_block,
            c.last_scanned_block,
            TO_CHAR(c.last_seen_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS last_seen_at,
            TO_CHAR(c.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS created_at,
            TO_CHAR(c.updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS updated_at
        FROM deposit_channels c
        INNER JOIN players p ON p.id = c.player_id
        INNER JOIN deposit_supported_assets a ON a.id = c.asset_id AND a.chain_key = c.chain_key
        LEFT JOIN player_profiles pp ON pp.player_id = p.id
        WHERE c.player_id = $1
          AND c.asset_id = $2
          AND c.chain_key = $3
          AND c.status = 'active'
        "#,
    )
    .bind(input.player_id)
    .bind(&input.asset_id)
    .bind(&input.chain_key)
    .fetch_optional(&mut *tx)
    .await
    .context("failed to query deposit channel")?
    {
        let existing_id = existing
            .try_get::<Uuid, _>("id")
            .context("missing existing channel id")?;
        let existing_address = existing
            .try_get::<String, _>("deposit_address")
            .context("missing existing deposit address")?;
        if !existing_address.eq_ignore_ascii_case(&input.deposit_address) {
            sqlx::query(
                r#"
                UPDATE deposit_channels
                SET deposit_address = $2,
                    qr_payload = $3,
                    route_kind = $4,
                    watch_from_block = COALESCE($5, watch_from_block),
                    last_scanned_block = COALESCE($6, last_scanned_block),
                    updated_at = NOW()
                WHERE id = $1
                "#,
            )
            .bind(existing_id)
            .bind(&input.deposit_address)
            .bind(&input.qr_payload)
            .bind(&input.route_kind)
            .bind(input.watch_from_block)
            .bind(input.last_scanned_block)
            .execute(&mut *tx)
            .await
            .context("failed to migrate deposit channel to chain-level address")?;

            insert_deposit_event(
                &mut tx,
                "channel",
                existing_id,
                "channel.address_migrated",
                json!({
                    "user_id": input.player_id.to_string(),
                    "asset_id": input.asset_id,
                    "chain_key": input.chain_key,
                    "previous_deposit_address": existing_address,
                    "deposit_address": input.deposit_address,
                }),
            )
            .await?;
            tx.commit()
                .await
                .context("failed to commit migrated deposit channel transaction")?;
            return get_deposit_channel(pool, existing_id)
                .await?
                .context("migrated deposit channel was not readable after update");
        }

        tx.commit()
            .await
            .context("failed to commit existing deposit channel transaction")?;
        return hydrate_deposit_channel(existing);
    }

    let channel_id = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO deposit_channels (
            id,
            player_id,
            asset_id,
            chain_key,
            deposit_address,
            qr_payload,
            route_kind,
            watch_from_block,
            last_scanned_block,
            status
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'active')
        "#,
    )
    .bind(channel_id)
    .bind(input.player_id)
    .bind(&input.asset_id)
    .bind(&input.chain_key)
    .bind(&input.deposit_address)
    .bind(&input.qr_payload)
    .bind(&input.route_kind)
    .bind(input.watch_from_block)
    .bind(input.last_scanned_block)
    .execute(&mut *tx)
    .await
    .context("failed to insert deposit channel")?;

    insert_deposit_event(
        &mut tx,
        "channel",
        channel_id,
        "channel.created",
        json!({
            "user_id": input.player_id.to_string(),
            "wallet_address": input.wallet_address,
            "asset_id": input.asset_id,
            "chain_key": input.chain_key,
            "deposit_address": input.deposit_address,
        }),
    )
    .await?;

    tx.commit()
        .await
        .context("failed to commit deposit channel transaction")?;

    get_deposit_channel(pool, channel_id)
        .await?
        .context("deposit channel was not readable after insert")
}

pub async fn list_watchable_channels(pool: &PgPool) -> anyhow::Result<Vec<DepositChannelRecord>> {
    let rows = sqlx::query(
        r#"
        SELECT
            c.id,
            p.id AS player_id,
            p.wallet_address,
            pp.username,
            c.asset_id,
            c.chain_key,
            a.asset_symbol,
            c.deposit_address,
            c.qr_payload,
            c.route_kind,
            c.status,
            c.watch_from_block,
            c.last_scanned_block,
            TO_CHAR(c.last_seen_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS last_seen_at,
            TO_CHAR(c.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS created_at,
            TO_CHAR(c.updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS updated_at
        FROM deposit_channels c
        INNER JOIN players p ON p.id = c.player_id
        INNER JOIN deposit_supported_assets a ON a.id = c.asset_id AND a.chain_key = c.chain_key
        LEFT JOIN player_profiles pp ON pp.player_id = p.id
        WHERE c.status = 'active'
          AND a.status = 'enabled'
        ORDER BY c.created_at ASC
        "#,
    )
    .fetch_all(pool)
    .await
    .context("failed to list watchable deposit channels")?;

    rows.into_iter().map(hydrate_deposit_channel).collect()
}

pub async fn update_channel_scan_block(
    pool: &PgPool,
    channel_id: Uuid,
    last_scanned_block: i64,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        UPDATE deposit_channels
        SET last_scanned_block = $2,
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(channel_id)
    .bind(last_scanned_block)
    .execute(pool)
    .await
    .context("failed to update deposit channel scan block")?;
    Ok(())
}

pub async fn get_deposit_channel(
    pool: &PgPool,
    channel_id: Uuid,
) -> anyhow::Result<Option<DepositChannelRecord>> {
    let row = sqlx::query(
        r#"
        SELECT
            c.id,
            p.id AS player_id,
            p.wallet_address,
            pp.username,
            c.asset_id,
            c.chain_key,
            a.asset_symbol,
            c.deposit_address,
            c.qr_payload,
            c.route_kind,
            c.status,
            c.watch_from_block,
            c.last_scanned_block,
            TO_CHAR(c.last_seen_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS last_seen_at,
            TO_CHAR(c.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS created_at,
            TO_CHAR(c.updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS updated_at
        FROM deposit_channels c
        INNER JOIN players p ON p.id = c.player_id
        INNER JOIN deposit_supported_assets a ON a.id = c.asset_id AND a.chain_key = c.chain_key
        LEFT JOIN player_profiles pp ON pp.player_id = p.id
        WHERE c.id = $1
        "#,
    )
    .bind(channel_id)
    .fetch_optional(pool)
    .await
    .context("failed to fetch deposit channel")?;

    row.map(hydrate_deposit_channel).transpose()
}

pub async fn get_deposit_channel_by_address(
    pool: &PgPool,
    deposit_address: &str,
) -> anyhow::Result<Option<DepositChannelRecord>> {
    let row = sqlx::query(
        r#"
        SELECT
            c.id,
            p.id AS player_id,
            p.wallet_address,
            pp.username,
            c.asset_id,
            c.chain_key,
            a.asset_symbol,
            c.deposit_address,
            c.qr_payload,
            c.route_kind,
            c.status,
            c.watch_from_block,
            c.last_scanned_block,
            TO_CHAR(c.last_seen_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS last_seen_at,
            TO_CHAR(c.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS created_at,
            TO_CHAR(c.updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS updated_at
        FROM deposit_channels c
        INNER JOIN players p ON p.id = c.player_id
        INNER JOIN deposit_supported_assets a ON a.id = c.asset_id AND a.chain_key = c.chain_key
        LEFT JOIN player_profiles pp ON pp.player_id = p.id
        WHERE LOWER(c.deposit_address) = LOWER($1)
        ORDER BY
            CASE WHEN c.status = 'active' THEN 0 ELSE 1 END ASC,
            c.created_at ASC
        LIMIT 1
        "#,
    )
    .bind(deposit_address)
    .fetch_optional(pool)
    .await
    .context("failed to fetch deposit channel by address")?;

    row.map(hydrate_deposit_channel).transpose()
}

pub async fn observe_deposit_transfer(
    pool: &PgPool,
    input: &ObserveDepositTransferInput,
) -> anyhow::Result<DepositTransferRecord> {
    let existing = sqlx::query(
        r#"
        SELECT id
        FROM deposit_transfers
        WHERE chain_key = $1
          AND tx_hash = $2
          AND asset_id = $3
          AND LOWER(deposit_address) = LOWER($4)
        "#,
    )
    .bind(&input.chain_key)
    .bind(&input.tx_hash)
    .bind(&input.asset_id)
    .bind(&input.deposit_address)
    .fetch_optional(pool)
    .await
    .context("failed to read existing deposit transfer")?;

    let status = derive_transfer_status(input.confirmations, input.required_confirmations, "clear");

    let transfer_id = if let Some(existing) = existing {
        let transfer_id: Uuid = existing
            .try_get("id")
            .context("missing existing transfer id")?;
        sqlx::query(
            r#"
            UPDATE deposit_transfers
            SET
                sender_address = COALESCE(sender_address, $2),
                block_number = COALESCE($3, block_number),
                block_hash = COALESCE($4, block_hash),
                amount_raw = $5,
                confirmations = GREATEST(confirmations, $6),
                required_confirmations = GREATEST(required_confirmations, $7),
                status = CASE
                    WHEN risk_state = 'flagged' THEN 'FLAGGED'
                    WHEN GREATEST(confirmations, $6) >= GREATEST(required_confirmations, $7) THEN 'ORIGIN_CONFIRMED'
                    ELSE 'DEPOSIT_DETECTED'
                END,
                credit_target = COALESCE(credit_target, $8),
                confirmed_at = CASE
                    WHEN GREATEST(confirmations, $6) >= GREATEST(required_confirmations, $7)
                        THEN COALESCE(confirmed_at, NOW())
                    ELSE confirmed_at
                END,
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(transfer_id)
        .bind(&input.sender_address)
        .bind(input.block_number)
        .bind(&input.block_hash)
        .bind(&input.amount_raw)
        .bind(input.confirmations)
        .bind(input.required_confirmations)
        .bind(&input.credit_target)
        .execute(pool)
        .await
        .context("failed to update observed transfer")?;
        transfer_id
    } else {
        let transfer_id = Uuid::now_v7();
        let row = sqlx::query(
            r#"
            INSERT INTO deposit_transfers (
                id,
                channel_id,
                asset_id,
                chain_key,
                deposit_address,
                sender_address,
                tx_hash,
                block_number,
                block_hash,
                amount_raw,
                confirmations,
                required_confirmations,
                status,
                credit_target,
                confirmed_at
            )
            VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14,
                CASE WHEN $11 >= $12 THEN NOW() ELSE NULL END
            )
            ON CONFLICT (chain_key, tx_hash, asset_id, deposit_address)
            DO UPDATE SET
                sender_address = COALESCE(deposit_transfers.sender_address, EXCLUDED.sender_address),
                block_number = COALESCE(EXCLUDED.block_number, deposit_transfers.block_number),
                block_hash = COALESCE(EXCLUDED.block_hash, deposit_transfers.block_hash),
                amount_raw = EXCLUDED.amount_raw,
                confirmations = GREATEST(deposit_transfers.confirmations, EXCLUDED.confirmations),
                required_confirmations = GREATEST(deposit_transfers.required_confirmations, EXCLUDED.required_confirmations),
                status = CASE
                    WHEN deposit_transfers.risk_state = 'flagged' THEN 'FLAGGED'
                    WHEN GREATEST(deposit_transfers.confirmations, EXCLUDED.confirmations)
                        >= GREATEST(deposit_transfers.required_confirmations, EXCLUDED.required_confirmations)
                        THEN 'ORIGIN_CONFIRMED'
                    ELSE 'DEPOSIT_DETECTED'
                END,
                credit_target = COALESCE(deposit_transfers.credit_target, EXCLUDED.credit_target),
                confirmed_at = CASE
                    WHEN GREATEST(deposit_transfers.confirmations, EXCLUDED.confirmations)
                        >= GREATEST(deposit_transfers.required_confirmations, EXCLUDED.required_confirmations)
                        THEN COALESCE(deposit_transfers.confirmed_at, NOW())
                    ELSE deposit_transfers.confirmed_at
                END,
                updated_at = NOW()
            RETURNING id
            "#,
        )
        .bind(transfer_id)
        .bind(input.channel_id)
        .bind(&input.asset_id)
        .bind(&input.chain_key)
        .bind(&input.deposit_address)
        .bind(&input.sender_address)
        .bind(&input.tx_hash)
        .bind(input.block_number)
        .bind(&input.block_hash)
        .bind(&input.amount_raw)
        .bind(input.confirmations)
        .bind(input.required_confirmations)
        .bind(status)
        .bind(&input.credit_target)
        .fetch_one(pool)
        .await
        .context("failed to upsert observed transfer")?;
        let transfer_id: Uuid = row.try_get("id").context("missing upserted transfer id")?;

        insert_event_for_pool(
            pool,
            "transfer",
            transfer_id,
            if input.confirmations >= input.required_confirmations {
                "transfer.confirmed"
            } else {
                "transfer.detected"
            },
            json!({
                "chain_key": input.chain_key,
                "asset_id": input.asset_id,
                "deposit_address": input.deposit_address,
                "tx_hash": input.tx_hash,
                "amount_raw": input.amount_raw,
                "confirmations": input.confirmations,
            }),
        )
        .await?;
        transfer_id
    };

    sqlx::query(
        r#"
        UPDATE deposit_channels
        SET last_seen_at = NOW(),
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(input.channel_id)
    .execute(pool)
    .await
    .context("failed to touch deposit channel after transfer detection")?;

    get_deposit_transfer(pool, transfer_id)
        .await?
        .context("observed transfer could not be read back")
}

pub async fn get_deposit_transfer(
    pool: &PgPool,
    transfer_id: Uuid,
) -> anyhow::Result<Option<DepositTransferRecord>> {
    let sql = transfer_select_sql("WHERE t.id = $1");
    let row = sqlx::query(&sql)
        .bind(transfer_id)
        .fetch_optional(pool)
        .await
        .context("failed to get deposit transfer")?;

    row.map(hydrate_transfer).transpose()
}

pub async fn list_transfers_by_deposit_address(
    pool: &PgPool,
    deposit_address: &str,
) -> anyhow::Result<Vec<DepositTransferRecord>> {
    let sql = transfer_select_sql(
        "WHERE LOWER(t.deposit_address) = LOWER($1) ORDER BY t.created_at DESC",
    );
    let rows = sqlx::query(&sql)
        .bind(deposit_address)
        .fetch_all(pool)
        .await
        .context("failed to list deposit transfers by address")?;

    rows.into_iter().map(hydrate_transfer).collect()
}

pub async fn list_transfers_ready_for_route(
    pool: &PgPool,
    limit: i64,
) -> anyhow::Result<Vec<DepositTransferRecord>> {
    let sql = transfer_select_sql(
        r#"
        WHERE t.status = 'ORIGIN_CONFIRMED'
          AND t.risk_state = 'clear'
          AND COALESCE(t.credit_target, p.wallet_address) IS NOT NULL
          AND NOT EXISTS (
            SELECT 1
            FROM deposit_route_jobs j
            WHERE j.transfer_id = t.id
              AND j.status IN ('queued', 'dispatching', 'processing', 'completed')
          )
        ORDER BY t.detected_at ASC
        LIMIT $1
        "#,
    );
    let rows = sqlx::query(&sql)
        .bind(limit)
        .fetch_all(pool)
        .await
        .context("failed to list transfers ready for routing")?;

    rows.into_iter().map(hydrate_transfer).collect()
}

pub async fn queue_route_job(
    pool: &PgPool,
    transfer_id: Uuid,
    job_type: &str,
    payload: Value,
) -> anyhow::Result<DepositRouteJobRecord> {
    sqlx::query(
        r#"
        INSERT INTO deposit_route_jobs (id, transfer_id, job_type, status, payload)
        VALUES ($1, $2, $3, 'queued', $4::jsonb)
        ON CONFLICT (transfer_id, job_type) DO NOTHING
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(transfer_id)
    .bind(job_type)
    .bind(payload.to_string())
    .execute(pool)
    .await
    .context("failed to enqueue deposit route job")?;

    let row = sqlx::query(
        r#"
        SELECT
            id,
            transfer_id,
            job_type,
            status,
            attempts,
            payload,
            response,
            last_error,
            TO_CHAR(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS created_at,
            TO_CHAR(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS updated_at
        FROM deposit_route_jobs
        WHERE transfer_id = $1
          AND job_type = $2
        "#,
    )
    .bind(transfer_id)
    .bind(job_type)
    .fetch_one(pool)
    .await
    .context("failed to fetch queued route job")?;

    hydrate_route_job(row)
}

pub async fn get_route_job(
    pool: &PgPool,
    job_id: Uuid,
) -> anyhow::Result<Option<DepositRouteJobRecord>> {
    let row = sqlx::query(
        r#"
        SELECT
            id,
            transfer_id,
            job_type,
            status,
            attempts,
            payload,
            response,
            last_error,
            TO_CHAR(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS created_at,
            TO_CHAR(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS updated_at
        FROM deposit_route_jobs
        WHERE id = $1
        "#,
    )
    .bind(job_id)
    .fetch_optional(pool)
    .await
    .context("failed to fetch route job")?;

    row.map(hydrate_route_job).transpose()
}

pub async fn list_dispatchable_route_jobs(
    pool: &PgPool,
    limit: i64,
) -> anyhow::Result<Vec<DepositRouteJobRecord>> {
    let rows = sqlx::query(
        r#"
        SELECT
            id,
            transfer_id,
            job_type,
            status,
            attempts,
            payload,
            response,
            last_error,
            TO_CHAR(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS created_at,
            TO_CHAR(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS updated_at
        FROM deposit_route_jobs
        WHERE status IN ('queued', 'retryable')
        ORDER BY created_at ASC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("failed to list dispatchable route jobs")?;

    rows.into_iter().map(hydrate_route_job).collect()
}

pub async fn list_route_jobs(
    pool: &PgPool,
    status: Option<&str>,
    limit: i64,
) -> anyhow::Result<Vec<DepositRouteJobRecord>> {
    let limit = limit.clamp(1, 200);
    let rows = if let Some(status) = status {
        sqlx::query(
            r#"
            SELECT
                id,
                transfer_id,
                job_type,
                status,
                attempts,
                payload,
                response,
                last_error,
                TO_CHAR(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS created_at,
                TO_CHAR(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS updated_at
            FROM deposit_route_jobs
            WHERE status = $1
            ORDER BY created_at DESC
            LIMIT $2
            "#,
        )
        .bind(status)
        .bind(limit)
        .fetch_all(pool)
        .await
        .context("failed to list route jobs by status")?
    } else {
        sqlx::query(
            r#"
            SELECT
                id,
                transfer_id,
                job_type,
                status,
                attempts,
                payload,
                response,
                last_error,
                TO_CHAR(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS created_at,
                TO_CHAR(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS updated_at
            FROM deposit_route_jobs
            ORDER BY created_at DESC
            LIMIT $1
            "#,
        )
        .bind(limit)
        .fetch_all(pool)
        .await
        .context("failed to list route jobs")?
    };

    rows.into_iter().map(hydrate_route_job).collect()
}

pub async fn list_route_jobs_by_deposit_address(
    pool: &PgPool,
    deposit_address: &str,
) -> anyhow::Result<Vec<DepositRouteJobRecord>> {
    let rows = sqlx::query(
        r#"
        SELECT
            j.id,
            j.transfer_id,
            j.job_type,
            j.status,
            j.attempts,
            j.payload,
            j.response,
            j.last_error,
            TO_CHAR(j.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS created_at,
            TO_CHAR(j.updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS updated_at
        FROM deposit_route_jobs j
        INNER JOIN deposit_transfers t ON t.id = j.transfer_id
        WHERE LOWER(t.deposit_address) = LOWER($1)
        ORDER BY j.created_at DESC
        "#,
    )
    .bind(deposit_address)
    .fetch_all(pool)
    .await
    .context("failed to list route jobs by deposit address")?;

    rows.into_iter().map(hydrate_route_job).collect()
}

pub async fn mark_route_job_dispatching(
    pool: &PgPool,
    job_id: Uuid,
) -> anyhow::Result<DepositRouteJobRecord> {
    sqlx::query(
        r#"
        UPDATE deposit_route_jobs
        SET status = 'dispatching',
            attempts = attempts + 1,
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(job_id)
    .execute(pool)
    .await
    .context("failed to mark route job dispatching")?;

    let row = sqlx::query(
        r#"
        SELECT
            id,
            transfer_id,
            job_type,
            status,
            attempts,
            payload,
            response,
            last_error,
            TO_CHAR(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS created_at,
            TO_CHAR(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS updated_at
        FROM deposit_route_jobs
        WHERE id = $1
        "#,
    )
    .bind(job_id)
    .fetch_one(pool)
    .await
    .context("failed to read dispatching route job")?;

    hydrate_route_job(row)
}

pub async fn mark_route_job_processing(
    pool: &PgPool,
    job_id: Uuid,
    response: Value,
) -> anyhow::Result<()> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to open route processing transaction")?;
    let transfer_id: Uuid = sqlx::query_scalar(
        r#"
        UPDATE deposit_route_jobs
        SET status = 'processing',
            response = $2::jsonb,
            last_error = NULL,
            updated_at = NOW()
        WHERE id = $1
        RETURNING transfer_id
        "#,
    )
    .bind(job_id)
    .bind(response.to_string())
    .fetch_one(&mut *tx)
    .await
    .context("failed to mark route job processing")?;

    sqlx::query(
        r#"
        UPDATE deposit_transfers
        SET status = 'PROCESSING',
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(transfer_id)
    .execute(&mut *tx)
    .await
    .context("failed to mark transfer processing")?;

    insert_deposit_event(
        &mut tx,
        "route_job",
        job_id,
        "route_job.processing",
        json!({
            "response": response,
        }),
    )
    .await?;
    tx.commit()
        .await
        .context("failed to commit route processing transaction")?;
    Ok(())
}

pub async fn mark_route_job_failed(
    pool: &PgPool,
    job_id: Uuid,
    last_error: &str,
    retryable: bool,
) -> anyhow::Result<()> {
    let next_status = if retryable { "retryable" } else { "failed" };
    let mut tx = pool
        .begin()
        .await
        .context("failed to open route failure transaction")?;
    let transfer_id: Uuid = sqlx::query_scalar(
        r#"
        UPDATE deposit_route_jobs
        SET status = $2,
            last_error = $3,
            updated_at = NOW()
        WHERE id = $1
        RETURNING transfer_id
        "#,
    )
    .bind(job_id)
    .bind(next_status)
    .bind(last_error)
    .fetch_one(&mut *tx)
    .await
    .context("failed to mark route job failed")?;

    let transfer_status = if retryable {
        "ORIGIN_CONFIRMED"
    } else {
        "RECOVERY_REQUIRED"
    };
    sqlx::query(
        r#"
        UPDATE deposit_transfers
        SET status = $2,
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(transfer_id)
    .bind(transfer_status)
    .execute(&mut *tx)
    .await
    .context("failed to update transfer status after route failure")?;

    insert_deposit_event(
        &mut tx,
        "route_job",
        job_id,
        if retryable {
            "route_job.retryable"
        } else {
            "route_job.failed"
        },
        json!({
            "last_error": last_error,
            "retryable": retryable,
        }),
    )
    .await?;

    tx.commit()
        .await
        .context("failed to commit route failure transaction")?;
    Ok(())
}

pub async fn mark_route_job_completed(
    pool: &PgPool,
    job_id: Uuid,
    response: Value,
    destination_tx_hash: Option<&str>,
) -> anyhow::Result<()> {
    if destination_tx_hash.unwrap_or_default().trim().is_empty() {
        return Err(anyhow!(
            "completed route callback must include confirmed destination_tx_hash"
        ));
    }

    let mut tx = pool
        .begin()
        .await
        .context("failed to open route completion transaction")?;

    let existing = sqlx::query(
        r#"
        SELECT id, transfer_id, status
        FROM deposit_route_jobs
        WHERE id = $1
        FOR UPDATE
        "#,
    )
    .bind(job_id)
    .fetch_optional(&mut *tx)
    .await
    .context("failed to lock route job for completion")?
    .ok_or_else(|| anyhow!("route job not found"))?;
    let transfer_id = existing
        .try_get::<Uuid, _>("transfer_id")
        .context("route job missing transfer_id")?;
    let current_status = existing
        .try_get::<String, _>("status")
        .context("route job missing status")?;
    if current_status == "completed" {
        tx.commit()
            .await
            .context("failed to commit idempotent route completion")?;
        return Ok(());
    }

    sqlx::query(
        r#"
        UPDATE deposit_route_jobs
        SET status = 'completed',
            response = $2::jsonb,
            last_error = NULL,
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(job_id)
    .bind(response.to_string())
    .execute(&mut *tx)
    .await
    .context("failed to mark route job completed")?;

    sqlx::query(
        r#"
        UPDATE deposit_transfers
        SET status = 'COMPLETED',
            destination_tx_hash = COALESCE($2, destination_tx_hash),
            completed_at = NOW(),
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(transfer_id)
    .bind(destination_tx_hash)
    .execute(&mut *tx)
    .await
    .context("failed to mark deposit transfer completed")?;

    let player_id: Uuid = sqlx::query_scalar(
        r#"
        SELECT c.player_id
        FROM deposit_transfers t
        INNER JOIN deposit_channels c ON c.id = t.channel_id
        WHERE t.id = $1
        "#,
    )
    .bind(transfer_id)
    .fetch_one(&mut *tx)
    .await
    .context("failed to resolve deposit transfer owner")?;

    let credited_amount_raw = response
        .get("credited_amount_raw")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("completed route response missing credited_amount_raw"))?
        .parse::<u128>()
        .context("invalid credited_amount_raw on completed route response")?;
    balances::credit_gambling_balance_tx(
        &mut tx,
        player_id,
        credited_amount_raw,
        "deposit_credit",
        Some("deposit_transfer"),
        Some(&transfer_id.to_string()),
        response.clone(),
    )
    .await?;

    insert_deposit_event(
        &mut tx,
        "route_job",
        job_id,
        "route_job.completed",
        json!({
            "destination_tx_hash": destination_tx_hash,
            "response": response,
        }),
    )
    .await?;

    tx.commit()
        .await
        .context("failed to commit route completion transaction")?;
    Ok(())
}

pub async fn retry_route_job(
    pool: &PgPool,
    job_id: Uuid,
) -> anyhow::Result<Option<DepositRouteJobRecord>> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to open route retry transaction")?;
    let transfer_id: Option<Uuid> = sqlx::query_scalar(
        r#"
        UPDATE deposit_route_jobs
        SET status = 'queued',
            last_error = NULL,
            updated_at = NOW()
        WHERE id = $1
          AND status IN ('failed', 'retryable')
        RETURNING transfer_id
        "#,
    )
    .bind(job_id)
    .fetch_optional(&mut *tx)
    .await
    .context("failed to retry route job")?;

    if let Some(transfer_id) = transfer_id {
        sqlx::query(
            r#"
            UPDATE deposit_transfers
            SET status = 'ORIGIN_CONFIRMED',
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(transfer_id)
        .execute(&mut *tx)
        .await
        .context("failed to reset transfer status for route retry")?;

        insert_deposit_event(
            &mut tx,
            "route_job",
            job_id,
            "route_job.requeued",
            json!({}),
        )
        .await?;
    }

    tx.commit()
        .await
        .context("failed to commit route retry transaction")?;
    get_route_job(pool, job_id).await
}

pub async fn flag_deposit_transfer(
    pool: &PgPool,
    transfer_id: Uuid,
    code: &str,
    severity: &str,
    description: &str,
) -> anyhow::Result<DepositRiskFlagRecord> {
    let flag_id = Uuid::now_v7();
    let mut tx = pool
        .begin()
        .await
        .context("failed to open risk flag transaction")?;
    sqlx::query(
        r#"
        UPDATE deposit_transfers
        SET risk_state = 'flagged',
            status = 'FLAGGED',
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(transfer_id)
    .execute(&mut *tx)
    .await
    .context("failed to flag transfer")?;

    sqlx::query(
        r#"
        INSERT INTO deposit_risk_flags (id, transfer_id, code, severity, description)
        VALUES ($1, $2, $3, $4, $5)
        "#,
    )
    .bind(flag_id)
    .bind(transfer_id)
    .bind(code)
    .bind(severity)
    .bind(description)
    .execute(&mut *tx)
    .await
    .context("failed to insert deposit risk flag")?;

    insert_deposit_event(
        &mut tx,
        "transfer",
        transfer_id,
        "transfer.flagged",
        json!({
            "code": code,
            "severity": severity,
            "description": description,
        }),
    )
    .await?;

    tx.commit()
        .await
        .context("failed to commit risk flag transaction")?;
    get_risk_flag(pool, flag_id)
        .await?
        .context("risk flag could not be read back")
}

pub async fn get_risk_flag(
    pool: &PgPool,
    flag_id: Uuid,
) -> anyhow::Result<Option<DepositRiskFlagRecord>> {
    let row = sqlx::query(
        r#"
        SELECT
            id,
            transfer_id,
            code,
            severity,
            description,
            resolution_status,
            resolution_notes,
            TO_CHAR(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS created_at,
            TO_CHAR(resolved_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS resolved_at
        FROM deposit_risk_flags
        WHERE id = $1
        "#,
    )
    .bind(flag_id)
    .fetch_optional(pool)
    .await
    .context("failed to read deposit risk flag")?;

    row.map(hydrate_risk_flag).transpose()
}

pub async fn list_open_risk_flags(pool: &PgPool) -> anyhow::Result<Vec<DepositRiskFlagRecord>> {
    let rows = sqlx::query(
        r#"
        SELECT
            id,
            transfer_id,
            code,
            severity,
            description,
            resolution_status,
            resolution_notes,
            TO_CHAR(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS created_at,
            TO_CHAR(resolved_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS resolved_at
        FROM deposit_risk_flags
        WHERE resolution_status = 'open'
        ORDER BY created_at DESC
        "#,
    )
    .fetch_all(pool)
    .await
    .context("failed to list open risk flags")?;

    rows.into_iter().map(hydrate_risk_flag).collect()
}

pub async fn list_risk_flags_by_deposit_address(
    pool: &PgPool,
    deposit_address: &str,
) -> anyhow::Result<Vec<DepositRiskFlagRecord>> {
    let rows = sqlx::query(
        r#"
        SELECT
            f.id,
            f.transfer_id,
            f.code,
            f.severity,
            f.description,
            f.resolution_status,
            f.resolution_notes,
            TO_CHAR(f.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS created_at,
            TO_CHAR(f.resolved_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS resolved_at
        FROM deposit_risk_flags f
        INNER JOIN deposit_transfers t ON t.id = f.transfer_id
        WHERE LOWER(t.deposit_address) = LOWER($1)
        ORDER BY f.created_at DESC
        "#,
    )
    .bind(deposit_address)
    .fetch_all(pool)
    .await
    .context("failed to list risk flags by deposit address")?;

    rows.into_iter().map(hydrate_risk_flag).collect()
}

pub async fn resolve_risk_flag(
    pool: &PgPool,
    flag_id: Uuid,
    input: &ResolveRiskFlagInput,
) -> anyhow::Result<Option<DepositRiskFlagRecord>> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to open resolve risk flag transaction")?;
    let transfer_id: Option<Uuid> = sqlx::query_scalar(
        r#"
        UPDATE deposit_risk_flags
        SET resolution_status = $2,
            resolution_notes = $3,
            resolved_at = NOW()
        WHERE id = $1
        RETURNING transfer_id
        "#,
    )
    .bind(flag_id)
    .bind(&input.resolution_status)
    .bind(&input.resolution_notes)
    .fetch_optional(&mut *tx)
    .await
    .context("failed to resolve risk flag")?;

    if let Some(transfer_id) = transfer_id {
        match input.resolution_status.as_str() {
            "approved" => {
                let current: Option<(i32, i32)> = sqlx::query_as(
                    "SELECT confirmations, required_confirmations FROM deposit_transfers WHERE id = $1",
                )
                .bind(transfer_id)
                .fetch_optional(&mut *tx)
                .await
                .context("failed to read transfer confirmations when approving risk flag")?;
                let (confirmations, required_confirmations) = current.unwrap_or((0, 0));
                let status = derive_transfer_status(confirmations, required_confirmations, "clear");
                sqlx::query(
                    r#"
                    UPDATE deposit_transfers
                    SET risk_state = 'clear',
                        status = $2,
                        updated_at = NOW()
                    WHERE id = $1
                    "#,
                )
                .bind(transfer_id)
                .bind(status)
                .execute(&mut *tx)
                .await
                .context("failed to clear transfer risk flag")?;
            }
            "rejected" => {
                sqlx::query(
                    r#"
                    UPDATE deposit_transfers
                    SET risk_state = 'blocked',
                        status = 'RECOVERY_REQUIRED',
                        updated_at = NOW()
                    WHERE id = $1
                    "#,
                )
                .bind(transfer_id)
                .execute(&mut *tx)
                .await
                .context("failed to block transfer after risk rejection")?;
            }
            _ => {}
        }
    }

    tx.commit()
        .await
        .context("failed to commit risk flag resolution")?;
    get_risk_flag(pool, flag_id).await
}

pub async fn create_recovery_request(
    pool: &PgPool,
    input: &CreateDepositRecoveryInput,
) -> anyhow::Result<DepositRecoveryRecord> {
    let recovery_id = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO deposit_recoveries (id, transfer_id, reason, notes, requested_by, status)
        VALUES ($1, $2, $3, $4, $5, 'open')
        "#,
    )
    .bind(recovery_id)
    .bind(input.transfer_id)
    .bind(&input.reason)
    .bind(&input.notes)
    .bind(&input.requested_by)
    .execute(pool)
    .await
    .context("failed to create deposit recovery request")?;

    sqlx::query(
        r#"
        UPDATE deposit_transfers
        SET status = 'RECOVERY_REQUIRED',
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(input.transfer_id)
    .execute(pool)
    .await
    .context("failed to mark transfer recovery required")?;

    get_recovery_request(pool, recovery_id)
        .await?
        .context("deposit recovery request could not be read back")
}

pub async fn get_recovery_request(
    pool: &PgPool,
    recovery_id: Uuid,
) -> anyhow::Result<Option<DepositRecoveryRecord>> {
    let row = sqlx::query(
        r#"
        SELECT
            id,
            transfer_id,
            reason,
            notes,
            requested_by,
            status,
            resolution_notes,
            TO_CHAR(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS created_at,
            TO_CHAR(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS updated_at,
            TO_CHAR(resolved_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS resolved_at
        FROM deposit_recoveries
        WHERE id = $1
        "#,
    )
    .bind(recovery_id)
    .fetch_optional(pool)
    .await
    .context("failed to fetch recovery request")?;

    row.map(hydrate_recovery).transpose()
}

pub async fn list_recoveries_by_deposit_address(
    pool: &PgPool,
    deposit_address: &str,
) -> anyhow::Result<Vec<DepositRecoveryRecord>> {
    let rows = sqlx::query(
        r#"
        SELECT
            r.id,
            r.transfer_id,
            r.reason,
            r.notes,
            r.requested_by,
            r.status,
            r.resolution_notes,
            TO_CHAR(r.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS created_at,
            TO_CHAR(r.updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS updated_at,
            TO_CHAR(r.resolved_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS resolved_at
        FROM deposit_recoveries r
        INNER JOIN deposit_transfers t ON t.id = r.transfer_id
        WHERE LOWER(t.deposit_address) = LOWER($1)
        ORDER BY r.created_at DESC
        "#,
    )
    .bind(deposit_address)
    .fetch_all(pool)
    .await
    .context("failed to list recoveries by deposit address")?;

    rows.into_iter().map(hydrate_recovery).collect()
}

pub async fn list_recoveries(
    pool: &PgPool,
    status: Option<&str>,
    limit: i64,
) -> anyhow::Result<Vec<DepositRecoveryRecord>> {
    let limit = limit.clamp(1, 200);
    let rows = if let Some(status) = status {
        sqlx::query(
            r#"
            SELECT
                id,
                transfer_id,
                reason,
                notes,
                requested_by,
                status,
                resolution_notes,
                TO_CHAR(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS created_at,
                TO_CHAR(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS updated_at,
                TO_CHAR(resolved_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS resolved_at
            FROM deposit_recoveries
            WHERE status = $1
            ORDER BY created_at DESC
            LIMIT $2
            "#,
        )
        .bind(status)
        .bind(limit)
        .fetch_all(pool)
        .await
        .context("failed to list recoveries by status")?
    } else {
        sqlx::query(
            r#"
            SELECT
                id,
                transfer_id,
                reason,
                notes,
                requested_by,
                status,
                resolution_notes,
                TO_CHAR(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS created_at,
                TO_CHAR(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS updated_at,
                TO_CHAR(resolved_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS resolved_at
            FROM deposit_recoveries
            ORDER BY created_at DESC
            LIMIT $1
            "#,
        )
        .bind(limit)
        .fetch_all(pool)
        .await
        .context("failed to list recoveries")?
    };

    rows.into_iter().map(hydrate_recovery).collect()
}

pub async fn resolve_recovery_request(
    pool: &PgPool,
    recovery_id: Uuid,
    input: &ResolveDepositRecoveryInput,
) -> anyhow::Result<Option<DepositRecoveryRecord>> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to open recovery resolution transaction")?;
    let transfer_id: Option<Uuid> = sqlx::query_scalar(
        r#"
        UPDATE deposit_recoveries
        SET status = $2,
            resolution_notes = $3,
            resolved_at = NOW(),
            updated_at = NOW()
        WHERE id = $1
        RETURNING transfer_id
        "#,
    )
    .bind(recovery_id)
    .bind(&input.status)
    .bind(&input.resolution_notes)
    .fetch_optional(&mut *tx)
    .await
    .context("failed to resolve recovery request")?;

    if let Some(transfer_id) = transfer_id {
        let next_transfer_status = match input.status.as_str() {
            "resolved" => "COMPLETED",
            "cancelled" => "FAILED",
            _ => "RECOVERY_REQUIRED",
        };
        sqlx::query(
            r#"
            UPDATE deposit_transfers
            SET status = $2,
                completed_at = CASE WHEN $2 = 'COMPLETED' THEN NOW() ELSE completed_at END,
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(transfer_id)
        .bind(next_transfer_status)
        .execute(&mut *tx)
        .await
        .context("failed to resolve transfer recovery status")?;
    }

    tx.commit()
        .await
        .context("failed to commit recovery resolution")?;
    get_recovery_request(pool, recovery_id).await
}

fn transfer_select_sql(suffix: &str) -> String {
    format!(
        r#"
        SELECT
            t.id,
            t.channel_id,
            p.id AS player_id,
            p.wallet_address,
            pp.username,
            t.asset_id,
            t.chain_key,
            a.asset_symbol,
            t.deposit_address,
            t.sender_address,
            t.tx_hash,
            t.block_number,
            t.amount_raw,
            t.confirmations,
            t.required_confirmations,
            t.status,
            t.risk_state,
            t.credit_target,
            t.destination_tx_hash,
            TO_CHAR(t.detected_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS detected_at,
            TO_CHAR(t.confirmed_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS confirmed_at,
            TO_CHAR(t.completed_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS completed_at,
            TO_CHAR(t.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS created_at,
            TO_CHAR(t.updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS updated_at,
            a.asset_decimals
        FROM deposit_transfers t
        INNER JOIN deposit_channels c ON c.id = t.channel_id
        INNER JOIN players p ON p.id = c.player_id
        INNER JOIN deposit_supported_assets a ON a.id = t.asset_id AND a.chain_key = t.chain_key
        LEFT JOIN player_profiles pp ON pp.player_id = p.id
        {suffix}
        "#
    )
}

async fn insert_event_for_pool(
    pool: &PgPool,
    entity_type: &str,
    entity_id: Uuid,
    event_type: &str,
    payload: Value,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO deposit_events (id, entity_type, entity_id, event_type, payload)
        VALUES ($1, $2, $3, $4, $5::jsonb)
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(entity_type)
    .bind(entity_id)
    .bind(event_type)
    .bind(payload.to_string())
    .execute(pool)
    .await
    .context("failed to insert deposit event")?;
    Ok(())
}

async fn insert_deposit_event(
    tx: &mut Transaction<'_, Postgres>,
    entity_type: &str,
    entity_id: Uuid,
    event_type: &str,
    payload: Value,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO deposit_events (id, entity_type, entity_id, event_type, payload)
        VALUES ($1, $2, $3, $4, $5::jsonb)
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(entity_type)
    .bind(entity_id)
    .bind(event_type)
    .bind(payload.to_string())
    .execute(&mut **tx)
    .await
    .context("failed to insert deposit event")?;
    Ok(())
}

fn derive_transfer_status(
    confirmations: i32,
    required_confirmations: i32,
    risk_state: &str,
) -> &'static str {
    if risk_state == "flagged" || risk_state == "blocked" {
        "FLAGGED"
    } else if confirmations >= required_confirmations {
        "ORIGIN_CONFIRMED"
    } else {
        "DEPOSIT_DETECTED"
    }
}

fn format_amount(raw: &str, decimals: i32) -> String {
    let decimals = decimals.max(0) as usize;
    let digits = raw.trim_start_matches('+');
    let negative = digits.starts_with('-');
    let digits = if negative { &digits[1..] } else { digits };
    let digits = if digits.is_empty() { "0" } else { digits };

    if decimals == 0 {
        return if negative {
            format!("-{digits}")
        } else {
            digits.to_string()
        };
    }

    let mut padded = digits.to_string();
    if padded.len() <= decimals {
        padded = format!("{}{}", "0".repeat(decimals + 1 - padded.len()), padded);
    }
    let split = padded.len() - decimals;
    let whole = &padded[..split];
    let fractional = padded[split..].trim_end_matches('0');
    let rendered = if fractional.is_empty() {
        whole.to_string()
    } else {
        format!("{whole}.{fractional}")
    };
    if negative {
        format!("-{rendered}")
    } else {
        rendered
    }
}

fn hydrate_supported_asset(
    row: sqlx::postgres::PgRow,
) -> anyhow::Result<DepositSupportedAssetRecord> {
    Ok(DepositSupportedAssetRecord {
        id: row.try_get("id").context("missing asset id")?,
        chain_key: row.try_get("chain_key").context("missing chain_key")?,
        chain_family: row
            .try_get("chain_family")
            .context("missing chain_family")?,
        network: row.try_get("network").context("missing network")?,
        chain_id: row.try_get("chain_id").context("missing chain_id")?,
        asset_symbol: row
            .try_get("asset_symbol")
            .context("missing asset_symbol")?,
        asset_address: row
            .try_get("asset_address")
            .context("missing asset_address")?,
        asset_decimals: row
            .try_get("asset_decimals")
            .context("missing asset_decimals")?,
        route_kind: row.try_get("route_kind").context("missing route_kind")?,
        watch_mode: row.try_get("watch_mode").context("missing watch_mode")?,
        min_amount: row.try_get("min_amount").context("missing min_amount")?,
        max_amount: row.try_get("max_amount").context("missing max_amount")?,
        confirmations_required: row
            .try_get("confirmations_required")
            .context("missing confirmations_required")?,
        status: row.try_get("status").context("missing status")?,
        metadata: row
            .try_get::<Value, _>("metadata")
            .context("missing metadata")?,
        created_at: row.try_get("created_at").context("missing created_at")?,
        updated_at: row.try_get("updated_at").context("missing updated_at")?,
    })
}

fn hydrate_deposit_channel(row: sqlx::postgres::PgRow) -> anyhow::Result<DepositChannelRecord> {
    Ok(DepositChannelRecord {
        channel_id: row
            .try_get::<Uuid, _>("id")
            .context("missing channel id")?
            .to_string(),
        user_id: row
            .try_get::<Uuid, _>("player_id")
            .context("missing player_id")?
            .to_string(),
        wallet_address: row
            .try_get::<Option<String>, _>("wallet_address")
            .context("missing wallet_address")?,
        username: row.try_get("username").ok(),
        asset_id: row.try_get("asset_id").context("missing asset_id")?,
        chain_key: row.try_get("chain_key").context("missing chain_key")?,
        asset_symbol: row
            .try_get("asset_symbol")
            .context("missing asset_symbol")?,
        deposit_address: row
            .try_get("deposit_address")
            .context("missing deposit_address")?,
        qr_payload: row.try_get("qr_payload").context("missing qr_payload")?,
        route_kind: row.try_get("route_kind").context("missing route_kind")?,
        status: row.try_get("status").context("missing status")?,
        watch_from_block: row.try_get("watch_from_block").ok(),
        last_scanned_block: row.try_get("last_scanned_block").ok(),
        last_seen_at: row.try_get("last_seen_at").ok(),
        created_at: row.try_get("created_at").context("missing created_at")?,
        updated_at: row.try_get("updated_at").context("missing updated_at")?,
    })
}

fn hydrate_transfer(row: sqlx::postgres::PgRow) -> anyhow::Result<DepositTransferRecord> {
    let decimals = row
        .try_get::<i32, _>("asset_decimals")
        .context("missing asset_decimals")?;
    let amount_raw = row
        .try_get::<String, _>("amount_raw")
        .context("missing amount_raw")?;
    Ok(DepositTransferRecord {
        transfer_id: row
            .try_get::<Uuid, _>("id")
            .context("missing transfer id")?
            .to_string(),
        channel_id: row
            .try_get::<Uuid, _>("channel_id")
            .context("missing channel_id")?
            .to_string(),
        user_id: row
            .try_get::<Uuid, _>("player_id")
            .context("missing transfer player_id")?
            .to_string(),
        wallet_address: row
            .try_get::<Option<String>, _>("wallet_address")
            .context("missing wallet_address")?,
        username: row.try_get("username").ok(),
        asset_id: row.try_get("asset_id").context("missing asset_id")?,
        chain_key: row.try_get("chain_key").context("missing chain_key")?,
        asset_symbol: row
            .try_get("asset_symbol")
            .context("missing asset_symbol")?,
        deposit_address: row
            .try_get("deposit_address")
            .context("missing deposit_address")?,
        sender_address: row.try_get("sender_address").ok(),
        tx_hash: row.try_get("tx_hash").context("missing tx_hash")?,
        block_number: row.try_get("block_number").ok(),
        amount_raw: amount_raw.clone(),
        amount_display: format_amount(&amount_raw, decimals),
        confirmations: row
            .try_get("confirmations")
            .context("missing confirmations")?,
        required_confirmations: row
            .try_get("required_confirmations")
            .context("missing required_confirmations")?,
        status: row.try_get("status").context("missing status")?,
        risk_state: row.try_get("risk_state").context("missing risk_state")?,
        credit_target: row.try_get("credit_target").ok(),
        destination_tx_hash: row.try_get("destination_tx_hash").ok(),
        detected_at: row.try_get("detected_at").context("missing detected_at")?,
        confirmed_at: row.try_get("confirmed_at").ok(),
        completed_at: row.try_get("completed_at").ok(),
        created_at: row.try_get("created_at").context("missing created_at")?,
        updated_at: row.try_get("updated_at").context("missing updated_at")?,
    })
}

fn hydrate_route_job(row: sqlx::postgres::PgRow) -> anyhow::Result<DepositRouteJobRecord> {
    Ok(DepositRouteJobRecord {
        job_id: row
            .try_get::<Uuid, _>("id")
            .context("missing route job id")?
            .to_string(),
        transfer_id: row
            .try_get::<Uuid, _>("transfer_id")
            .context("missing transfer_id")?
            .to_string(),
        job_type: row.try_get("job_type").context("missing job_type")?,
        status: row.try_get("status").context("missing status")?,
        attempts: row.try_get("attempts").context("missing attempts")?,
        payload: row
            .try_get::<Value, _>("payload")
            .context("missing payload")?,
        response: row.try_get("response").ok(),
        last_error: row.try_get("last_error").ok(),
        created_at: row.try_get("created_at").context("missing created_at")?,
        updated_at: row.try_get("updated_at").context("missing updated_at")?,
    })
}

fn hydrate_risk_flag(row: sqlx::postgres::PgRow) -> anyhow::Result<DepositRiskFlagRecord> {
    Ok(DepositRiskFlagRecord {
        flag_id: row
            .try_get::<Uuid, _>("id")
            .context("missing risk flag id")?
            .to_string(),
        transfer_id: row
            .try_get::<Uuid, _>("transfer_id")
            .context("missing transfer_id")?
            .to_string(),
        code: row.try_get("code").context("missing code")?,
        severity: row.try_get("severity").context("missing severity")?,
        description: row.try_get("description").context("missing description")?,
        resolution_status: row
            .try_get("resolution_status")
            .context("missing resolution_status")?,
        resolution_notes: row.try_get("resolution_notes").ok(),
        created_at: row.try_get("created_at").context("missing created_at")?,
        resolved_at: row.try_get("resolved_at").ok(),
    })
}

fn hydrate_recovery(row: sqlx::postgres::PgRow) -> anyhow::Result<DepositRecoveryRecord> {
    Ok(DepositRecoveryRecord {
        recovery_id: row
            .try_get::<Uuid, _>("id")
            .context("missing recovery id")?
            .to_string(),
        transfer_id: row
            .try_get::<Uuid, _>("transfer_id")
            .context("missing transfer id")?
            .to_string(),
        reason: row.try_get("reason").context("missing reason")?,
        notes: row.try_get("notes").ok(),
        requested_by: row.try_get("requested_by").ok(),
        status: row.try_get("status").context("missing status")?,
        resolution_notes: row.try_get("resolution_notes").ok(),
        created_at: row.try_get("created_at").context("missing created_at")?,
        updated_at: row.try_get("updated_at").context("missing updated_at")?,
        resolved_at: row.try_get("resolved_at").ok(),
    })
}

#[cfg(test)]
mod tests {
    use super::{derive_transfer_status, format_amount};

    #[test]
    fn formats_raw_amounts_with_decimals() {
        assert_eq!(format_amount("0", 18), "0");
        assert_eq!(format_amount("1", 18), "0.000000000000000001");
        assert_eq!(format_amount("123450000", 6), "123.45");
        assert_eq!(format_amount("1000000", 6), "1");
    }

    #[test]
    fn derives_transfer_status_from_confirmations_and_risk() {
        assert_eq!(derive_transfer_status(1, 12, "clear"), "DEPOSIT_DETECTED");
        assert_eq!(derive_transfer_status(12, 12, "clear"), "ORIGIN_CONFIRMED");
        assert_eq!(derive_transfer_status(12, 12, "flagged"), "FLAGGED");
    }
}
