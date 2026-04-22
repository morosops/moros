use anyhow::{Context, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalanceAccountRecord {
    pub user_id: String,
    pub gambling_balance: String,
    pub gambling_reserved: String,
    pub vault_balance: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalanceReservationRecord {
    pub reservation_id: String,
    pub user_id: String,
    pub game_kind: String,
    pub reference_id: String,
    pub amount: String,
    pub status: String,
    pub payout_amount: Option<String>,
    pub metadata: Value,
    pub created_at: String,
    pub updated_at: String,
}

pub async fn ensure_balance_account(pool: &PgPool, player_id: Uuid) -> anyhow::Result<()> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to open balance account transaction")?;
    ensure_balance_account_tx(&mut tx, player_id).await?;
    tx.commit()
        .await
        .context("failed to commit balance account transaction")?;
    Ok(())
}

pub async fn ensure_balance_account_tx(
    tx: &mut Transaction<'_, Postgres>,
    player_id: Uuid,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO bankroll_accounts (
            player_id,
            public_balance,
            reserved_balance,
            bankroll_status,
            gambling_balance,
            gambling_reserved,
            vault_balance
        )
        VALUES ($1, '0', '0', 'active', '0', '0', '0')
        ON CONFLICT (player_id) DO NOTHING
        "#,
    )
    .bind(player_id)
    .execute(&mut **tx)
    .await
    .context("failed to ensure balance account row")?;
    Ok(())
}

pub async fn get_balance_account(
    pool: &PgPool,
    player_id: Uuid,
) -> anyhow::Result<Option<BalanceAccountRecord>> {
    let row = sqlx::query(
        r#"
        SELECT
            player_id,
            gambling_balance,
            gambling_reserved,
            vault_balance,
            TO_CHAR(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS updated_at
        FROM bankroll_accounts
        WHERE player_id = $1
        "#,
    )
    .bind(player_id)
    .fetch_optional(pool)
    .await
    .context("failed to query balance account")?;
    row.map(hydrate_balance_account).transpose()
}

pub async fn reconcile_onchain_vault_balances(
    pool: &PgPool,
    player_id: Uuid,
    gambling_balance: u128,
    vault_balance: u128,
    reference_kind: Option<&str>,
    reference_id: Option<&str>,
    metadata: Value,
) -> anyhow::Result<BalanceAccountRecord> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to open balance reconciliation transaction")?;
    let updated = reconcile_onchain_vault_balances_tx(
        &mut tx,
        player_id,
        gambling_balance,
        vault_balance,
        reference_kind,
        reference_id,
        metadata,
    )
    .await?;
    tx.commit()
        .await
        .context("failed to commit balance reconciliation")?;
    Ok(updated)
}

pub async fn reconcile_onchain_vault_balances_tx(
    tx: &mut Transaction<'_, Postgres>,
    player_id: Uuid,
    gambling_balance: u128,
    vault_balance: u128,
    reference_kind: Option<&str>,
    reference_id: Option<&str>,
    metadata: Value,
) -> anyhow::Result<BalanceAccountRecord> {
    ensure_balance_account_tx(tx, player_id).await?;
    let snapshot = lock_balance_row(tx, player_id).await?;

    if snapshot.available == gambling_balance && snapshot.vault == vault_balance {
        return get_balance_account_from_tx(tx, player_id).await;
    }

    update_balance_row(
        tx,
        player_id,
        gambling_balance,
        snapshot.reserved,
        vault_balance,
    )
    .await?;

    let gambling_delta = checked_u128_delta(snapshot.available, gambling_balance)
        .context("gambling balance reconciliation delta overflowed i128")?;
    if gambling_delta != 0 {
        append_ledger_entry(
            tx,
            player_id,
            "gambling",
            "onchain_reconciled",
            gambling_delta,
            reference_kind,
            reference_id,
            metadata.clone(),
        )
        .await?;
    }

    let vault_delta = checked_u128_delta(snapshot.vault, vault_balance)
        .context("vault balance reconciliation delta overflowed i128")?;
    if vault_delta != 0 {
        append_ledger_entry(
            tx,
            player_id,
            "vault",
            "onchain_reconciled",
            vault_delta,
            reference_kind,
            reference_id,
            metadata,
        )
        .await?;
    }

    get_balance_account_from_tx(tx, player_id).await
}

