use crate::{config::ServiceConfig, migrations};
use redis::Client as RedisClient;
use serde::Serialize;
use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use std::str::FromStr;

#[derive(Clone)]
pub struct ServiceInfra {
    pub database: Option<PgPool>,
    pub redis: Option<RedisClient>,
}

#[derive(Debug, Clone, Default)]
pub struct InfraReadiness {
    pub database_ready: bool,
    pub redis_ready: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct InfraSnapshot {
    pub environment: String,
    pub database_configured: bool,
    pub database_ready: bool,
    pub redis_configured: bool,
    pub redis_ready: bool,
    pub starknet_rpc_configured: bool,
}

impl ServiceInfra {
    pub fn from_config(config: &ServiceConfig) -> anyhow::Result<Self> {
        let database = match &config.database_url {
            Some(url) => {
                let options = PgConnectOptions::from_str(url)?;
                Some(
                    PgPoolOptions::new()
                        .max_connections(10)
                        .connect_lazy_with(options),
                )
            }
            None => None,
        };

        let redis = match &config.redis_url {
            Some(url) => Some(RedisClient::open(url.as_str())?),
            None => None,
        };

        Ok(Self { database, redis })
    }

    pub async fn prepare(&self) -> anyhow::Result<InfraReadiness> {
        let mut readiness = InfraReadiness::default();

        if let Some(database) = &self.database {
            migrations::run(database).await?;
            readiness.database_ready = true;
        }

        if let Some(redis) = &self.redis {
            let mut connection = redis.get_multiplexed_async_connection().await?;
            let _: String = redis::cmd("PING").query_async(&mut connection).await?;
            readiness.redis_ready = true;
        }

        Ok(readiness)
    }

    pub fn snapshot(&self, config: &ServiceConfig, readiness: InfraReadiness) -> InfraSnapshot {
        InfraSnapshot {
            environment: config.environment.clone(),
            database_configured: self.database.is_some(),
            database_ready: readiness.database_ready,
            redis_configured: self.redis.is_some(),
            redis_ready: readiness.redis_ready,
            starknet_rpc_configured: config.starknet_rpc_url.is_some(),
        }
    }
}
