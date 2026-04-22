use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConfig {
    pub environment: String,
    pub host: String,
    pub port: u16,
    pub service_name: String,
    pub database_url: Option<String>,
    pub redis_url: Option<String>,
    pub starknet_rpc_url: Option<String>,
    pub starknet_chain: String,
    pub starknet_account_address: Option<String>,
    pub starknet_private_key: Option<String>,
    pub bankroll_vault_address: Option<String>,
    pub table_registry_address: Option<String>,
    pub session_registry_address: Option<String>,
    pub dealer_commitment_address: Option<String>,
    pub deck_commitment_address: Option<String>,
    pub blackjack_table_address: Option<String>,
    pub dice_table_address: Option<String>,
    pub roulette_table_address: Option<String>,
    pub baccarat_table_address: Option<String>,
    pub strk_token_address: Option<String>,
    pub rewards_treasury_address: Option<String>,
}

impl ServiceConfig {
    pub fn from_env(service_name: &'static str, default_port: u16) -> Self {
        let environment = std::env::var("MOROS_ENV").unwrap_or_else(|_| "development".to_string());
        let host = std::env::var("MOROS_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
        let port = std::env::var("MOROS_PORT")
            .ok()
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(default_port);
        let database_url = std::env::var("MOROS_DATABASE_URL").ok();
        let redis_url = std::env::var("MOROS_REDIS_URL").ok();
        let starknet_chain =
            std::env::var("MOROS_STARKNET_CHAIN").unwrap_or_else(|_| "sepolia".to_string());
        let starknet_rpc_url = std::env::var("MOROS_STARKNET_RPC_URL")
            .ok()
            .or_else(|| default_rpc_for_chain(&starknet_chain));
        let starknet_account_address = std::env::var("MOROS_STARKNET_ACCOUNT_ADDRESS").ok();
        let starknet_private_key = std::env::var("MOROS_STARKNET_PRIVATE_KEY").ok();
        let bankroll_vault_address = std::env::var("MOROS_BANKROLL_VAULT_ADDRESS").ok();
        let table_registry_address = std::env::var("MOROS_TABLE_REGISTRY_ADDRESS").ok();
        let session_registry_address = std::env::var("MOROS_SESSION_REGISTRY_ADDRESS").ok();
        let dealer_commitment_address = std::env::var("MOROS_DEALER_COMMITMENT_ADDRESS").ok();
        let deck_commitment_address = std::env::var("MOROS_DECK_COMMITMENT_ADDRESS").ok();
        let blackjack_table_address = std::env::var("MOROS_BLACKJACK_TABLE_ADDRESS").ok();
        let dice_table_address = std::env::var("MOROS_DICE_TABLE_ADDRESS").ok();
        let roulette_table_address = std::env::var("MOROS_ROULETTE_TABLE_ADDRESS").ok();
        let baccarat_table_address = std::env::var("MOROS_BACCARAT_TABLE_ADDRESS").ok();
        let strk_token_address = std::env::var("MOROS_STRK_TOKEN_ADDRESS").ok();
        let rewards_treasury_address = std::env::var("MOROS_REWARDS_TREASURY_ADDRESS").ok();

        Self {
            environment,
            host,
            port,
            service_name: service_name.to_string(),
            database_url,
            redis_url,
            starknet_rpc_url,
            starknet_chain,
            starknet_account_address,
            starknet_private_key,
            bankroll_vault_address,
            table_registry_address,
            session_registry_address,
            dealer_commitment_address,
            deck_commitment_address,
            blackjack_table_address,
            dice_table_address,
            roulette_table_address,
            baccarat_table_address,
            strk_token_address,
            rewards_treasury_address,
        }
    }

    pub fn bind_address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

fn default_rpc_for_chain(chain: &str) -> Option<String> {
    match chain {
        "sepolia" => Some("https://starknet-sepolia.public.blastapi.io/rpc/v0_8".to_string()),
        "mainnet" => Some("https://starknet-mainnet.public.blastapi.io/rpc/v0_8".to_string()),
        _ => None,
    }
}