pub async fn credit_gambling_balance(
    pool: &PgPool,
    player_id: Uuid,
    amount: u128,
    entry_kind: &str,
    reference_kind: Option<&str>,
    reference_id: Option<&str>,
    metadata: Value,
) -> anyhow::Result<BalanceAccountRecord> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to open gambling credit transaction")?;
    let updated = credit_gambling_balance_tx(
        &mut tx,
        player_id,
        amount,
        entry_kind,
        reference_kind,
        reference_id,
        metadata,
    )
    .await?;
    tx.commit()
        .await
        .context("failed to commit gambling credit transaction")?;
    Ok(updated)
}

pub async fn credit_gambling_balance_tx(
    tx: &mut Transaction<'_, Postgres>,
    player_id: Uuid,
    amount: u128,
    entry_kind: &str,
    reference_kind: Option<&str>,
    reference_id: Option<&str>,
    metadata: Value,
) -> anyhow::Result<BalanceAccountRecord> {
    ensure_balance_account_tx(tx, player_id).await?;
    let snapshot = lock_balance_row(tx, player_id).await?;
    let next_available = snapshot.available.saturating_add(amount);
    update_balance_row(
        tx,
        player_id,
        next_available,
        snapshot.reserved,
        snapshot.vault,
    )
    .await?;
    append_ledger_entry(
        tx,
        player_id,
        "gambling",
        entry_kind,
        amount as i128,
        reference_kind,
        reference_id,
        metadata,
    )
    .await?;
    get_balance_account_from_tx(tx, player_id).await
}

pub async fn credit_balance_tx(
    tx: &mut Transaction<'_, Postgres>,
    player_id: Uuid,
    balance_scope: &str,
    amount: u128,
    entry_kind: &str,
    reference_kind: Option<&str>,
    reference_id: Option<&str>,
    metadata: Value,
) -> anyhow::Result<BalanceAccountRecord> {
    if amount == 0 {
        return Err(anyhow!("credit amount must be greater than zero"));
    }
    ensure_balance_account_tx(tx, player_id).await?;
    let snapshot = lock_balance_row(tx, player_id).await?;
    let (next_available, next_vault) = match balance_scope {
        "gambling" => (snapshot.available.saturating_add(amount), snapshot.vault),
        "vault" => (snapshot.available, snapshot.vault.saturating_add(amount)),
        _ => return Err(anyhow!("unsupported balance scope")),
    };

    update_balance_row(tx, player_id, next_available, snapshot.reserved, next_vault).await?;
    append_ledger_entry(
        tx,
        player_id,
        balance_scope,
        entry_kind,
        amount as i128,
        reference_kind,
        reference_id,
        metadata,
    )
    .await?;
    get_balance_account_from_tx(tx, player_id).await
}

pub async fn debit_balance_tx(
    tx: &mut Transaction<'_, Postgres>,
    player_id: Uuid,
    balance_scope: &str,
    amount: u128,
    entry_kind: &str,
    reference_kind: Option<&str>,
    reference_id: Option<&str>,
    metadata: Value,
) -> anyhow::Result<BalanceAccountRecord> {
    if amount == 0 {
        return Err(anyhow!("debit amount must be greater than zero"));
    }
    ensure_balance_account_tx(tx, player_id).await?;
    let snapshot = lock_balance_row(tx, player_id).await?;
    let (next_available, next_vault) = match balance_scope {
        "gambling" => {
            if snapshot.available < amount {
                return Err(anyhow!(
                    "insufficient gambling balance: have {}, need {}",
                    snapshot.available,
                    amount
                ));
            }
            (snapshot.available - amount, snapshot.vault)
        }
        "vault" => {
            if snapshot.vault < amount {
                return Err(anyhow!(
                    "insufficient vault balance: have {}, need {}",
                    snapshot.vault,
                    amount
                ));
            }
            (snapshot.available, snapshot.vault - amount)
        }
        _ => return Err(anyhow!("unsupported balance scope")),
    };

    update_balance_row(tx, player_id, next_available, snapshot.reserved, next_vault).await?;
    append_ledger_entry(
        tx,
        player_id,
        balance_scope,
        entry_kind,
        -(amount as i128),
        reference_kind,
        reference_id,
        metadata,
    )
    .await?;
    get_balance_account_from_tx(tx, player_id).await
}

pub async fn transfer_from_gambling_to_vault(
    pool: &PgPool,
    player_id: Uuid,
    amount: u128,
    reference_kind: Option<&str>,
    reference_id: Option<&str>,
    metadata: Value,
) -> anyhow::Result<BalanceAccountRecord> {
    transfer_between_scopes(
        pool,
        player_id,
        "gambling",
        "vault",
        amount,
        reference_kind,
        reference_id,
        metadata,
    )
    .await
}

