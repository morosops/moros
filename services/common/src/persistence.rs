use crate::{
    accounts::{self, EnsurePlayerAccountInput},
    balances,
    blackjack::{self, BlackjackHandSnapshot, BlackjackHandView},
};
use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateHandInput {
    pub hand_id: Option<String>,
    pub session_id: Option<String>,
    pub table_id: u64,
    pub player: String,
    pub wager: String,
    pub transcript_root: String,
    pub relay_token: String,
    pub expires_at_unix: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlackjackHandRecord {
    pub hand_id: String,
    pub session_id: Option<String>,
    pub player: String,
    pub relay_token: Option<String>,
    pub table_id: u64,
    pub wager: String,
    pub status: String,
    pub phase: String,
    pub transcript_root: String,
    pub active_seat: u8,
    pub seat_count: u8,
    pub dealer_upcard: Option<u8>,
    pub chain_hand_id: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiceServerCommitmentRecord {
    pub commitment_id: i64,
    pub server_seed: String,
    pub server_seed_hash: String,
    pub reveal_deadline_block: i64,
    pub status: String,
    pub round_id: Option<i64>,
    pub commit_tx_hash: Option<String>,
    pub settle_tx_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettledBetSeed {
    pub game: String,
    pub game_id: i64,
    pub settled_at: String,
    pub tx_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerProfileRecord {
    pub user_id: String,
    pub wallet_address: Option<String>,
    pub username: Option<String>,
    pub auth_provider: String,
    pub auth_subject: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub async fn list_recent_settled_bets(
    pool: &PgPool,
    limit: i64,
) -> anyhow::Result<Vec<SettledBetSeed>> {
    let rows = sqlx::query(
        r#"
        SELECT game, game_id, settled_at::TEXT AS settled_at, tx_hash
        FROM (
            SELECT
                'blackjack' AS game,
                chain_hand_id AS game_id,
                updated_at AS settled_at,
                NULL::TEXT AS tx_hash
            FROM blackjack_hands
            WHERE chain_hand_id IS NOT NULL
              AND (status = 'settled' OR phase = 'settled')
            UNION ALL
            SELECT
                'dice' AS game,
                round_id AS game_id,
                updated_at AS settled_at,
                settle_tx_hash AS tx_hash
            FROM dice_server_commitments
            WHERE status = 'settled' AND round_id IS NOT NULL
            UNION ALL
            SELECT
                'roulette' AS game,
                spin_id AS game_id,
                updated_at AS settled_at,
                settle_tx_hash AS tx_hash
            FROM roulette_server_commitments
            WHERE status = 'settled' AND spin_id IS NOT NULL
            UNION ALL
            SELECT
                'baccarat' AS game,
                round_id AS game_id,
                updated_at AS settled_at,
                settle_tx_hash AS tx_hash
            FROM baccarat_server_commitments
            WHERE status = 'settled' AND round_id IS NOT NULL
        ) settled
        ORDER BY settled_at DESC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("failed to query settled bet feed seeds")?;

    rows.into_iter()
        .map(|row| {
            Ok(SettledBetSeed {
                game: row.try_get("game").context("missing settled bet game")?,
                game_id: row
                    .try_get("game_id")
                    .context("missing settled bet game id")?,
                settled_at: row
                    .try_get("settled_at")
                    .context("missing settled bet timestamp")?,
                tx_hash: row.try_get("tx_hash").ok(),
            })
        })
        .collect()
}

pub async fn count_live_activity_for_game(
    pool: &PgPool,
    game_kind: &str,
    table_id: u64,
) -> anyhow::Result<u64> {
    let normalized = game_kind.to_ascii_lowercase();

    let count: i64 = match normalized.as_str() {
        "blackjack" => sqlx::query_scalar(
            r#"
                SELECT COUNT(DISTINCT player_id)
                FROM game_sessions
                WHERE table_id = $1
                  AND game = 'blackjack'
                  AND expires_at > NOW()
                  AND status NOT IN ('settled', 'expired')
                  AND phase NOT IN ('settled', 'expired')
                "#,
        )
        .bind(i64::try_from(table_id).context("table id does not fit in i64")?)
        .fetch_one(pool)
        .await
        .context("failed to count live blackjack sessions")?,
        "dice" => sqlx::query_scalar(
            r#"
                SELECT COUNT(*)
                FROM dice_server_commitments
                WHERE round_id IS NOT NULL
                  AND status <> 'settled'
                "#,
        )
        .fetch_one(pool)
        .await
        .context("failed to count live dice rounds")?,
        "roulette" => sqlx::query_scalar(
            r#"
                SELECT COUNT(*)
                FROM roulette_server_commitments
                WHERE spin_id IS NOT NULL
                  AND status <> 'settled'
                "#,
        )
        .fetch_one(pool)
        .await
        .context("failed to count live roulette rounds")?,
        "baccarat" => sqlx::query_scalar(
            r#"
                SELECT COUNT(*)
                FROM baccarat_server_commitments
                WHERE round_id IS NOT NULL
                  AND status <> 'settled'
                "#,
        )
        .fetch_one(pool)
        .await
        .context("failed to count live baccarat rounds")?,
        _ => 0,
    };

    Ok(u64::try_from(count.max(0)).unwrap_or(0))
}

pub async fn get_player_profile_by_wallet(
    pool: &PgPool,
    wallet_address: &str,
) -> anyhow::Result<Option<PlayerProfileRecord>> {
    let normalized = accounts::normalize_wallet_address(wallet_address);
    let row = sqlx::query(
        r#"
        SELECT
            p.id AS player_id,
            p.wallet_address,
            pp.username,
            pp.auth_provider,
            pp.auth_subject,
            TO_CHAR(pp.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS created_at,
            TO_CHAR(pp.updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS updated_at
        FROM player_wallets w
        INNER JOIN players p ON p.id = w.player_id
        INNER JOIN player_profiles pp ON pp.player_id = p.id
        WHERE w.wallet_address = $1
        "#,
    )
    .bind(normalized)
    .fetch_optional(pool)
    .await
    .context("failed to query player profile by wallet")?;

    row.map(hydrate_player_profile).transpose()
}

pub async fn get_player_profile_by_username(
    pool: &PgPool,
    username: &str,
) -> anyhow::Result<Option<PlayerProfileRecord>> {
    let row = sqlx::query(
        r#"
        SELECT
            p.id AS player_id,
            p.wallet_address,
            pp.username,
            pp.auth_provider,
            pp.auth_subject,
            TO_CHAR(pp.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS created_at,
            TO_CHAR(pp.updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS updated_at
        FROM player_profiles pp
        INNER JOIN players p ON p.id = pp.player_id
        WHERE pp.username = $1
        "#,
    )
    .bind(username)
    .fetch_optional(pool)
    .await
    .context("failed to query player profile by username")?;

    row.map(hydrate_player_profile).transpose()
}

pub async fn get_player_profile_by_user_id(
    pool: &PgPool,
    user_id: &str,
) -> anyhow::Result<Option<PlayerProfileRecord>> {
    let player_id = Uuid::parse_str(user_id).context("invalid canonical user id")?;
    let row = sqlx::query(
        r#"
        SELECT
            p.id AS player_id,
            p.wallet_address,
            pp.username,
            pp.auth_provider,
            pp.auth_subject,
            TO_CHAR(pp.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS created_at,
            TO_CHAR(pp.updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS updated_at
        FROM player_profiles pp
        INNER JOIN players p ON p.id = pp.player_id
        WHERE pp.player_id = $1
        "#,
    )
    .bind(player_id)
    .fetch_optional(pool)
    .await
    .context("failed to query player profile by user id")?;

    row.map(hydrate_player_profile).transpose()
}

pub async fn username_is_available(pool: &PgPool, username: &str) -> anyhow::Result<bool> {
    Ok(get_player_profile_by_username(pool, username)
        .await?
        .is_none())
}

pub async fn upsert_player_profile(
    pool: &PgPool,
    wallet_address: &str,
    username: Option<&str>,
    auth_provider: &str,
    auth_subject: Option<&str>,
) -> anyhow::Result<PlayerProfileRecord> {
    let normalized_wallet = accounts::normalize_wallet_address(wallet_address);
    let account = accounts::ensure_player_account(
        pool,
        EnsurePlayerAccountInput {
            wallet_address: normalized_wallet.clone(),
            auth_provider: (!auth_provider.trim().is_empty()).then(|| auth_provider.to_string()),
            auth_subject: auth_subject.map(ToString::to_string),
            linked_via: Some("profile".to_string()),
            make_primary: true,
        },
    )
    .await?;
    sqlx::query(
        r#"
        INSERT INTO player_profiles (
            player_id,
            wallet_address,
            username,
            auth_provider,
            auth_subject
        )
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (player_id) DO UPDATE
        SET username = EXCLUDED.username,
            wallet_address = EXCLUDED.wallet_address,
            auth_provider = EXCLUDED.auth_provider,
            auth_subject = EXCLUDED.auth_subject,
            updated_at = NOW()
        "#,
    )
    .bind(Uuid::parse_str(&account.user_id).context("invalid canonical user id")?)
    .bind(&normalized_wallet)
    .bind(username)
    .bind(auth_provider)
    .bind(auth_subject)
    .execute(pool)
    .await
    .context("failed to upsert player profile")?;

    get_player_profile_by_user_id(pool, &account.user_id)
        .await?
        .context("player profile missing after upsert")
}

pub async fn list_player_profiles_by_wallets(
    pool: &PgPool,
    wallet_addresses: &[String],
) -> anyhow::Result<HashMap<String, PlayerProfileRecord>> {
    if wallet_addresses.is_empty() {
        return Ok(HashMap::new());
    }

    let rows = sqlx::query(
        r#"
        SELECT
            p.id AS player_id,
            p.wallet_address,
            pp.username,
            pp.auth_provider,
            pp.auth_subject,
            TO_CHAR(pp.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS created_at,
            TO_CHAR(pp.updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS updated_at,
            w.wallet_address AS lookup_wallet
        FROM player_wallets w
        INNER JOIN players p ON p.id = w.player_id
        INNER JOIN player_profiles pp ON pp.player_id = p.id
        WHERE w.wallet_address = ANY($1)
        "#,
    )
    .bind(
        wallet_addresses
            .iter()
            .map(|value| accounts::normalize_wallet_address(value))
            .collect::<Vec<_>>(),
    )
    .fetch_all(pool)
    .await
    .context("failed to query player profiles by wallet list")?;

    rows.into_iter()
        .map(|row| {
            let lookup_wallet = row
                .try_get::<String, _>("lookup_wallet")
                .context("missing lookup_wallet")?;
            let profile = hydrate_player_profile(row)?;
            Ok((accounts::normalize_wallet_address(&lookup_wallet), profile))
        })
        .collect()
}

pub async fn create_dice_server_commitment(
    pool: &PgPool,
    commitment_id: u64,
    server_seed: &str,
    server_seed_hash: &str,
    reveal_deadline_block: u64,
    commit_tx_hash: &str,
) -> anyhow::Result<DiceServerCommitmentRecord> {
    sqlx::query(
        r#"
        INSERT INTO dice_server_commitments (
            commitment_id,
            server_seed,
            server_seed_hash,
            reveal_deadline_block,
            status,
            commit_tx_hash
        )
        VALUES ($1, $2, $3, $4, 'available', $5)
        ON CONFLICT (commitment_id)
        DO UPDATE SET
            server_seed = EXCLUDED.server_seed,
            server_seed_hash = EXCLUDED.server_seed_hash,
            reveal_deadline_block = EXCLUDED.reveal_deadline_block,
            status = EXCLUDED.status,
            commit_tx_hash = EXCLUDED.commit_tx_hash,
            updated_at = NOW()
        "#,
    )
    .bind(i64::try_from(commitment_id).context("commitment id does not fit in i64")?)
    .bind(server_seed)
    .bind(server_seed_hash)
    .bind(i64::try_from(reveal_deadline_block).context("reveal deadline does not fit in i64")?)
    .bind(commit_tx_hash)
    .execute(pool)
    .await
    .context("failed to persist dice server commitment")?;
    get_dice_server_commitment(pool, commitment_id)
        .await?
        .context("inserted dice commitment could not be read back")
}

pub async fn get_dice_server_commitment(
    pool: &PgPool,
    commitment_id: u64,
) -> anyhow::Result<Option<DiceServerCommitmentRecord>> {
    let row = sqlx::query(
        r#"
        SELECT
            commitment_id,
            server_seed,
            server_seed_hash,
            reveal_deadline_block,
            status,
            round_id,
            commit_tx_hash,
            settle_tx_hash
        FROM dice_server_commitments
        WHERE commitment_id = $1
        "#,
    )
    .bind(i64::try_from(commitment_id).context("commitment id does not fit in i64")?)
    .fetch_optional(pool)
    .await
    .context("failed to query dice server commitment")?;
    row.map(hydrate_dice_server_commitment).transpose()
}

pub async fn mark_dice_commitment_settled(
    pool: &PgPool,
    commitment_id: u64,
    round_id: u64,
    settle_tx_hash: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        UPDATE dice_server_commitments
        SET status = 'settled',
            round_id = $2,
            settle_tx_hash = $3,
            updated_at = NOW()
        WHERE commitment_id = $1
        "#,
    )
    .bind(i64::try_from(commitment_id).context("commitment id does not fit in i64")?)
    .bind(i64::try_from(round_id).context("round id does not fit in i64")?)
    .bind(settle_tx_hash)
    .execute(pool)
    .await
    .context("failed to mark dice commitment settled")?;
    Ok(())
}

pub async fn mark_dice_commitment_opened(
    pool: &PgPool,
    commitment_id: u64,
    round_id: u64,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        UPDATE dice_server_commitments
        SET status = 'opened',
            round_id = $2,
            updated_at = NOW()
        WHERE commitment_id = $1
        "#,
    )
    .bind(i64::try_from(commitment_id).context("commitment id does not fit in i64")?)
    .bind(i64::try_from(round_id).context("round id does not fit in i64")?)
    .execute(pool)
    .await
    .context("failed to mark dice commitment opened")?;
    Ok(())
}

pub async fn create_roulette_server_commitment(
    pool: &PgPool,
    commitment_id: u64,
    server_seed: &str,
    server_seed_hash: &str,
    reveal_deadline_block: u64,
    commit_tx_hash: &str,
) -> anyhow::Result<DiceServerCommitmentRecord> {
    sqlx::query(
        r#"
        INSERT INTO roulette_server_commitments (
            commitment_id,
            server_seed,
            server_seed_hash,
            reveal_deadline_block,
            status,
            commit_tx_hash
        )
        VALUES ($1, $2, $3, $4, 'available', $5)
        ON CONFLICT (commitment_id)
        DO UPDATE SET
            server_seed = EXCLUDED.server_seed,
            server_seed_hash = EXCLUDED.server_seed_hash,
            reveal_deadline_block = EXCLUDED.reveal_deadline_block,
            status = EXCLUDED.status,
            commit_tx_hash = EXCLUDED.commit_tx_hash,
            updated_at = NOW()
        "#,
    )
    .bind(i64::try_from(commitment_id).context("commitment id does not fit in i64")?)
    .bind(server_seed)
    .bind(server_seed_hash)
    .bind(i64::try_from(reveal_deadline_block).context("reveal deadline does not fit in i64")?)
    .bind(commit_tx_hash)
    .execute(pool)
    .await
    .context("failed to persist roulette server commitment")?;
    get_roulette_server_commitment(pool, commitment_id)
        .await?
        .context("inserted roulette commitment could not be read back")
}

pub async fn get_roulette_server_commitment(
    pool: &PgPool,
    commitment_id: u64,
) -> anyhow::Result<Option<DiceServerCommitmentRecord>> {
    let row = sqlx::query(
        r#"
        SELECT
            commitment_id,
            server_seed,
            server_seed_hash,
            reveal_deadline_block,
            status,
            spin_id AS round_id,
            commit_tx_hash,
            settle_tx_hash
        FROM roulette_server_commitments
        WHERE commitment_id = $1
        "#,
    )
    .bind(i64::try_from(commitment_id).context("commitment id does not fit in i64")?)
    .fetch_optional(pool)
    .await
    .context("failed to query roulette server commitment")?;
    row.map(hydrate_dice_server_commitment).transpose()
}

pub async fn mark_roulette_commitment_settled(
    pool: &PgPool,
    commitment_id: u64,
    spin_id: u64,
    settle_tx_hash: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        UPDATE roulette_server_commitments
        SET status = 'settled',
            spin_id = $2,
            settle_tx_hash = $3,
            updated_at = NOW()
        WHERE commitment_id = $1
        "#,
    )
    .bind(i64::try_from(commitment_id).context("commitment id does not fit in i64")?)
    .bind(i64::try_from(spin_id).context("spin id does not fit in i64")?)
    .bind(settle_tx_hash)
    .execute(pool)
    .await
    .context("failed to mark roulette commitment settled")?;
    Ok(())
}

pub async fn mark_roulette_commitment_opened(
    pool: &PgPool,
    commitment_id: u64,
    spin_id: u64,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        UPDATE roulette_server_commitments
        SET status = 'opened',
            spin_id = $2,
            updated_at = NOW()
        WHERE commitment_id = $1
        "#,
    )
    .bind(i64::try_from(commitment_id).context("commitment id does not fit in i64")?)
    .bind(i64::try_from(spin_id).context("spin id does not fit in i64")?)
    .execute(pool)
    .await
    .context("failed to mark roulette commitment opened")?;
    Ok(())
}

pub async fn create_baccarat_server_commitment(
    pool: &PgPool,
    commitment_id: u64,
    server_seed: &str,
    server_seed_hash: &str,
    reveal_deadline_block: u64,
    commit_tx_hash: &str,
) -> anyhow::Result<DiceServerCommitmentRecord> {
    sqlx::query(
        r#"
        INSERT INTO baccarat_server_commitments (
            commitment_id,
            server_seed,
            server_seed_hash,
            reveal_deadline_block,
            status,
            commit_tx_hash
        )
        VALUES ($1, $2, $3, $4, 'available', $5)
        ON CONFLICT (commitment_id)
        DO UPDATE SET
            server_seed = EXCLUDED.server_seed,
            server_seed_hash = EXCLUDED.server_seed_hash,
            reveal_deadline_block = EXCLUDED.reveal_deadline_block,
            status = EXCLUDED.status,
            commit_tx_hash = EXCLUDED.commit_tx_hash,
            updated_at = NOW()
        "#,
    )
    .bind(i64::try_from(commitment_id).context("commitment id does not fit in i64")?)
    .bind(server_seed)
    .bind(server_seed_hash)
    .bind(i64::try_from(reveal_deadline_block).context("reveal deadline does not fit in i64")?)
    .bind(commit_tx_hash)
    .execute(pool)
    .await
    .context("failed to persist baccarat server commitment")?;
    get_baccarat_server_commitment(pool, commitment_id)
        .await?
        .context("inserted baccarat commitment could not be read back")
}

pub async fn get_baccarat_server_commitment(
    pool: &PgPool,
    commitment_id: u64,
) -> anyhow::Result<Option<DiceServerCommitmentRecord>> {
    let row = sqlx::query(
        r#"
        SELECT
            commitment_id,
            server_seed,
            server_seed_hash,
            reveal_deadline_block,
            status,
            round_id,
            commit_tx_hash,
            settle_tx_hash
        FROM baccarat_server_commitments
        WHERE commitment_id = $1
        "#,
    )
    .bind(i64::try_from(commitment_id).context("commitment id does not fit in i64")?)
    .fetch_optional(pool)
    .await
    .context("failed to query baccarat server commitment")?;
    row.map(hydrate_dice_server_commitment).transpose()
}

pub async fn mark_baccarat_commitment_settled(
    pool: &PgPool,
    commitment_id: u64,
    round_id: u64,
    settle_tx_hash: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        UPDATE baccarat_server_commitments
        SET status = 'settled',
            round_id = $2,
            settle_tx_hash = $3,
            updated_at = NOW()
        WHERE commitment_id = $1
        "#,
    )
    .bind(i64::try_from(commitment_id).context("commitment id does not fit in i64")?)
    .bind(i64::try_from(round_id).context("round id does not fit in i64")?)
    .bind(settle_tx_hash)
    .execute(pool)
    .await
    .context("failed to mark baccarat commitment settled")?;
    Ok(())
}

pub async fn mark_baccarat_commitment_opened(
    pool: &PgPool,
    commitment_id: u64,
    round_id: u64,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        UPDATE baccarat_server_commitments
        SET status = 'opened',
            round_id = $2,
            updated_at = NOW()
        WHERE commitment_id = $1
        "#,
    )
    .bind(i64::try_from(commitment_id).context("commitment id does not fit in i64")?)
    .bind(i64::try_from(round_id).context("round id does not fit in i64")?)
    .execute(pool)
    .await
    .context("failed to mark baccarat commitment opened")?;
    Ok(())
}

pub async fn seed_blackjack_view(
    pool: &PgPool,
    hand: &BlackjackHandRecord,
    server_seed_hash: &str,
    server_seed: &str,
    client_seed: Option<&str>,
) -> anyhow::Result<BlackjackHandView> {
    let snapshot = blackjack::seed_hand_snapshot_with_secret(
        &hand.hand_id,
        &hand.player,
        hand.table_id,
        &hand.wager,
        &hand.transcript_root,
        server_seed_hash,
        server_seed,
        client_seed,
    )?;
    let hand_id = Uuid::parse_str(&hand.hand_id).context("invalid hand_id in record")?;
    store_blackjack_snapshot(pool, hand_id, &snapshot).await?;
    Ok(blackjack::snapshot_to_view(&snapshot))
}

pub async fn get_blackjack_view(
    pool: &PgPool,
    hand_id: Uuid,
) -> anyhow::Result<Option<BlackjackHandView>> {
    Ok(get_blackjack_snapshot(pool, hand_id)
        .await?
        .map(|snapshot| blackjack::snapshot_to_view(&snapshot)))
}

pub async fn get_blackjack_snapshot(
    pool: &PgPool,
    hand_id: Uuid,
) -> anyhow::Result<Option<BlackjackHandSnapshot>> {
    let row = sqlx::query(
        r#"
        SELECT snapshot
        FROM blackjack_hand_views
        WHERE hand_id = $1
        "#,
    )
    .bind(hand_id)
    .fetch_optional(pool)
    .await
    .context("failed to query blackjack hand snapshot")?;

    row.map(|row| {
        let snapshot = row
            .try_get::<Value, _>("snapshot")
            .context("missing snapshot column")?;
        serde_json::from_value(snapshot).context("failed to deserialize blackjack hand snapshot")
    })
    .transpose()
}

pub async fn store_blackjack_snapshot(
    pool: &PgPool,
    hand_id: Uuid,
    snapshot: &BlackjackHandSnapshot,
) -> anyhow::Result<()> {
    let payload =
        serde_json::to_string(snapshot).context("failed to serialize blackjack snapshot")?;
    let dealer_upcard = snapshot
        .dealer
        .cards
        .first()
        .map(|card| i16::from(card.rank));
    sqlx::query(
        r#"
        INSERT INTO blackjack_hand_views (hand_id, snapshot, updated_at)
        VALUES ($1, $2::jsonb, NOW())
        ON CONFLICT (hand_id)
        DO UPDATE SET snapshot = EXCLUDED.snapshot, updated_at = NOW()
        "#,
    )
    .bind(hand_id)
    .bind(&payload)
    .execute(pool)
    .await
    .context("failed to store blackjack hand snapshot")?;

    sqlx::query(
        r#"
        UPDATE blackjack_hands
        SET
            status = $2,
            phase = $3,
            active_seat = $4,
            seat_count = $5,
            dealer_upcard = $6,
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(hand_id)
    .bind(&snapshot.status)
    .bind(&snapshot.phase)
    .bind(i16::from(snapshot.active_seat))
    .bind(i16::from(snapshot.seat_count))
    .bind(dealer_upcard)
    .execute(pool)
    .await
    .context("failed to sync blackjack hand metadata")?;
    Ok(())
}

pub async fn append_blackjack_event(
    pool: &PgPool,
    hand_id: Uuid,
    session_id: Option<Uuid>,
    event_type: &str,
    payload: Value,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO blackjack_hand_events (id, hand_id, session_id, event_type, payload)
        VALUES ($1, $2, $3, $4, $5::jsonb)
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(hand_id)
    .bind(session_id)
    .bind(event_type)
    .bind(payload.to_string())
    .execute(pool)
    .await
    .context("failed to append blackjack hand event")?;
    Ok(())
}

pub async fn set_chain_hand_id(
    pool: &PgPool,
    hand_id: Uuid,
    chain_hand_id: i64,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        UPDATE blackjack_hands
        SET chain_hand_id = $2, updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(hand_id)
    .bind(chain_hand_id)
    .execute(pool)
    .await
    .context("failed to persist chain hand id")?;
    Ok(())
}

pub async fn create_blackjack_hand(
    pool: &PgPool,
    input: CreateHandInput,
) -> anyhow::Result<BlackjackHandRecord> {
    let hand_id = input
        .hand_id
        .as_deref()
        .map(Uuid::parse_str)
        .transpose()
        .context("invalid provided hand_id")?
        .unwrap_or_else(Uuid::now_v7);
    let session_id = input
        .session_id
        .as_deref()
        .map(Uuid::parse_str)
        .transpose()
        .context("invalid provided session_id")?
        .unwrap_or_else(Uuid::now_v7);
    let mut tx = pool
        .begin()
        .await
        .context("failed to begin hand transaction")?;

    let player_id = accounts::ensure_player_account_tx(
        &mut tx,
        &EnsurePlayerAccountInput {
            wallet_address: input.player.clone(),
            auth_provider: None,
            auth_subject: None,
            linked_via: Some("gameplay".to_string()),
            make_primary: false,
        },
    )
    .await?;
    balances::ensure_balance_account_tx(&mut tx, player_id).await?;

    sqlx::query(
        r#"
        INSERT INTO blackjack_hands (
            id,
            player_id,
            table_id,
            wager,
            status,
            phase,
            transcript_root,
            active_seat,
            seat_count
        )
        VALUES ($1, $2, $3, $4, 'pending_open', 'coordinator_pending', $5, 0, 1)
        "#,
    )
    .bind(hand_id)
    .bind(player_id)
    .bind(i64::try_from(input.table_id).context("table id does not fit in i64")?)
    .bind(&input.wager)
    .bind(&input.transcript_root)
    .execute(&mut *tx)
    .await
    .context("failed to insert blackjack hand")?;

    sqlx::query(
        r#"
        INSERT INTO game_sessions (
            id,
            player_id,
            hand_id,
            session_key,
            table_id,
            game,
            status,
            phase,
            transcript_root,
            max_wager,
            expires_at
        )
        VALUES (
            $1,
            $2,
            $3,
            $4,
            $5,
            'blackjack',
            'active',
            'awaiting_open',
            $6,
            $7,
            TO_TIMESTAMP($8)
        )
        "#,
    )
    .bind(session_id)
    .bind(player_id)
    .bind(hand_id)
    .bind(&input.relay_token)
    .bind(i64::try_from(input.table_id).context("table id does not fit in i64")?)
    .bind(&input.transcript_root)
    .bind(&input.wager)
    .bind(input.expires_at_unix as f64)
    .execute(&mut *tx)
    .await
    .context("failed to insert game session")?;

    let event_payload = serde_json::json!({
        "session_id": session_id,
        "phase": "awaiting_open",
        "table_id": input.table_id,
    });

    sqlx::query(
        r#"
        INSERT INTO blackjack_hand_events (id, hand_id, session_id, event_type, payload)
        VALUES ($1, $2, $3, 'session.created', $4::jsonb)
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(hand_id)
    .bind(session_id)
    .bind(event_payload.to_string())
    .execute(&mut *tx)
    .await
    .context("failed to insert blackjack hand event")?;

    tx.commit()
        .await
        .context("failed to commit hand transaction")?;
    get_blackjack_hand(pool, hand_id)
        .await?
        .context("inserted hand could not be read back")
}

pub async fn get_blackjack_hand(
    pool: &PgPool,
    hand_id: Uuid,
) -> anyhow::Result<Option<BlackjackHandRecord>> {
    let row = sqlx::query(
        r#"
        SELECT
            h.id,
            p.wallet_address,
            h.table_id,
            h.wager,
            h.status,
            h.phase,
            h.transcript_root,
            h.active_seat,
            h.seat_count,
            h.dealer_upcard,
            h.chain_hand_id,
            s.id AS session_id,
            s.session_key AS relay_token,
            TO_CHAR(h.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS created_at,
            TO_CHAR(h.updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS updated_at
        FROM blackjack_hands h
        INNER JOIN players p ON p.id = h.player_id
        LEFT JOIN game_sessions s ON s.hand_id = h.id
        WHERE h.id = $1
        "#,
    )
    .bind(hand_id)
    .fetch_optional(pool)
    .await
    .context("failed to query blackjack hand")?;

    row.map(hydrate_hand_record).transpose()
}

pub async fn get_blackjack_hand_by_chain_hand_id(
    pool: &PgPool,
    chain_hand_id: i64,
) -> anyhow::Result<Option<BlackjackHandRecord>> {
    let row = sqlx::query(
        r#"
        SELECT
            h.id,
            p.wallet_address,
            h.table_id,
            h.wager,
            h.status,
            h.phase,
            h.transcript_root,
            h.active_seat,
            h.seat_count,
            h.dealer_upcard,
            h.chain_hand_id,
            s.id AS session_id,
            s.session_key AS relay_token,
            TO_CHAR(h.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS created_at,
            TO_CHAR(h.updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS updated_at
        FROM blackjack_hands h
        INNER JOIN players p ON p.id = h.player_id
        LEFT JOIN game_sessions s ON s.hand_id = h.id
        WHERE h.chain_hand_id = $1
        ORDER BY h.updated_at DESC
        LIMIT 1
        "#,
    )
    .bind(chain_hand_id)
    .fetch_optional(pool)
    .await
    .context("failed to query blackjack hand by chain hand id")?;

    row.map(hydrate_hand_record).transpose()
}

pub async fn delete_blackjack_hand(pool: &PgPool, hand_id: Uuid) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        DELETE FROM blackjack_hands
        WHERE id = $1
        "#,
    )
    .bind(hand_id)
    .execute(pool)
    .await
    .context("failed to delete blackjack hand")?;
    Ok(())
}

fn hydrate_hand_record(row: sqlx::postgres::PgRow) -> anyhow::Result<BlackjackHandRecord> {
    let table_id: i64 = row.try_get("table_id").context("missing table id")?;
    let active_seat: i16 = row.try_get("active_seat").context("missing active seat")?;
    let seat_count: i16 = row.try_get("seat_count").context("missing seat count")?;
    let dealer_upcard: Option<i16> = row.try_get("dealer_upcard").ok();
    let session_id: Option<Uuid> = row.try_get("session_id").ok();

    Ok(BlackjackHandRecord {
        hand_id: row
            .try_get::<Uuid, _>("id")
            .context("missing hand id")?
            .to_string(),
        session_id: session_id.map(|value| value.to_string()),
        player: row
            .try_get::<String, _>("wallet_address")
            .context("missing wallet address")?,
        relay_token: row.try_get("relay_token").ok(),
        table_id: u64::try_from(table_id).context("negative table id")?,
        wager: row.try_get::<String, _>("wager").context("missing wager")?,
        status: row
            .try_get::<String, _>("status")
            .context("missing status")?,
        phase: row.try_get::<String, _>("phase").context("missing phase")?,
        transcript_root: row
            .try_get::<String, _>("transcript_root")
            .context("missing transcript root")?,
        active_seat: u8::try_from(active_seat).context("invalid active seat")?,
        seat_count: u8::try_from(seat_count).context("invalid seat count")?,
        dealer_upcard: dealer_upcard.map(|value| value as u8),
        chain_hand_id: row.try_get("chain_hand_id").ok(),
        created_at: row
            .try_get::<String, _>("created_at")
            .context("missing created_at")?,
        updated_at: row
            .try_get::<String, _>("updated_at")
            .context("missing updated_at")?,
    })
}

fn hydrate_dice_server_commitment(
    row: sqlx::postgres::PgRow,
) -> anyhow::Result<DiceServerCommitmentRecord> {
    Ok(DiceServerCommitmentRecord {
        commitment_id: row
            .try_get::<i64, _>("commitment_id")
            .context("missing commitment_id")?,
        server_seed: row
            .try_get::<String, _>("server_seed")
            .context("missing server_seed")?,
        server_seed_hash: row
            .try_get::<String, _>("server_seed_hash")
            .context("missing server_seed_hash")?,
        reveal_deadline_block: row
            .try_get::<i64, _>("reveal_deadline_block")
            .context("missing reveal_deadline_block")?,
        status: row
            .try_get::<String, _>("status")
            .context("missing status")?,
        round_id: row.try_get("round_id").ok(),
        commit_tx_hash: row.try_get("commit_tx_hash").ok(),
        settle_tx_hash: row.try_get("settle_tx_hash").ok(),
    })
}

fn hydrate_player_profile(row: sqlx::postgres::PgRow) -> anyhow::Result<PlayerProfileRecord> {
    Ok(PlayerProfileRecord {
        user_id: row
            .try_get::<Uuid, _>("player_id")
            .context("missing player_id")?
            .to_string(),
        wallet_address: row
            .try_get::<Option<String>, _>("wallet_address")
            .context("missing wallet_address")?,
        username: row
            .try_get::<Option<String>, _>("username")
            .context("missing username")?,
        auth_provider: row
            .try_get::<String, _>("auth_provider")
            .context("missing auth_provider")?,
        auth_subject: row.try_get("auth_subject").ok(),
        created_at: row
            .try_get::<String, _>("created_at")
            .context("missing created_at")?,
        updated_at: row
            .try_get::<String, _>("updated_at")
            .context("missing updated_at")?,
    })
}
