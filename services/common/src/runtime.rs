use anyhow::Context;
use redis::{AsyncCommands, Client as RedisClient};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRuntimeState {
    pub session_id: String,
    pub hand_id: String,
    pub player: String,
    pub relay_token: String,
    pub table_id: u64,
    pub transcript_root: String,
    pub status: String,
    pub phase: String,
    pub allowed_actions: Vec<String>,
    pub expires_at_unix: i64,
}

pub async fn cache_session(redis: &RedisClient, state: &SessionRuntimeState) -> anyhow::Result<()> {
    let ttl_seconds = ttl_seconds(state.expires_at_unix);
    let payload = serde_json::to_string(state).context("failed to serialize session runtime")?;
    let session_key = session_runtime_key(&state.session_id);
    let hand_key = hand_runtime_key(&state.hand_id);
    let mut connection = redis
        .get_multiplexed_async_connection()
        .await
        .context("failed to open redis connection")?;

    connection
        .set_ex::<_, _, ()>(session_key, payload, ttl_seconds)
        .await
        .context("failed to write redis session runtime")?;
    connection
        .set_ex::<_, _, ()>(hand_key, state.session_id.clone(), ttl_seconds)
        .await
        .context("failed to write hand to session mapping")?;

    Ok(())
}

pub async fn get_session_by_hand(
    redis: &RedisClient,
    hand_id: Uuid,
) -> anyhow::Result<Option<SessionRuntimeState>> {
    let hand_key = hand_runtime_key(&hand_id.to_string());
    let mut connection = redis
        .get_multiplexed_async_connection()
        .await
        .context("failed to open redis connection")?;
    let session_id: Option<String> = connection
        .get(hand_key)
        .await
        .context("failed to read hand to session mapping")?;

    match session_id {
        Some(session_id) => get_session(redis, Uuid::parse_str(&session_id)?).await,
        None => Ok(None),
    }
}

pub async fn get_session(
    redis: &RedisClient,
    session_id: Uuid,
) -> anyhow::Result<Option<SessionRuntimeState>> {
    let session_key = session_runtime_key(&session_id.to_string());
    let mut connection = redis
        .get_multiplexed_async_connection()
        .await
        .context("failed to open redis connection")?;
    let payload: Option<String> = connection
        .get(session_key)
        .await
        .context("failed to read redis session runtime")?;

    payload
        .map(|payload| {
            serde_json::from_str::<SessionRuntimeState>(&payload)
                .context("failed to deserialize session runtime")
        })
        .transpose()
}

pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn ttl_seconds(expires_at_unix: i64) -> u64 {
    let now = now_unix();
    if expires_at_unix <= now {
        60
    } else {
        (expires_at_unix - now) as u64
    }
}

fn session_runtime_key(session_id: &str) -> String {
    format!("moros:session-runtime:{session_id}")
}

fn hand_runtime_key(hand_id: &str) -> String {
    format!("moros:hand-session:{hand_id}")
}