pub async fn transfer_from_vault_to_gambling(
    pool: &PgPool,
    player_id: Uuid,
    amount: u128,
    reference_kind: Option<&str>,
    reference_id: Option<&str>,
    metadata: Value,
) -> anyhow::Result<BalanceAccountRecord> {
    transfer_between_scopes(
        pool,
        player_id,
        "vault",
        "gambling",
        amount,
        reference_kind,
        reference_id,
        metadata,
    )
    .await
}

pub async fn reserve_gambling_balance(
    pool: &PgPool,
    player_id: Uuid,
    game_kind: &str,
    reference_id: &str,
    amount: u128,
    metadata: Value,
) -> anyhow::Result<BalanceReservationRecord> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to open reserve balance transaction")?;
    let reservation = reserve_gambling_balance_tx(
        &mut tx,
        player_id,
        game_kind,
        reference_id,
        amount,
        metadata,
    )
    .await?;
    tx.commit()
        .await
        .context("failed to commit reserve balance transaction")?;
    Ok(reservation)
}

pub async fn reserve_gambling_balance_tx(
    tx: &mut Transaction<'_, Postgres>,
    player_id: Uuid,
    game_kind: &str,
    reference_id: &str,
    amount: u128,
    metadata: Value,
) -> anyhow::Result<BalanceReservationRecord> {
    if amount == 0 {
        return Err(anyhow!("reservation amount must be greater than zero"));
    }
    ensure_balance_account_tx(tx, player_id).await?;
    let snapshot = lock_balance_row(tx, player_id).await?;
    if snapshot.available < amount {
        return Err(anyhow!(
            "insufficient gambling balance: have {}, need {}",
            snapshot.available,
            amount
        ));
    }

    let existing = sqlx::query(
        r#"
        SELECT id, amount
        FROM balance_reservations
        WHERE player_id = $1
          AND game_kind = $2
          AND reference_id = $3
          AND status = 'active'
        FOR UPDATE
        "#,
    )
    .bind(player_id)
    .bind(game_kind)
    .bind(reference_id)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to read active balance reservation")?;

    let reservation_id;
    let next_amount;
    if let Some(existing) = existing {
        reservation_id = existing
            .try_get::<Uuid, _>("id")
            .context("missing active reservation id")?;
        let current_amount = parse_u128_amount(
            &existing
                .try_get::<String, _>("amount")
                .context("missing active reservation amount")?,
            "active reservation amount",
        )?;
        next_amount = current_amount.saturating_add(amount);
        sqlx::query(
            r#"
            UPDATE balance_reservations
            SET amount = $2,
                metadata = $3::jsonb,
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(reservation_id)
        .bind(next_amount.to_string())
        .bind(metadata.to_string())
        .execute(&mut **tx)
        .await
        .context("failed to extend active balance reservation")?;
    } else {
        reservation_id = Uuid::now_v7();
        next_amount = amount;
        sqlx::query(
            r#"
            INSERT INTO balance_reservations (
                id,
                player_id,
                game_kind,
                reference_id,
                amount,
                status,
                metadata
            )
            VALUES ($1, $2, $3, $4, $5, 'active', $6::jsonb)
            "#,
        )
        .bind(reservation_id)
        .bind(player_id)
        .bind(game_kind)
        .bind(reference_id)
        .bind(next_amount.to_string())
        .bind(metadata.to_string())
        .execute(&mut **tx)
        .await
        .context("failed to insert balance reservation")?;
    }

    update_balance_row(
        tx,
        player_id,
        snapshot.available - amount,
        snapshot.reserved + amount,
        snapshot.vault,
    )
    .await?;
    append_ledger_entry(
        tx,
        player_id,
        "gambling",
        "wager_reserved",
        -(amount as i128),
        Some(game_kind),
        Some(reference_id),
        serde_json::json!({
            "requested_amount": amount.to_string(),
        }),
    )
    .await?;
    get_reservation_from_tx(tx, reservation_id).await
}

pub async fn release_gambling_balance(
    pool: &PgPool,
    player_id: Uuid,
    game_kind: &str,
    reference_id: &str,
    metadata: Value,
) -> anyhow::Result<Option<BalanceReservationRecord>> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to open release balance transaction")?;
    let released =
        release_gambling_balance_tx(&mut tx, player_id, game_kind, reference_id, metadata).await?;
    tx.commit()
        .await
        .context("failed to commit release balance transaction")?;
    Ok(released)
}

