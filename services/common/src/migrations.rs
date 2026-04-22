use anyhow::Context;
use sqlx::{PgPool, migrate::Migrator};

static MIGRATOR: Migrator = sqlx::migrate!("../migrations");

pub async fn run(pool: &PgPool) -> anyhow::Result<()> {
    MIGRATOR
        .run(pool)
        .await
        .context("failed to apply Moros SQL migrations")?;
    Ok(())
}
