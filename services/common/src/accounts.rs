use anyhow::{Context, anyhow};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerAccountRecord {
    pub user_id: String,
    pub wallet_address: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerWalletLinkRecord {
    pub user_id: String,
    pub wallet_address: String,
    pub linked_via: String,
    pub wallet_kind: String,
    pub is_primary: bool,
    pub created_at: String,
    pub last_seen_at: String,
}

#[derive(Debug, Clone, Default)]
pub struct EnsurePlayerAccountInput {
    pub wallet_address: String,
    pub auth_provider: Option<String>,
    pub auth_subject: Option<String>,
    pub linked_via: Option<String>,
    pub make_primary: bool,
}

pub fn normalize_wallet_address(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

pub async fn get_player_account(
    pool: &PgPool,
    player_id: Uuid,
) -> anyhow::Result<Option<PlayerAccountRecord>> {
    let row = sqlx::query(
        r#"
        SELECT
            id,
            wallet_address,
            TO_CHAR(first_seen_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS created_at,
            TO_CHAR(last_seen_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS updated_at
        FROM players
        WHERE id = $1
        "#,
    )
    .bind(player_id)
    .fetch_optional(pool)
    .await
    .context("failed to query player account")?;

    row.map(hydrate_player_account).transpose()
}

pub async fn resolve_player_id_by_wallet(
    pool: &PgPool,
    wallet_address: &str,
) -> anyhow::Result<Option<Uuid>> {
    let normalized = normalize_wallet_address(wallet_address);
    sqlx::query_scalar(
        r#"
        SELECT player_id
        FROM player_wallets
        WHERE wallet_address = $1
        "#,
    )
    .bind(&normalized)
    .fetch_optional(pool)
    .await
    .context("failed to resolve player by wallet address")
}

pub async fn resolve_player_account_by_wallet(
    pool: &PgPool,
    wallet_address: &str,
) -> anyhow::Result<Option<PlayerAccountRecord>> {
    let Some(player_id) = resolve_player_id_by_wallet(pool, wallet_address).await? else {
        return Ok(None);
    };
    get_player_account(pool, player_id).await
}

pub async fn list_primary_execution_wallets(
    pool: &PgPool,
    limit: i64,
) -> anyhow::Result<Vec<PlayerWalletLinkRecord>> {
    let rows = sqlx::query(
        r#"
        SELECT
            player_id,
            wallet_address,
            linked_via,
            wallet_kind,
            is_primary,
            TO_CHAR(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS created_at,
            TO_CHAR(last_seen_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS last_seen_at
        FROM player_wallets
        WHERE wallet_kind = 'execution'
          AND is_primary = TRUE
        ORDER BY last_seen_at DESC
        LIMIT $1
        "#,
    )
    .bind(limit.max(1))
    .fetch_all(pool)
    .await
    .context("failed to list primary execution wallets")?;

    rows.into_iter()
        .map(|row| {
            Ok(PlayerWalletLinkRecord {
                user_id: row
                    .try_get::<Uuid, _>("player_id")
                    .context("missing player id")?
                    .to_string(),
                wallet_address: row.try_get("wallet_address").context("missing wallet")?,
                linked_via: row.try_get("linked_via").context("missing linked_via")?,
                wallet_kind: row.try_get("wallet_kind").context("missing wallet_kind")?,
                is_primary: row.try_get("is_primary").context("missing is_primary")?,
                created_at: row.try_get("created_at").context("missing created_at")?,
                last_seen_at: row
                    .try_get("last_seen_at")
                    .context("missing last_seen_at")?,
            })
        })
        .collect()
}

pub async fn ensure_player_account(
    pool: &PgPool,
    input: EnsurePlayerAccountInput,
) -> anyhow::Result<PlayerAccountRecord> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to open player account transaction")?;
    let player_id = ensure_player_account_tx(&mut tx, &input).await?;
    tx.commit()
        .await
        .context("failed to commit player account transaction")?;
    get_player_account(pool, player_id)
        .await?
        .ok_or_else(|| anyhow!("player account missing after ensure"))
}

pub async fn ensure_player_account_tx(
    tx: &mut Transaction<'_, Postgres>,
    input: &EnsurePlayerAccountInput,
) -> anyhow::Result<Uuid> {
    let normalized_wallet_input = normalize_wallet_address(&input.wallet_address);
    let normalized_auth = match (&input.auth_provider, &input.auth_subject) {
        (Some(provider), Some(subject))
            if !provider.trim().is_empty() && !subject.trim().is_empty() =>
        {
            Some((
                provider.trim().to_ascii_lowercase(),
                subject.trim().to_ascii_lowercase(),
            ))
        }
        _ => None,
    };

    let normalized_wallet = if !normalized_wallet_input.is_empty() {
        Some(normalized_wallet_input)
    } else {
        None
    };

    if normalized_wallet.is_none() && normalized_auth.is_none() {
        return Err(anyhow!("wallet address or auth identity is required"));
    }

    let by_auth = if let Some((provider, subject)) = &normalized_auth {
        find_player_id_by_auth_tx(tx, provider, subject).await?
    } else {
        None
    };
    let by_wallet = if let Some(wallet_address) = normalized_wallet.as_deref() {
        find_player_id_by_wallet_tx(tx, wallet_address).await?
    } else {
        None
    };

    let player_id = match (by_auth, by_wallet) {
        (Some(auth_id), Some(wallet_id)) if auth_id != wallet_id => {
            return Err(anyhow!(
                "wallet address is already linked to a different Moros user"
            ));
        }
        (Some(existing), _) | (_, Some(existing)) => {
            touch_player_row(tx, existing).await?;
            existing
        }
        (None, None) => {
            let new_id = Uuid::now_v7();
            sqlx::query("INSERT INTO players (id, wallet_address) VALUES ($1, $2)")
                .bind(new_id)
                .bind(normalized_wallet.as_deref())
                .execute(&mut **tx)
                .await
                .context("failed to insert canonical player row")?;
            new_id
        }
    };

    if let Some(wallet_address) = normalized_wallet.as_deref() {
        let should_make_primary = input.make_primary || by_wallet.is_none();

        upsert_wallet_link_tx(
            tx,
            player_id,
            wallet_address,
            input
                .linked_via
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("wallet"),
            should_make_primary,
        )
        .await?;

        if should_make_primary
            || player_primary_wallet_matches(tx, player_id, wallet_address).await?
        {
            sqlx::query(
                r#"
                UPDATE players
                SET wallet_address = $2,
                    last_seen_at = NOW()
                WHERE id = $1
                "#,
            )
            .bind(player_id)
            .bind(wallet_address)
            .execute(&mut **tx)
            .await
            .context("failed to sync primary wallet on player row")?;
            sqlx::query(
                r#"
                UPDATE player_profiles
                SET wallet_address = $2,
                    updated_at = NOW()
                WHERE player_id = $1
                "#,
            )
            .bind(player_id)
            .bind(wallet_address)
            .execute(&mut **tx)
            .await
            .context("failed to sync profile wallet address")?;
        }
    }

    if let Some((provider, subject)) = normalized_auth {
        sqlx::query(
            r#"
            INSERT INTO player_auth_identities (
                auth_provider,
                auth_subject,
                player_id,
                metadata
            )
            VALUES ($1, $2, $3, '{}'::jsonb)
            ON CONFLICT (auth_provider, auth_subject) DO UPDATE
            SET player_id = EXCLUDED.player_id,
                updated_at = NOW()
            "#,
        )
        .bind(provider)
        .bind(subject)
        .bind(player_id)
        .execute(&mut **tx)
        .await
        .context("failed to link auth identity to canonical player")?;
    }

    Ok(player_id)
}

async fn touch_player_row(
    tx: &mut Transaction<'_, Postgres>,
    player_id: Uuid,
) -> anyhow::Result<()> {
    sqlx::query("UPDATE players SET last_seen_at = NOW() WHERE id = $1")
        .bind(player_id)
        .execute(&mut **tx)
        .await
        .context("failed to touch canonical player row")?;
    Ok(())
}

async fn find_player_id_by_wallet_tx(
    tx: &mut Transaction<'_, Postgres>,
    wallet_address: &str,
) -> anyhow::Result<Option<Uuid>> {
    let linked = sqlx::query_scalar(
        r#"
        SELECT player_id
        FROM player_wallets
        WHERE wallet_address = $1
        "#,
    )
    .bind(wallet_address)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to read linked wallet account")?;
    if linked.is_some() {
        return Ok(linked);
    }

    let legacy = sqlx::query_scalar(
        r#"
        SELECT id
        FROM players
        WHERE LOWER(wallet_address) = LOWER($1)
        "#,
    )
    .bind(wallet_address)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to read legacy player wallet account")?;
    if let Some(player_id) = legacy {
        upsert_wallet_link_tx(tx, player_id, wallet_address, "legacy", true).await?;
        return Ok(Some(player_id));
    }

    Ok(None)
}

async fn find_player_id_by_auth_tx(
    tx: &mut Transaction<'_, Postgres>,
    auth_provider: &str,
    auth_subject: &str,
) -> anyhow::Result<Option<Uuid>> {
    sqlx::query_scalar(
        r#"
        SELECT player_id
        FROM player_auth_identities
        WHERE auth_provider = $1
          AND auth_subject = $2
        "#,
    )
    .bind(auth_provider)
    .bind(auth_subject)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to resolve player by auth identity")
}

async fn player_primary_wallet_matches(
    tx: &mut Transaction<'_, Postgres>,
    player_id: Uuid,
    wallet_address: &str,
) -> anyhow::Result<bool> {
    let current: Option<String> =
        sqlx::query_scalar("SELECT wallet_address FROM players WHERE id = $1")
            .bind(player_id)
            .fetch_optional(&mut **tx)
            .await
            .context("failed to read current primary player wallet")?;
    Ok(current
        .map(|value| normalize_wallet_address(&value) == wallet_address)
        .unwrap_or(false))
}

async fn upsert_wallet_link_tx(
    tx: &mut Transaction<'_, Postgres>,
    player_id: Uuid,
    wallet_address: &str,
    linked_via: &str,
    mark_primary: bool,
) -> anyhow::Result<()> {
    if mark_primary {
        sqlx::query(
            r#"
            UPDATE player_wallets
            SET is_primary = FALSE
            WHERE player_id = $1
            "#,
        )
        .bind(player_id)
        .execute(&mut **tx)
        .await
        .context("failed to clear existing primary wallet link")?;
    }

    sqlx::query(
        r#"
        INSERT INTO player_wallets (
            wallet_address,
            player_id,
            linked_via,
            wallet_kind,
            is_primary,
            created_at,
            last_seen_at
        )
        VALUES ($1, $2, $3, 'execution', $4, NOW(), NOW())
        ON CONFLICT (wallet_address) DO UPDATE
        SET player_id = EXCLUDED.player_id,
            linked_via = EXCLUDED.linked_via,
            is_primary = CASE
                WHEN EXCLUDED.is_primary THEN TRUE
                ELSE player_wallets.is_primary
            END,
            last_seen_at = NOW()
        "#,
    )
    .bind(wallet_address)
    .bind(player_id)
    .bind(linked_via)
    .bind(mark_primary)
    .execute(&mut **tx)
    .await
    .context("failed to upsert player wallet link")?;
    Ok(())
}

fn hydrate_player_account(row: sqlx::postgres::PgRow) -> anyhow::Result<PlayerAccountRecord> {
    Ok(PlayerAccountRecord {
        user_id: row
            .try_get::<Uuid, _>("id")
            .context("missing user id")?
            .to_string(),
        wallet_address: row
            .try_get::<Option<String>, _>("wallet_address")
            .context("missing primary wallet address")?,
        created_at: row
            .try_get::<String, _>("created_at")
            .context("missing player created_at")?,
        updated_at: row
            .try_get::<String, _>("updated_at")
            .context("missing player updated_at")?,
    })
}