pub async fn release_gambling_balance_tx(
    tx: &mut Transaction<'_, Postgres>,
    player_id: Uuid,
    game_kind: &str,
    reference_id: &str,
    metadata: Value,
) -> anyhow::Result<Option<BalanceReservationRecord>> {
    ensure_balance_account_tx(tx, player_id).await?;
    let snapshot = lock_balance_row(tx, player_id).await?;
    let reservation = fetch_active_reservation(tx, player_id, game_kind, reference_id).await?;
    let Some(reservation) = reservation else {
        return Ok(None);
    };
    let reserved_amount = parse_u128_amount(&reservation.amount, "reservation amount")?;
    update_balance_row(
        tx,
        player_id,
        snapshot.available.saturating_add(reserved_amount),
        snapshot.reserved.saturating_sub(reserved_amount),
        snapshot.vault,
    )
    .await?;
    sqlx::query(
        r#"
        UPDATE balance_reservations
        SET status = 'released',
            metadata = $2::jsonb,
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(Uuid::parse_str(&reservation.reservation_id).context("invalid reservation id")?)
    .bind(metadata.to_string())
    .execute(&mut **tx)
    .await
    .context("failed to release balance reservation")?;
    append_ledger_entry(
        tx,
        player_id,
        "gambling",
        "wager_released",
        reserved_amount as i128,
        Some(game_kind),
        Some(reference_id),
        metadata,
    )
    .await?;
    Ok(Some(
        get_reservation_from_tx(
            tx,
            Uuid::parse_str(&reservation.reservation_id).context("invalid reservation id")?,
        )
        .await?,
    ))
}

pub async fn reduce_gambling_reservation(
    pool: &PgPool,
    player_id: Uuid,
    game_kind: &str,
    reference_id: &str,
    amount: u128,
    metadata: Value,
) -> anyhow::Result<Option<BalanceReservationRecord>> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to open reduce reservation transaction")?;
    let reduced = reduce_gambling_reservation_tx(
        &mut tx,
        player_id,
        game_kind,
        reference_id,
        amount,
        metadata,
    )
    .await?;
    tx.commit()
        .await
        .context("failed to commit reduce reservation transaction")?;
    Ok(reduced)
}

pub async fn reduce_gambling_reservation_tx(
    tx: &mut Transaction<'_, Postgres>,
    player_id: Uuid,
    game_kind: &str,
    reference_id: &str,
    amount: u128,
    metadata: Value,
) -> anyhow::Result<Option<BalanceReservationRecord>> {
    if amount == 0 {
        return Ok(None);
    }
    ensure_balance_account_tx(tx, player_id).await?;
    let snapshot = lock_balance_row(tx, player_id).await?;
    let reservation = fetch_active_reservation(tx, player_id, game_kind, reference_id).await?;
    let Some(reservation) = reservation else {
        return Ok(None);
    };
    let reserved_amount = parse_u128_amount(&reservation.amount, "reservation amount")?;
    if reserved_amount < amount {
        return Err(anyhow!(
            "cannot reduce reservation by {} because only {} is reserved",
            amount,
            reserved_amount
        ));
    }
    let next_reserved_amount = reserved_amount - amount;
    let reservation_id =
        Uuid::parse_str(&reservation.reservation_id).context("invalid reservation id")?;
    sqlx::query(
        r#"
        UPDATE balance_reservations
        SET amount = $2,
            status = CASE WHEN $2 = '0' THEN 'released' ELSE status END,
            metadata = $3::jsonb,
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(reservation_id)
    .bind(next_reserved_amount.to_string())
    .bind(metadata.to_string())
    .execute(&mut **tx)
    .await
    .context("failed to reduce balance reservation")?;
    update_balance_row(
        tx,
        player_id,
        snapshot.available.saturating_add(amount),
        snapshot.reserved.saturating_sub(amount),
        snapshot.vault,
    )
    .await?;
    append_ledger_entry(
        tx,
        player_id,
        "gambling",
        "wager_reservation_reduced",
        i128::try_from(amount).context("reservation reduction amount overflowed i128")?,
        Some(game_kind),
        Some(reference_id),
        metadata,
    )
    .await?;
    if next_reserved_amount == 0 {
        Ok(None)
    } else {
        Ok(Some(get_reservation_from_tx(tx, reservation_id).await?))
    }
}

pub async fn settle_gambling_balance(
    pool: &PgPool,
    player_id: Uuid,
    game_kind: &str,
    reference_id: &str,
    payout: u128,
    metadata: Value,
) -> anyhow::Result<Option<BalanceReservationRecord>> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to open settle balance transaction")?;
    let settled = settle_gambling_balance_tx(
        &mut tx,
        player_id,
        game_kind,
        reference_id,
        payout,
        metadata,
    )
    .await?;
    tx.commit()
        .await
        .context("failed to commit settle balance transaction")?;
    Ok(settled)
}

pub async fn settle_gambling_balance_tx(
    tx: &mut Transaction<'_, Postgres>,
    player_id: Uuid,
    game_kind: &str,
    reference_id: &str,
    payout: u128,
    metadata: Value,
) -> anyhow::Result<Option<BalanceReservationRecord>> {
    ensure_balance_account_tx(tx, player_id).await?;
    let snapshot = lock_balance_row(tx, player_id).await?;
    let reservation = fetch_active_reservation(tx, player_id, game_kind, reference_id).await?;
    let Some(reservation) = reservation else {
        return Ok(None);
    };
    let reserved_amount = parse_u128_amount(&reservation.amount, "reservation amount")?;
    update_balance_row(
        tx,
        player_id,
        snapshot.available.saturating_add(payout),
        snapshot.reserved.saturating_sub(reserved_amount),
        snapshot.vault,
    )
    .await?;
    let reservation_id =
        Uuid::parse_str(&reservation.reservation_id).context("invalid reservation id")?;
    sqlx::query(
        r#"
        UPDATE balance_reservations
        SET status = 'settled',
            payout_amount = $2,
            metadata = $3::jsonb,
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(reservation_id)
    .bind(payout.to_string())
    .bind(metadata.to_string())
    .execute(&mut **tx)
    .await
    .context("failed to settle balance reservation")?;
    append_ledger_entry(
        tx,
        player_id,
        "gambling",
        "wager_settled",
        payout as i128,
        Some(game_kind),
        Some(reference_id),
        serde_json::json!({
            "reserved_amount": reserved_amount.to_string(),
            "payout": payout.to_string(),
            "metadata": metadata,
        }),
    )
    .await?;
    Ok(Some(get_reservation_from_tx(tx, reservation_id).await?))
}

pub async fn sync_gambling_reservation_from_chain_tx(
    tx: &mut Transaction<'_, Postgres>,
    player_id: Uuid,
    game_kind: &str,
    reference_id: &str,
    target_amount: u128,
    metadata: Value,
) -> anyhow::Result<Option<BalanceReservationRecord>> {
    ensure_balance_account_tx(tx, player_id).await?;
    let snapshot = lock_balance_row(tx, player_id).await?;
    let reservation = fetch_active_reservation(tx, player_id, game_kind, reference_id).await?;
    let current_amount = reservation
        .as_ref()
        .map(|value| parse_u128_amount(&value.amount, "reservation amount"))
        .transpose()?
        .unwrap_or(0);

    update_balance_row(
        tx,
        player_id,
        snapshot.available,
        snapshot
            .reserved
            .saturating_sub(current_amount)
            .saturating_add(target_amount),
        snapshot.vault,
    )
    .await?;

    let reservation_id = if let Some(existing) = reservation {
        let reservation_id =
            Uuid::parse_str(&existing.reservation_id).context("invalid reservation id")?;
        if target_amount == 0 {
            sqlx::query(
                r#"
                UPDATE balance_reservations
                SET status = 'released',
                    metadata = $2::jsonb,
                    updated_at = NOW()
                WHERE id = $1
                "#,
            )
            .bind(reservation_id)
            .bind(metadata.to_string())
            .execute(&mut **tx)
            .await
            .context("failed to release synced chain reservation")?;
        } else {
            sqlx::query(
                r#"
                UPDATE balance_reservations
                SET amount = $2,
                    metadata = $3::jsonb,
                    updated_at = NOW()
                WHERE id = $1
                "#,
            )
            .bind(reservation_id)
            .bind(target_amount.to_string())
            .bind(metadata.to_string())
            .execute(&mut **tx)
            .await
            .context("failed to sync active reservation from chain")?;
        }
        reservation_id
    } else if target_amount > 0 {
        let reservation_id = Uuid::now_v7();
        sqlx::query(
            r#"
            INSERT INTO balance_reservations (
                id,
                player_id,
                game_kind,
                reference_id,
                amount,
                status,
                metadata
            )
            VALUES ($1, $2, $3, $4, $5, 'active', $6::jsonb)
            "#,
        )
        .bind(reservation_id)
        .bind(player_id)
        .bind(game_kind)
        .bind(reference_id)
        .bind(target_amount.to_string())
        .bind(metadata.to_string())
        .execute(&mut **tx)
        .await
        .context("failed to insert chain reservation")?;
        reservation_id
    } else {
        return Ok(None);
    };

    let reserved_delta = checked_u128_delta(current_amount, target_amount)
        .context("reservation sync delta overflowed i128")?;
    if reserved_delta != 0 {
        append_ledger_entry(
            tx,
            player_id,
            "reserved",
            "reservation_synced_from_chain",
            reserved_delta,
            Some(game_kind),
            Some(reference_id),
            metadata,
        )
        .await?;
    }

    if target_amount == 0 {
        Ok(None)
    } else {
        Ok(Some(get_reservation_from_tx(tx, reservation_id).await?))
    }
}

pub async fn release_gambling_reservation_from_chain_tx(
    tx: &mut Transaction<'_, Postgres>,
    player_id: Uuid,
    game_kind: &str,
    reference_id: &str,
    metadata: Value,
) -> anyhow::Result<Option<BalanceReservationRecord>> {
    ensure_balance_account_tx(tx, player_id).await?;
    let snapshot = lock_balance_row(tx, player_id).await?;
    let reservation = fetch_active_reservation(tx, player_id, game_kind, reference_id).await?;
    let Some(reservation) = reservation else {
        return Ok(None);
    };
    let reserved_amount = parse_u128_amount(&reservation.amount, "reservation amount")?;
    let reservation_id =
        Uuid::parse_str(&reservation.reservation_id).context("invalid reservation id")?;
    update_balance_row(
        tx,
        player_id,
        snapshot.available,
        snapshot.reserved.saturating_sub(reserved_amount),
        snapshot.vault,
    )
    .await?;
    sqlx::query(
        r#"
        UPDATE balance_reservations
        SET status = 'released',
            metadata = $2::jsonb,
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(reservation_id)
    .bind(metadata.to_string())
    .execute(&mut **tx)
    .await
    .context("failed to release chain reservation")?;
    append_ledger_entry(
        tx,
        player_id,
        "reserved",
        "reservation_released_from_chain",
        -(reserved_amount as i128),
        Some(game_kind),
        Some(reference_id),
        metadata,
    )
    .await?;
    Ok(Some(get_reservation_from_tx(tx, reservation_id).await?))
}

pub async fn settle_gambling_reservation_from_chain_tx(
    tx: &mut Transaction<'_, Postgres>,
    player_id: Uuid,
    game_kind: &str,
    reference_id: &str,
    payout: u128,
    metadata: Value,
) -> anyhow::Result<Option<BalanceReservationRecord>> {
    ensure_balance_account_tx(tx, player_id).await?;
    let snapshot = lock_balance_row(tx, player_id).await?;
    let reservation = fetch_active_reservation(tx, player_id, game_kind, reference_id).await?;
    let Some(reservation) = reservation else {
        return Ok(None);
    };
    let reserved_amount = parse_u128_amount(&reservation.amount, "reservation amount")?;
    let reservation_id =
        Uuid::parse_str(&reservation.reservation_id).context("invalid reservation id")?;
    update_balance_row(
        tx,
        player_id,
        snapshot.available,
        snapshot.reserved.saturating_sub(reserved_amount),
        snapshot.vault,
    )
    .await?;
    sqlx::query(
        r#"
        UPDATE balance_reservations
        SET status = 'settled',
            payout_amount = $2,
            metadata = $3::jsonb,
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(reservation_id)
    .bind(payout.to_string())
    .bind(metadata.to_string())
    .execute(&mut **tx)
    .await
    .context("failed to settle chain reservation")?;
    append_ledger_entry(
        tx,
        player_id,
        "reserved",
        "reservation_settled_from_chain",
        -(reserved_amount as i128),
        Some(game_kind),
        Some(reference_id),
        metadata,
    )
    .await?;
    Ok(Some(get_reservation_from_tx(tx, reservation_id).await?))
}

async fn transfer_between_scopes(
    pool: &PgPool,
    player_id: Uuid,
    from_scope: &str,
    to_scope: &str,
    amount: u128,
    reference_kind: Option<&str>,
    reference_id: Option<&str>,
    metadata: Value,
) -> anyhow::Result<BalanceAccountRecord> {
    if amount == 0 {
        return Err(anyhow!("transfer amount must be greater than zero"));
    }
    let mut tx = pool
        .begin()
        .await
        .context("failed to open scope transfer transaction")?;
    ensure_balance_account_tx(&mut tx, player_id).await?;
    let snapshot = lock_balance_row(&mut tx, player_id).await?;
    let (next_available, next_vault) = match (from_scope, to_scope) {
        ("gambling", "vault") => {
            if snapshot.available < amount {
                return Err(anyhow!(
                    "insufficient gambling balance: have {}, need {}",
                    snapshot.available,
                    amount
                ));
            }
            (snapshot.available - amount, snapshot.vault + amount)
        }
        ("vault", "gambling") => {
            if snapshot.vault < amount {
                return Err(anyhow!(
                    "insufficient vault balance: have {}, need {}",
                    snapshot.vault,
                    amount
                ));
            }
            (snapshot.available + amount, snapshot.vault - amount)
        }
        _ => return Err(anyhow!("unsupported balance transfer direction")),
    };
    update_balance_row(
        &mut tx,
        player_id,
        next_available,
        snapshot.reserved,
        next_vault,
    )
    .await?;
    append_ledger_entry(
        &mut tx,
        player_id,
        from_scope,
        "balance_transfer_out",
        -(amount as i128),
        reference_kind,
        reference_id,
        metadata.clone(),
    )
    .await?;
    append_ledger_entry(
        &mut tx,
        player_id,
        to_scope,
        "balance_transfer_in",
        amount as i128,
        reference_kind,
        reference_id,
        metadata,
    )
    .await?;
    tx.commit()
        .await
        .context("failed to commit scope transfer transaction")?;
    get_balance_account(pool, player_id)
        .await?
        .ok_or_else(|| anyhow!("balance account missing after transfer"))
}

struct LockedBalanceRow {
    available: u128,
    reserved: u128,
    vault: u128,
}

async fn lock_balance_row(
    tx: &mut Transaction<'_, Postgres>,
    player_id: Uuid,
) -> anyhow::Result<LockedBalanceRow> {
    let row = sqlx::query(
        r#"
        SELECT gambling_balance, gambling_reserved, vault_balance
        FROM bankroll_accounts
        WHERE player_id = $1
        FOR UPDATE
        "#,
    )
    .bind(player_id)
    .fetch_one(&mut **tx)
    .await
    .context("failed to lock balance row")?;
    Ok(LockedBalanceRow {
        available: parse_u128_amount(
            &row.try_get::<String, _>("gambling_balance")
                .context("missing gambling balance")?,
            "gambling balance",
        )?,
        reserved: parse_u128_amount(
            &row.try_get::<String, _>("gambling_reserved")
                .context("missing gambling reserved")?,
            "gambling reserved",
        )?,
        vault: parse_u128_amount(
            &row.try_get::<String, _>("vault_balance")
                .context("missing vault balance")?,
            "vault balance",
        )?,
    })
}

async fn update_balance_row(
    tx: &mut Transaction<'_, Postgres>,
    player_id: Uuid,
    available: u128,
    reserved: u128,
    vault: u128,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        UPDATE bankroll_accounts
        SET gambling_balance = $2,
            gambling_reserved = $3,
            vault_balance = $4,
            public_balance = $2,
            reserved_balance = $3,
            updated_at = NOW()
        WHERE player_id = $1
        "#,
    )
    .bind(player_id)
    .bind(available.to_string())
    .bind(reserved.to_string())
    .bind(vault.to_string())
    .execute(&mut **tx)
    .await
    .context("failed to update balance row")?;
    Ok(())
}

async fn append_ledger_entry(
    tx: &mut Transaction<'_, Postgres>,
    player_id: Uuid,
    balance_scope: &str,
    entry_kind: &str,
    amount_delta: i128,
    reference_kind: Option<&str>,
    reference_id: Option<&str>,
    metadata: Value,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO balance_ledger_entries (
            id,
            player_id,
            balance_scope,
            entry_kind,
            amount_delta,
            reference_kind,
            reference_id,
            metadata
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8::jsonb)
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(player_id)
    .bind(balance_scope)
    .bind(entry_kind)
    .bind(amount_delta.to_string())
    .bind(reference_kind)
    .bind(reference_id)
    .bind(metadata.to_string())
    .execute(&mut **tx)
    .await
    .context("failed to append balance ledger entry")?;
    Ok(())
}

async fn fetch_active_reservation(
    tx: &mut Transaction<'_, Postgres>,
    player_id: Uuid,
    game_kind: &str,
    reference_id: &str,
) -> anyhow::Result<Option<BalanceReservationRecord>> {
    let row = sqlx::query(
        r#"
        SELECT
            id,
            player_id,
            game_kind,
            reference_id,
            amount,
            status,
            payout_amount,
            metadata,
            TO_CHAR(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS created_at,
            TO_CHAR(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS updated_at
        FROM balance_reservations
        WHERE player_id = $1
          AND game_kind = $2
          AND reference_id = $3
          AND status = 'active'
        FOR UPDATE
        "#,
    )
    .bind(player_id)
    .bind(game_kind)
    .bind(reference_id)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to fetch active balance reservation")?;
    row.map(hydrate_reservation).transpose()
}

async fn get_reservation_from_tx(
    tx: &mut Transaction<'_, Postgres>,
    reservation_id: Uuid,
) -> anyhow::Result<BalanceReservationRecord> {
    let row = sqlx::query(
        r#"
        SELECT
            id,
            player_id,
            game_kind,
            reference_id,
            amount,
            status,
            payout_amount,
            metadata,
            TO_CHAR(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS created_at,
            TO_CHAR(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS updated_at
        FROM balance_reservations
        WHERE id = $1
        "#,
    )
    .bind(reservation_id)
    .fetch_one(&mut **tx)
    .await
    .context("failed to fetch balance reservation")?;
    hydrate_reservation(row)
}

async fn get_balance_account_from_tx(
    tx: &mut Transaction<'_, Postgres>,
    player_id: Uuid,
) -> anyhow::Result<BalanceAccountRecord> {
    let row = sqlx::query(
        r#"
        SELECT
            player_id,
            gambling_balance,
            gambling_reserved,
            vault_balance,
            TO_CHAR(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS updated_at
        FROM bankroll_accounts
        WHERE player_id = $1
        "#,
    )
    .bind(player_id)
    .fetch_one(&mut **tx)
    .await
    .context("failed to fetch balance account after update")?;
    hydrate_balance_account(row)
}

fn parse_u128_amount(value: &str, field_name: &str) -> anyhow::Result<u128> {
    value
        .parse::<u128>()
        .with_context(|| format!("invalid {field_name}: {value}"))
}

fn checked_u128_delta(previous: u128, next: u128) -> anyhow::Result<i128> {
    if next >= previous {
        Ok(i128::try_from(next - previous)?)
    } else {
        Ok(-i128::try_from(previous - next)?)
    }
}

fn hydrate_balance_account(row: sqlx::postgres::PgRow) -> anyhow::Result<BalanceAccountRecord> {
    Ok(BalanceAccountRecord {
        user_id: row
            .try_get::<Uuid, _>("player_id")
            .context("missing balance player_id")?
            .to_string(),
        gambling_balance: row
            .try_get::<String, _>("gambling_balance")
            .context("missing gambling balance")?,
        gambling_reserved: row
            .try_get::<String, _>("gambling_reserved")
            .context("missing gambling reserved")?,
        vault_balance: row
            .try_get::<String, _>("vault_balance")
            .context("missing vault balance")?,
        updated_at: row
            .try_get::<String, _>("updated_at")
            .context("missing balance updated_at")?,
    })
}

fn hydrate_reservation(row: sqlx::postgres::PgRow) -> anyhow::Result<BalanceReservationRecord> {
    Ok(BalanceReservationRecord {
        reservation_id: row
            .try_get::<Uuid, _>("id")
            .context("missing reservation id")?
            .to_string(),
        user_id: row
            .try_get::<Uuid, _>("player_id")
            .context("missing reservation player_id")?
            .to_string(),
        game_kind: row
            .try_get::<String, _>("game_kind")
            .context("missing reservation game kind")?,
        reference_id: row
            .try_get::<String, _>("reference_id")
            .context("missing reservation reference id")?,
        amount: row
            .try_get::<String, _>("amount")
            .context("missing reservation amount")?,
        status: row
            .try_get::<String, _>("status")
            .context("missing reservation status")?,
        payout_amount: row.try_get("payout_amount").ok(),
        metadata: row
            .try_get::<Value, _>("metadata")
            .context("missing reservation metadata")?,
        created_at: row
            .try_get::<String, _>("created_at")
            .context("missing reservation created_at")?,
        updated_at: row
            .try_get::<String, _>("updated_at")
            .context("missing reservation updated_at")?,
    })
}
