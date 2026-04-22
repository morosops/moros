use crate::{
    blackjack::{BlackjackChainHand, BlackjackChainSeat, BlackjackOnchainCardRevealProof},
    config::ServiceConfig,
};
use anyhow::{Context, anyhow, bail};
use num_bigint::BigUint;
use serde::{Deserialize, Serialize};
use starknet::{
    accounts::{Account, ExecutionEncoding, SingleOwnerAccount},
    core::{
        chain_id,
        types::{
            BlockId, BlockTag, Call, EventFilter, EventsPage, ExecutionResult, Felt, FunctionCall,
        },
        utils::get_selector_from_name,
    },
    providers::{
        Provider, Url,
        jsonrpc::{HttpTransport, JsonRpcClient},
    },
    signers::{LocalWallet, SigningKey},
};
use starknet_crypto::poseidon_hash_many;
use std::{sync::Arc, time::Duration};

const HAND_POLL_ATTEMPTS: usize = 45;
const HAND_POLL_DELAY_MS: u64 = 900;
const REWARD_TX_POLL_ATTEMPTS: usize = 90;
const REWARD_TX_POLL_DELAY_MS: u64 = 1_000;
const RPC_READ_RETRY_ATTEMPTS: usize = 6;
const RPC_READ_RETRY_BASE_DELAY_MS: u64 = 150;
const ORIGINALS_SERVER_SEED_DOMAIN: &[u8] = b"MOROS_SERVER_SEED";
const BLACKJACK_TIMEOUT_BLOCKS: u64 = 50;

type StarknetProvider = JsonRpcClient<HttpTransport>;
type StarknetAccount = SingleOwnerAccount<StarknetProvider, LocalWallet>;

#[derive(Debug, Clone)]
pub struct ChainContracts {
    pub bankroll_vault: Felt,
    pub table_registry: Felt,
    pub session_registry: Felt,
    pub dealer_commitment: Felt,
    pub deck_commitment: Felt,
    pub blackjack_table: Felt,
    pub dice_table: Option<Felt>,
    pub roulette_table: Option<Felt>,
    pub baccarat_table: Option<Felt>,
    pub strk_token: Felt,
    pub rewards_treasury: Option<Felt>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionGrantView {
    pub player: String,
    pub session_key: String,
    pub max_wager: String,
    pub expires_at: u64,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableConfigView {
    pub table_id: u64,
    pub table_contract: String,
    pub game_kind: String,
    pub status: String,
    pub min_wager: String,
    pub max_wager: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlackjackTableState {
    pub table: TableConfigView,
    pub house_available: String,
    pub house_locked: String,
    pub recommended_house_bankroll: String,
    pub fully_covered_max_wager: String,
    pub player_balance: Option<String>,
    pub player_fully_covered_max_wager: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerVaultBalances {
    pub player: String,
    pub gambling_balance: String,
    pub vault_balance: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiceRoundView {
    pub round_id: u64,
    pub table_id: u64,
    pub player: String,
    pub wager: String,
    pub status: String,
    pub transcript_root: String,
    pub commitment_id: u64,
    pub server_seed_hash: String,
    pub client_seed: String,
    pub target_bps: u32,
    pub roll_over: bool,
    pub roll_bps: u32,
    pub chance_bps: u32,
    pub multiplier_bps: u32,
    pub payout: String,
    pub win: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiceCommitmentView {
    pub commitment_id: u64,
    pub server_seed_hash: String,
    pub reveal_deadline: u64,
    pub status: String,
    pub round_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouletteSpinView {
    pub spin_id: u64,
    pub table_id: u64,
    pub player: String,
    pub wager: String,
    pub status: String,
    pub transcript_root: String,
    pub commitment_id: u64,
    pub server_seed_hash: String,
    pub client_seed: String,
    pub result_number: u8,
    pub bet_count: u8,
    pub payout: String,
    pub bets: Vec<RouletteBetView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouletteBetView {
    pub kind: u8,
    pub selection: u8,
    pub amount: String,
    pub payout_multiplier: String,
    pub payout: String,
    pub win: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaccaratRoundView {
    pub round_id: u64,
    pub table_id: u64,
    pub player: String,
    pub wager: String,
    pub status: String,
    pub transcript_root: String,
    pub commitment_id: u64,
    pub server_seed_hash: String,
    pub client_seed: String,
    pub bet_side: u8,
    pub player_total: u8,
    pub banker_total: u8,
    pub player_card_count: u8,
    pub banker_card_count: u8,
    pub winner: u8,
    pub payout: String,
    pub player_cards: Vec<u8>,
    pub banker_cards: Vec<u8>,
    pub player_card_positions: Vec<u16>,
    pub banker_card_positions: Vec<u16>,
    pub player_card_draw_indices: Vec<u8>,
    pub banker_card_draw_indices: Vec<u8>,
    pub player_card_attempts: Vec<u8>,
    pub banker_card_attempts: Vec<u8>,
    pub player_card_commitments: Vec<String>,
    pub banker_card_commitments: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiceQuoteView {
    pub chance_bps: u32,
    pub multiplier_bps: u32,
    pub payout: String,
    pub exposure: String,
}

#[derive(Clone)]
pub struct ChainService {
    provider: StarknetProvider,
    account: Arc<StarknetAccount>,
    contracts: ChainContracts,
}

impl ChainService {
    fn is_retryable_read_error(error: &str) -> bool {
        let normalized = error.trim().to_ascii_lowercase();
        normalized.contains("code=429")
            || normalized.contains("compute units per second capacity")
            || normalized.contains("too many requests")
            || normalized.contains("eof while parsing a value")
            || normalized.contains("connection reset by peer")
            || normalized.contains("temporarily unavailable")
            || normalized.contains("timeout")
    }

    async fn provider_call_with_retry(
        &self,
        call: FunctionCall,
        context_label: &str,
    ) -> anyhow::Result<Vec<Felt>> {
        let mut last_error: Option<anyhow::Error> = None;

        for attempt in 0..RPC_READ_RETRY_ATTEMPTS {
            match self
                .provider
                .call(call.clone(), BlockId::Tag(BlockTag::Latest))
                .await
            {
                Ok(values) => return Ok(values),
                Err(error) => {
                    let rendered = error.to_string();
                    if attempt + 1 == RPC_READ_RETRY_ATTEMPTS
                        || !Self::is_retryable_read_error(&rendered)
                    {
                        return Err(error).with_context(|| context_label.to_string());
                    }

                    tracing::warn!(
                        attempt = attempt + 1,
                        max_attempts = RPC_READ_RETRY_ATTEMPTS,
                        context = context_label,
                        error = %rendered,
                        "retrying transient Starknet RPC read failure"
                    );
                    last_error = Some(anyhow!(rendered));
                    let delay_ms = RPC_READ_RETRY_BASE_DELAY_MS * (1_u64 << attempt);
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow!("Starknet RPC read failed")))
            .with_context(|| context_label.to_string())
    }

    pub fn from_config(config: &ServiceConfig) -> anyhow::Result<Option<Self>> {
        let Some(rpc_url) = &config.starknet_rpc_url else {
            return Ok(None);
        };
        let Some(account_address) = &config.starknet_account_address else {
            return Ok(None);
        };
        let Some(private_key) = &config.starknet_private_key else {
            return Ok(None);
        };
        let Some(bankroll_vault_address) = &config.bankroll_vault_address else {
            return Ok(None);
        };
        let Some(table_registry_address) = &config.table_registry_address else {
            return Ok(None);
        };
        let Some(session_registry_address) = &config.session_registry_address else {
            return Ok(None);
        };
        let Some(dealer_commitment_address) = &config.dealer_commitment_address else {
            return Ok(None);
        };
        let Some(deck_commitment_address) = &config.deck_commitment_address else {
            return Ok(None);
        };
        let Some(blackjack_table_address) = &config.blackjack_table_address else {
            return Ok(None);
        };
        let Some(strk_token_address) = &config.strk_token_address else {
            return Ok(None);
        };
        let rewards_treasury_address = config.rewards_treasury_address.as_deref();

        let provider = JsonRpcClient::new(HttpTransport::new(
            Url::parse(rpc_url).context("invalid MOROS_STARKNET_RPC_URL")?,
        ));
        let account_provider = JsonRpcClient::new(HttpTransport::new(
            Url::parse(rpc_url).context("invalid MOROS_STARKNET_RPC_URL")?,
        ));
        let signer = LocalWallet::from(SigningKey::from_secret_scalar(felt_from_hex(
            private_key,
            "MOROS_STARKNET_PRIVATE_KEY",
        )?));
        let account = SingleOwnerAccount::new(
            account_provider,
            signer,
            felt_from_hex(account_address, "MOROS_STARKNET_ACCOUNT_ADDRESS")?,
            if config.starknet_chain.eq_ignore_ascii_case("mainnet") {
                chain_id::MAINNET
            } else {
                chain_id::SEPOLIA
            },
            ExecutionEncoding::New,
        );

        Ok(Some(Self {
            provider,
            account: Arc::new(account),
            contracts: ChainContracts {
                bankroll_vault: felt_from_hex(
                    bankroll_vault_address,
                    "MOROS_BANKROLL_VAULT_ADDRESS",
                )?,
                table_registry: felt_from_hex(
                    table_registry_address,
                    "MOROS_TABLE_REGISTRY_ADDRESS",
                )?,
                session_registry: felt_from_hex(
                    session_registry_address,
                    "MOROS_SESSION_REGISTRY_ADDRESS",
                )?,
                dealer_commitment: felt_from_hex(
                    dealer_commitment_address,
                    "MOROS_DEALER_COMMITMENT_ADDRESS",
                )?,
                deck_commitment: felt_from_hex(
                    deck_commitment_address,
                    "MOROS_DECK_COMMITMENT_ADDRESS",
                )?,
                blackjack_table: felt_from_hex(
                    blackjack_table_address,
                    "MOROS_BLACKJACK_TABLE_ADDRESS",
                )?,
                dice_table: config
                    .dice_table_address
                    .as_deref()
                    .map(|value| felt_from_hex(value, "MOROS_DICE_TABLE_ADDRESS"))
                    .transpose()?,
                roulette_table: config
                    .roulette_table_address
                    .as_deref()
                    .map(|value| felt_from_hex(value, "MOROS_ROULETTE_TABLE_ADDRESS"))
                    .transpose()?,
                baccarat_table: config
                    .baccarat_table_address
                    .as_deref()
                    .map(|value| felt_from_hex(value, "MOROS_BACCARAT_TABLE_ADDRESS"))
                    .transpose()?,
                strk_token: felt_from_hex(strk_token_address, "MOROS_STRK_TOKEN_ADDRESS")?,
                rewards_treasury: rewards_treasury_address
                    .map(|value| felt_from_hex(value, "MOROS_REWARDS_TREASURY_ADDRESS"))
                    .transpose()?,
            },
        }))
    }

    pub async fn verify_message_signature(
        &self,
        account_address: Felt,
        message_hash: Felt,
        signature: &[Felt],
    ) -> anyhow::Result<bool> {
        let invalid_markers = [
            "argent/invalid-signature",
            "0x617267656e742f696e76616c69642d7369676e6174757265",
            "is invalid, with respect to the public key",
            "0x697320696e76616c6964",
            "INVALID_SIG",
            "0x494e56414c49445f534947",
        ];
        let mut last_error: Option<anyhow::Error> = None;

        for entrypoint in ["isValidSignature", "is_valid_signature"] {
            let mut calldata = Vec::with_capacity(signature.len() + 2);
            calldata.push(message_hash);
            calldata.push(Felt::from(signature.len() as u64));
            calldata.extend_from_slice(signature);

            let selector = get_selector_from_name(entrypoint)
                .with_context(|| format!("failed to resolve signature selector {entrypoint}"))?;

            match self
                .provider
                .call(
                    FunctionCall {
                        contract_address: account_address,
                        entry_point_selector: selector,
                        calldata,
                    },
                    BlockId::Tag(BlockTag::Latest),
                )
                .await
            {
                Ok(response) => {
                    if let Some(first) = response.first() {
                        let rendered = format!("{first:#x}");
                        if rendered == "0x0" || rendered == "0x00" {
                            return Ok(false);
                        }
                    }
                    return Ok(true);
                }
                Err(error) => {
                    let rendered = error.to_string();
                    if invalid_markers
                        .iter()
                        .any(|marker| rendered.contains(marker))
                    {
                        return Ok(false);
                    }
                    last_error = Some(anyhow!(
                        "signature verification via {entrypoint} failed: {rendered}"
                    ));
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow!("signature verification failed")))
    }

    pub fn contract_addresses(&self) -> &ChainContracts {
        &self.contracts
    }

    pub fn operator_address_hex(&self) -> String {
        format!("{:#x}", self.account.address())
    }

    pub async fn fetch_latest_block_number(&self) -> anyhow::Result<u64> {
        self.provider
            .block_number()
            .await
            .context("failed to fetch latest Starknet block number")
    }

    pub async fn fetch_events(
        &self,
        address: Felt,
        from_block: u64,
        to_block: u64,
        keys: Option<Vec<Vec<Felt>>>,
        continuation_token: Option<String>,
        chunk_size: u64,
    ) -> anyhow::Result<EventsPage> {
        self.provider
            .get_events(
                EventFilter {
                    from_block: Some(BlockId::Number(from_block)),
                    to_block: Some(BlockId::Number(to_block)),
                    address: Some(address),
                    keys,
                },
                continuation_token,
                chunk_size,
            )
            .await
            .with_context(|| {
                format!(
                    "failed to fetch Starknet events for {address:#x} from block {from_block} to {to_block}"
                )
            })
    }

    pub async fn fetch_session_grant(
        &self,
        player: &str,
        session_key: &str,
    ) -> anyhow::Result<SessionGrantView> {
        let values = self
            .call_contract(
                self.contracts.session_registry,
                "get_session",
                vec![
                    felt_from_hex(player, "player address")?,
                    felt_from_hex(session_key, "session key")?,
                ],
            )
            .await
            .context("failed to read session grant")?;
        Ok(SessionGrantView {
            player: format!(
                "{:#x}",
                values.first().context("session grant missing player")?
            ),
            session_key: format!(
                "{:#x}",
                values.get(1).context("session grant missing session_key")?
            ),
            max_wager: felt_to_u128(
                values.get(2).context("session grant missing max_wager")?,
                "session.max_wager",
            )?
            .to_string(),
            expires_at: felt_to_u64(
                values.get(3).context("session grant missing expires_at")?,
                "session.expires_at",
            )?,
            active: felt_to_bool(values.get(4).context("session grant missing active")?),
        })
    }

    pub async fn fetch_blackjack_table_state(
        &self,
        table_id: u64,
        player: Option<&str>,
    ) -> anyhow::Result<BlackjackTableState> {
        let table = self
            .call_contract(
                self.contracts.table_registry,
                "get_table",
                vec![Felt::from(table_id)],
            )
            .await
            .with_context(|| format!("failed to read table config for table {table_id}"))?;
        let game_kind = decode_game_kind(felt_to_u8(
            table.first().context("table config missing game_kind")?,
            "table.game_kind",
        )?)?;
        let table_contract = format!(
            "{:#x}",
            table
                .get(1)
                .context("table config missing table_contract")?
        );
        let min_wager = felt_to_u128(
            table.get(2).context("table config missing min_wager")?,
            "table.min_wager",
        )?;
        let max_wager = felt_to_u128(
            table.get(3).context("table config missing max_wager")?,
            "table.max_wager",
        )?;
        let status = decode_table_status(felt_to_u8(
            table.get(4).context("table config missing status")?,
            "table.status",
        )?)?;

        let effective_max_wager = self
            .fetch_effective_game_max_wager(game_kind, max_wager)
            .await
            .unwrap_or(max_wager);

        let house_available = felt_to_u128(
            self.call_contract(self.contracts.bankroll_vault, "house_available", vec![])
                .await?
                .first()
                .context("house_available returned no value")?,
            "house_available",
        )?;
        let house_locked = felt_to_u128(
            self.call_contract(self.contracts.bankroll_vault, "house_locked", vec![])
                .await?
                .first()
                .context("house_locked returned no value")?,
            "house_locked",
        )?;
        let (house_exposure_factor, player_exposure_factor) = match game_kind {
            // Vegas Strip blackjack can reach 4 hands after 3 splits, and each split hand can
            // double. Ace-upcard openings can also temporarily lock insurance exposure, so the
            // UI uses the conservative 9x lock factor.
            "blackjack" => (9, 8),
            "roulette" => (36, 1),
            "baccarat" => (9, 1),
            "dice" => (99, 1),
            _ => (4, 1),
        };
        let dynamic_exposure_divisor = 100;
        let recommended_house_bankroll = effective_max_wager
            .saturating_mul(house_exposure_factor)
            .saturating_mul(dynamic_exposure_divisor);
        let fully_covered_max_wager =
            house_available / house_exposure_factor / dynamic_exposure_divisor;

        let player_balance = match player {
            Some(player) => Some(felt_to_u128(
                self.call_contract(
                    self.contracts.bankroll_vault,
                    "balance_of",
                    vec![felt_from_hex(player, "player address")?],
                )
                .await?
                .first()
                .context("balance_of returned no value")?,
                "balance_of",
            )?),
            None => None,
        };

        Ok(BlackjackTableState {
            table: TableConfigView {
                table_id,
                table_contract,
                game_kind: game_kind.to_string(),
                status: status.to_string(),
                min_wager: min_wager.to_string(),
                max_wager: effective_max_wager.to_string(),
            },
            house_available: house_available.to_string(),
            house_locked: house_locked.to_string(),
            recommended_house_bankroll: recommended_house_bankroll.to_string(),
            fully_covered_max_wager: fully_covered_max_wager.min(effective_max_wager).to_string(),
            player_balance: player_balance.map(|value| value.to_string()),
            player_fully_covered_max_wager: player_balance.map(|value| {
                (value / player_exposure_factor)
                    .min(effective_max_wager)
                    .to_string()
            }),
        })
    }

    async fn fetch_effective_game_max_wager(
        &self,
        game_kind: &str,
        table_max_wager: u128,
    ) -> anyhow::Result<u128> {
        let contract_address = match game_kind {
            "blackjack" => self.contracts.blackjack_table,
            "dice" => self
                .contracts
                .dice_table
                .context("dice table address is not configured")?,
            "baccarat" => self
                .contracts
                .baccarat_table
                .context("baccarat table address is not configured")?,
            _ => return Ok(table_max_wager),
        };
        let configured_cap = felt_to_u128(
            self.call_contract(contract_address, "get_wager_cap", vec![])
                .await?
                .first()
                .context("get_wager_cap returned no value")?,
            "game_wager_cap",
        )?;
        Ok(table_max_wager.min(configured_cap))
    }

    pub async fn fetch_player_vault_balances(
        &self,
        player: &str,
    ) -> anyhow::Result<PlayerVaultBalances> {
        let player_felt = felt_from_hex(player, "player address")?;
        let gambling_balance = felt_to_u128(
            self.call_contract(
                self.contracts.bankroll_vault,
                "gambling_balance_of",
                vec![player_felt],
            )
            .await?
            .first()
            .context("gambling_balance_of returned no value")?,
            "gambling_balance_of",
        )?;
        let vault_balance = felt_to_u128(
            self.call_contract(
                self.contracts.bankroll_vault,
                "vault_balance_of",
                vec![player_felt],
            )
            .await?
            .first()
            .context("vault_balance_of returned no value")?,
            "vault_balance_of",
        )?;

        Ok(PlayerVaultBalances {
            player: format!("{player_felt:#x}"),
            gambling_balance: gambling_balance.to_string(),
            vault_balance: vault_balance.to_string(),
        })
    }

    pub async fn credit_rewards_to_vault(
        &self,
        player: &str,
        amount: u128,
    ) -> anyhow::Result<String> {
        if amount == 0 {
            bail!("reward credit amount must be greater than zero");
        }

        let player_felt = felt_from_hex(player, "player address")?;
        let rewards_treasury = self
            .contracts
            .rewards_treasury
            .context("MOROS_REWARDS_TREASURY_ADDRESS is required for reward credits")?;
        let credit_selector = get_selector_from_name("credit_to_vault")
            .context("missing selector for credit_to_vault")?;
        let result = self
            .account
            .execute_v3(vec![Call {
                to: rewards_treasury,
                selector: credit_selector,
                calldata: vec![player_felt, Felt::from(amount)],
            }])
            .send()
            .await
            .context("failed to submit reward treasury credit")?;
        Ok(format!("{:#x}", result.transaction_hash))
    }

    pub async fn wait_for_transaction_success(&self, tx_hash: &str) -> anyhow::Result<()> {
        let tx_hash_felt = felt_from_hex(tx_hash, "transaction hash")?;
        let mut last_error = None;
        for attempt in 0..REWARD_TX_POLL_ATTEMPTS {
            match self.provider.get_transaction_receipt(tx_hash_felt).await {
                Ok(receipt) => match receipt.receipt.execution_result() {
                    ExecutionResult::Succeeded => return Ok(()),
                    ExecutionResult::Reverted { reason } => {
                        bail!("reward credit transaction reverted: {reason}");
                    }
                },
                Err(error) => {
                    last_error = Some(error.to_string());
                    if attempt + 1 == REWARD_TX_POLL_ATTEMPTS {
                        break;
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(REWARD_TX_POLL_DELAY_MS)).await;
        }
        bail!(
            "reward credit transaction was not confirmed before timeout: {}",
            last_error.unwrap_or_else(|| tx_hash.to_string())
        )
    }

    pub async fn peek_next_hand_id(&self) -> anyhow::Result<u64> {
        let values = self
            .call_contract(self.contracts.blackjack_table, "peek_next_hand_id", vec![])
            .await
            .context("failed to read blackjack next_hand_id")?;
        let value = values
            .first()
            .context("peek_next_hand_id returned no value")?;
        felt_to_u64(value, "peek_next_hand_id")
    }

    pub async fn quote_dice_round(
        &self,
        wager: u128,
        target_bps: u32,
        roll_over: bool,
    ) -> anyhow::Result<DiceQuoteView> {
        let dice_table = self.dice_table()?;
        let values = self
            .call_contract(
                dice_table,
                "quote_payout",
                vec![
                    Felt::from(wager),
                    Felt::from(target_bps),
                    Felt::from(u8::from(roll_over)),
                ],
            )
            .await
            .context("failed to quote dice payout")?;
        Ok(DiceQuoteView {
            chance_bps: felt_to_u32(
                values.first().context("dice quote missing chance")?,
                "chance_bps",
            )?,
            multiplier_bps: felt_to_u32(
                values.get(1).context("dice quote missing multiplier")?,
                "multiplier_bps",
            )?,
            payout: felt_to_u128(
                values.get(2).context("dice quote missing payout")?,
                "payout",
            )?
            .to_string(),
            exposure: felt_to_u128(
                values.get(3).context("dice quote missing exposure")?,
                "exposure",
            )?
            .to_string(),
        })
    }

    pub async fn peek_next_dice_round_id(&self) -> anyhow::Result<u64> {
        let dice_table = self.dice_table()?;
        let values = self
            .call_contract(dice_table, "peek_next_round_id", vec![])
            .await
            .context("failed to read dice next_round_id")?;
        let value = values
            .first()
            .context("peek_next_round_id returned no value")?;
        felt_to_u64(value, "peek_next_round_id")
    }

    pub fn hash_server_seed_commitment(&self, server_seed: Felt) -> Felt {
        poseidon_hash_many(&[
            Felt::from_bytes_be_slice(ORIGINALS_SERVER_SEED_DOMAIN),
            server_seed,
        ])
    }

    pub async fn peek_next_dice_commitment_id(&self) -> anyhow::Result<u64> {
        let dice_table = self.dice_table()?;
        let values = self
            .call_contract(dice_table, "peek_next_commitment_id", vec![])
            .await
            .context("failed to read dice next_commitment_id")?;
        let value = values
            .first()
            .context("peek_next_commitment_id returned no value")?;
        felt_to_u64(value, "peek_next_commitment_id")
    }

    pub async fn commit_dice_server_seed(
        &self,
        server_seed_hash: Felt,
        reveal_deadline: u64,
    ) -> anyhow::Result<(u64, String)> {
        let dice_table = self.dice_table()?;
        let expected_commitment_id = self.peek_next_dice_commitment_id().await?;
        let tx_hash = self
            .invoke(
                dice_table,
                "commit_server_seed",
                vec![server_seed_hash, Felt::from(reveal_deadline)],
            )
            .await?;
        self.wait_for_dice_commitment(expected_commitment_id)
            .await?;
        Ok((expected_commitment_id, format!("{tx_hash:#x}")))
    }

    pub async fn fetch_dice_commitment(
        &self,
        commitment_id: u64,
    ) -> anyhow::Result<DiceCommitmentView> {
        let dice_table = self.dice_table()?;
        let values = self
            .call_contract(
                dice_table,
                "get_commitment",
                vec![Felt::from(commitment_id)],
            )
            .await
            .with_context(|| format!("failed to read dice commitment {commitment_id}"))?;
        Ok(DiceCommitmentView {
            commitment_id: felt_to_u64(
                values
                    .first()
                    .context("dice commitment missing commitment_id")?,
                "commitment_id",
            )?,
            server_seed_hash: format!(
                "{:#x}",
                values
                    .get(1)
                    .context("dice commitment missing server_seed_hash")?
            ),
            reveal_deadline: felt_to_u64(
                values
                    .get(2)
                    .context("dice commitment missing reveal_deadline")?,
                "reveal_deadline",
            )?,
            status: decode_dice_commitment_status(felt_to_u8(
                values.get(3).context("dice commitment missing status")?,
                "dice_commitment.status",
            )?)?
            .to_string(),
            round_id: felt_to_u64(
                values.get(4).context("dice commitment missing round_id")?,
                "dice_commitment.round_id",
            )?,
        })
    }

    pub async fn wait_for_dice_commitment(
        &self,
        commitment_id: u64,
    ) -> anyhow::Result<DiceCommitmentView> {
        let mut last_error = None;
        for _ in 0..HAND_POLL_ATTEMPTS {
            match self.fetch_dice_commitment(commitment_id).await {
                Ok(commitment) => return Ok(commitment),
                Err(error) => last_error = Some(error),
            }
            tokio::time::sleep(Duration::from_millis(HAND_POLL_DELAY_MS)).await;
        }

        if let Some(error) = last_error {
            return Err(error).context("timed out waiting for dice commitment");
        }
        bail!("timed out waiting for dice commitment");
    }

    pub async fn fetch_dice_round_for_commitment(&self, commitment_id: u64) -> anyhow::Result<u64> {
        let dice_table = self.dice_table()?;
        let values = self
            .call_contract(
                dice_table,
                "get_round_for_commitment",
                vec![Felt::from(commitment_id)],
            )
            .await
            .with_context(|| format!("failed to read dice round for commitment {commitment_id}"))?;
        felt_to_u64(
            values
                .first()
                .context("get_round_for_commitment returned no value")?,
            "round_for_commitment",
        )
    }

    pub async fn open_dice_round(
        &self,
        player: &str,
        session_key: &str,
        table_id: u64,
        wager: u128,
        target_bps: u32,
        roll_over: bool,
        client_seed: Felt,
        commitment_id: u64,
    ) -> anyhow::Result<(u64, String)> {
        let dice_table = self.dice_table()?;
        let expected_round_id = self.peek_next_dice_round_id().await?;
        let tx_hash = self
            .invoke(
                dice_table,
                "open_round",
                vec![
                    Felt::from(table_id),
                    felt_from_hex(player, "player address")?,
                    felt_from_hex(session_key, "session key")?,
                    Felt::from(wager),
                    Felt::from(target_bps),
                    Felt::from(u8::from(roll_over)),
                    client_seed,
                    Felt::from(commitment_id),
                ],
            )
            .await?;
        self.wait_for_dice_round(expected_round_id).await?;
        Ok((expected_round_id, format!("{tx_hash:#x}")))
    }

    pub async fn peek_next_roulette_spin_id(&self) -> anyhow::Result<u64> {
        let roulette_table = self.roulette_table()?;
        let values = self
            .call_contract(roulette_table, "peek_next_spin_id", vec![])
            .await
            .context("failed to read roulette next_spin_id")?;
        felt_to_u64(
            values
                .first()
                .context("peek_next_spin_id returned no value")?,
            "roulette.peek_next_spin_id",
        )
    }

    pub async fn open_roulette_spin(
        &self,
        player: &str,
        session_key: &str,
        table_id: u64,
        total_wager: u128,
        client_seed: Felt,
        commitment_id: u64,
        bets: Vec<(u8, u8, u128)>,
    ) -> anyhow::Result<(u64, String)> {
        let roulette_table = self.roulette_table()?;
        let expected_spin_id = self.peek_next_roulette_spin_id().await?;
        let padded = (0..8)
            .map(|index| bets.get(index).copied().unwrap_or((0, 0, 0)))
            .collect::<Vec<_>>();
        let mut calldata = vec![
            Felt::from(table_id),
            felt_from_hex(player, "player address")?,
            felt_from_hex(session_key, "session key")?,
            Felt::from(total_wager),
            client_seed,
            Felt::from(commitment_id),
            Felt::from(u8::try_from(bets.len()).context("roulette bet count exceeds u8")?),
        ];
        for (kind, selection, amount) in padded {
            calldata.extend([Felt::from(kind), Felt::from(selection), Felt::from(amount)]);
        }
        let tx_hash = self.invoke(roulette_table, "open_spin", calldata).await?;
        self.wait_for_roulette_spin_status(expected_spin_id, "active")
            .await?;
        Ok((expected_spin_id, format!("{tx_hash:#x}")))
    }

    pub async fn settle_dice_round(
        &self,
        round_id: u64,
        server_seed: Felt,
    ) -> anyhow::Result<(DiceRoundView, String)> {
        let dice_table = self.dice_table()?;
        let tx_hash = self
            .invoke(
                dice_table,
                "settle_round",
                vec![Felt::from(round_id), server_seed],
            )
            .await?;
        let round = self.wait_for_dice_round_status(round_id, "settled").await?;
        Ok((round, format!("{tx_hash:#x}")))
    }

    pub async fn fetch_dice_round(&self, round_id: u64) -> anyhow::Result<DiceRoundView> {
        let dice_table = self.dice_table()?;
        let values = self
            .call_contract(dice_table, "get_round", vec![Felt::from(round_id)])
            .await
            .with_context(|| format!("failed to read dice round {round_id}"))?;

        Ok(DiceRoundView {
            round_id: felt_to_u64(
                values.first().context("dice round missing round_id")?,
                "round_id",
            )?,
            table_id: felt_to_u64(
                values.get(1).context("dice round missing table_id")?,
                "table_id",
            )?,
            player: format!("{:#x}", values.get(2).context("dice round missing player")?),
            wager: felt_to_u128(values.get(3).context("dice round missing wager")?, "wager")?
                .to_string(),
            status: decode_hand_status(felt_to_u8(
                values.get(4).context("dice round missing status")?,
                "status",
            )?)?
            .to_string(),
            transcript_root: format!(
                "{:#x}",
                values
                    .get(5)
                    .context("dice round missing transcript_root")?
            ),
            commitment_id: felt_to_u64(
                values.get(6).context("dice round missing commitment_id")?,
                "commitment_id",
            )?,
            server_seed_hash: format!(
                "{:#x}",
                values
                    .get(7)
                    .context("dice round missing server_seed_hash")?
            ),
            client_seed: format!(
                "{:#x}",
                values.get(8).context("dice round missing client_seed")?
            ),
            target_bps: felt_to_u32(
                values.get(9).context("dice round missing target_bps")?,
                "target_bps",
            )?,
            roll_over: felt_to_bool(values.get(10).context("dice round missing roll_over")?),
            roll_bps: felt_to_u32(
                values.get(11).context("dice round missing roll_bps")?,
                "roll_bps",
            )?,
            chance_bps: felt_to_u32(
                values.get(12).context("dice round missing chance_bps")?,
                "chance_bps",
            )?,
            multiplier_bps: felt_to_u32(
                values
                    .get(13)
                    .context("dice round missing multiplier_bps")?,
                "multiplier_bps",
            )?,
            payout: felt_to_u128(
                values.get(14).context("dice round missing payout")?,
                "payout",
            )?
            .to_string(),
            win: felt_to_bool(values.get(15).context("dice round missing win")?),
        })
    }

    pub async fn wait_for_dice_round(&self, round_id: u64) -> anyhow::Result<DiceRoundView> {
        let mut last_error = None;
        for _ in 0..HAND_POLL_ATTEMPTS {
            match self.fetch_dice_round(round_id).await {
                Ok(round) => return Ok(round),
                Err(error) => last_error = Some(error),
            }
            tokio::time::sleep(Duration::from_millis(HAND_POLL_DELAY_MS)).await;
        }

        if let Some(error) = last_error {
            return Err(error).context("timed out waiting for dice round");
        }
        bail!("timed out waiting for dice round");
    }

    pub async fn wait_for_dice_round_status(
        &self,
        round_id: u64,
        expected_status: &str,
    ) -> anyhow::Result<DiceRoundView> {
        let mut last_round = None;
        let mut last_error = None;
        for _ in 0..HAND_POLL_ATTEMPTS {
            match self.fetch_dice_round(round_id).await {
                Ok(round) if round.status == expected_status => return Ok(round),
                Ok(round) => last_round = Some(round),
                Err(error) => last_error = Some(error),
            }
            tokio::time::sleep(Duration::from_millis(HAND_POLL_DELAY_MS)).await;
        }

        if let Some(round) = last_round {
            bail!(
                "timed out waiting for dice round {round_id} to reach {expected_status}; latest status {}",
                round.status
            );
        }
        if let Some(error) = last_error {
            return Err(error).context("timed out waiting for dice round status");
        }
        bail!("timed out waiting for dice round status");
    }

    pub async fn peek_next_roulette_commitment_id(&self) -> anyhow::Result<u64> {
        let roulette_table = self.roulette_table()?;
        let values = self
            .call_contract(roulette_table, "peek_next_commitment_id", vec![])
            .await
            .context("failed to read roulette next_commitment_id")?;
        felt_to_u64(
            values
                .first()
                .context("peek_next_commitment_id returned no value")?,
            "roulette.peek_next_commitment_id",
        )
    }

    pub async fn commit_roulette_server_seed(
        &self,
        server_seed_hash: Felt,
        reveal_deadline: u64,
    ) -> anyhow::Result<(u64, String)> {
        let roulette_table = self.roulette_table()?;
        let expected_commitment_id = self.peek_next_roulette_commitment_id().await?;
        let tx_hash = self
            .invoke(
                roulette_table,
                "commit_server_seed",
                vec![server_seed_hash, Felt::from(reveal_deadline)],
            )
            .await?;
        self.wait_for_roulette_commitment(expected_commitment_id)
            .await?;
        Ok((expected_commitment_id, format!("{tx_hash:#x}")))
    }

    pub async fn fetch_roulette_commitment(
        &self,
        commitment_id: u64,
    ) -> anyhow::Result<DiceCommitmentView> {
        let roulette_table = self.roulette_table()?;
        let values = self
            .call_contract(
                roulette_table,
                "get_commitment",
                vec![Felt::from(commitment_id)],
            )
            .await
            .with_context(|| format!("failed to read roulette commitment {commitment_id}"))?;
        Ok(DiceCommitmentView {
            commitment_id: felt_to_u64(
                values
                    .first()
                    .context("roulette commitment missing commitment_id")?,
                "roulette.commitment_id",
            )?,
            server_seed_hash: format!(
                "{:#x}",
                values
                    .get(1)
                    .context("roulette commitment missing server_seed_hash")?
            ),
            reveal_deadline: felt_to_u64(
                values
                    .get(2)
                    .context("roulette commitment missing reveal_deadline")?,
                "roulette.reveal_deadline",
            )?,
            status: decode_dice_commitment_status(felt_to_u8(
                values
                    .get(3)
                    .context("roulette commitment missing status")?,
                "roulette_commitment.status",
            )?)?
            .to_string(),
            round_id: felt_to_u64(
                values
                    .get(4)
                    .context("roulette commitment missing round_id")?,
                "roulette_commitment.round_id",
            )?,
        })
    }

    pub async fn wait_for_roulette_commitment(
        &self,
        commitment_id: u64,
    ) -> anyhow::Result<DiceCommitmentView> {
        let mut last_error = None;
        for _ in 0..HAND_POLL_ATTEMPTS {
            match self.fetch_roulette_commitment(commitment_id).await {
                Ok(commitment) => return Ok(commitment),
                Err(error) => last_error = Some(error),
            }
            tokio::time::sleep(Duration::from_millis(HAND_POLL_DELAY_MS)).await;
        }
        if let Some(error) = last_error {
            return Err(error).context("timed out waiting for roulette commitment");
        }
        bail!("timed out waiting for roulette commitment");
    }

    pub async fn fetch_roulette_spin_for_commitment(
        &self,
        commitment_id: u64,
    ) -> anyhow::Result<u64> {
        let roulette_table = self.roulette_table()?;
        let values = self
            .call_contract(
                roulette_table,
                "get_spin_for_commitment",
                vec![Felt::from(commitment_id)],
            )
            .await
            .with_context(|| {
                format!("failed to read roulette spin for commitment {commitment_id}")
            })?;
        felt_to_u64(
            values
                .first()
                .context("get_spin_for_commitment returned no value")?,
            "spin_for_commitment",
        )
    }

    pub async fn settle_roulette_spin(
        &self,
        spin_id: u64,
        server_seed: Felt,
    ) -> anyhow::Result<(RouletteSpinView, String)> {
        let roulette_table = self.roulette_table()?;
        let tx_hash = self
            .invoke(
                roulette_table,
                "settle_spin",
                vec![Felt::from(spin_id), server_seed],
            )
            .await?;
        let spin = self
            .wait_for_roulette_spin_status(spin_id, "settled")
            .await?;
        Ok((spin, format!("{tx_hash:#x}")))
    }

    pub async fn fetch_roulette_spin(&self, spin_id: u64) -> anyhow::Result<RouletteSpinView> {
        let roulette_table = self.roulette_table()?;
        let values = self
            .call_contract(roulette_table, "get_spin", vec![Felt::from(spin_id)])
            .await
            .with_context(|| format!("failed to read roulette spin {spin_id}"))?;
        let bet_count = felt_to_u8(
            values.get(10).context("roulette spin missing bet_count")?,
            "roulette.bet_count",
        )?;
        let mut bets = Vec::new();
        for index in 0..bet_count {
            bets.push(self.fetch_roulette_bet(spin_id, index).await?);
        }
        Ok(RouletteSpinView {
            spin_id: felt_to_u64(
                values.first().context("roulette spin missing spin_id")?,
                "roulette.spin_id",
            )?,
            table_id: felt_to_u64(
                values.get(1).context("roulette spin missing table_id")?,
                "roulette.table_id",
            )?,
            player: format!(
                "{:#x}",
                values.get(2).context("roulette spin missing player")?
            ),
            wager: felt_to_u128(
                values.get(3).context("roulette spin missing wager")?,
                "wager",
            )?
            .to_string(),
            status: decode_hand_status(felt_to_u8(
                values.get(4).context("roulette spin missing status")?,
                "roulette.status",
            )?)?
            .to_string(),
            transcript_root: format!(
                "{:#x}",
                values
                    .get(5)
                    .context("roulette spin missing transcript_root")?
            ),
            commitment_id: felt_to_u64(
                values
                    .get(6)
                    .context("roulette spin missing commitment_id")?,
                "roulette.commitment_id",
            )?,
            server_seed_hash: format!(
                "{:#x}",
                values
                    .get(7)
                    .context("roulette spin missing server_seed_hash")?
            ),
            client_seed: format!(
                "{:#x}",
                values.get(8).context("roulette spin missing client_seed")?
            ),
            result_number: felt_to_u8(
                values
                    .get(9)
                    .context("roulette spin missing result_number")?,
                "roulette.result_number",
            )?,
            bet_count,
            payout: felt_to_u128(
                values.get(11).context("roulette spin missing payout")?,
                "roulette.payout",
            )?
            .to_string(),
            bets,
        })
    }

    async fn fetch_roulette_bet(
        &self,
        spin_id: u64,
        bet_index: u8,
    ) -> anyhow::Result<RouletteBetView> {
        let roulette_table = self.roulette_table()?;
        let values = self
            .call_contract(
                roulette_table,
                "get_bet",
                vec![Felt::from(spin_id), Felt::from(bet_index)],
            )
            .await
            .with_context(|| {
                format!("failed to read roulette bet {bet_index} for spin {spin_id}")
            })?;
        Ok(RouletteBetView {
            kind: felt_to_u8(
                values.first().context("roulette bet missing kind")?,
                "bet.kind",
            )?,
            selection: felt_to_u8(
                values.get(1).context("roulette bet missing selection")?,
                "bet.selection",
            )?,
            amount: felt_to_u128(
                values.get(2).context("roulette bet missing amount")?,
                "bet.amount",
            )?
            .to_string(),
            payout_multiplier: felt_to_u128(
                values
                    .get(3)
                    .context("roulette bet missing payout_multiplier")?,
                "bet.payout_multiplier",
            )?
            .to_string(),
            payout: felt_to_u128(
                values.get(4).context("roulette bet missing payout")?,
                "bet.payout",
            )?
            .to_string(),
            win: felt_to_bool(values.get(5).context("roulette bet missing win")?),
        })
    }

    pub async fn wait_for_roulette_spin_status(
        &self,
        spin_id: u64,
        expected_status: &str,
    ) -> anyhow::Result<RouletteSpinView> {
        let mut last_spin = None;
        let mut last_error = None;
        for _ in 0..HAND_POLL_ATTEMPTS {
            match self.fetch_roulette_spin(spin_id).await {
                Ok(spin) if spin.status == expected_status => return Ok(spin),
                Ok(spin) => last_spin = Some(spin),
                Err(error) => last_error = Some(error),
            }
            tokio::time::sleep(Duration::from_millis(HAND_POLL_DELAY_MS)).await;
        }
        if let Some(spin) = last_spin {
            bail!(
                "timed out waiting for roulette spin {spin_id} to reach {expected_status}; latest status {}",
                spin.status
            );
        }
        if let Some(error) = last_error {
            return Err(error).context("timed out waiting for roulette spin status");
        }
        bail!("timed out waiting for roulette spin status");
    }

    pub async fn peek_next_baccarat_commitment_id(&self) -> anyhow::Result<u64> {
        let baccarat_table = self.baccarat_table()?;
        let values = self
            .call_contract(baccarat_table, "peek_next_commitment_id", vec![])
            .await
            .context("failed to read baccarat next_commitment_id")?;
        felt_to_u64(
            values
                .first()
                .context("peek_next_commitment_id returned no value")?,
            "baccarat.peek_next_commitment_id",
        )
    }

    pub async fn commit_baccarat_server_seed(
        &self,
        server_seed_hash: Felt,
        reveal_deadline: u64,
    ) -> anyhow::Result<(u64, String)> {
        let baccarat_table = self.baccarat_table()?;
        let expected_commitment_id = self.peek_next_baccarat_commitment_id().await?;
        let tx_hash = self
            .invoke(
                baccarat_table,
                "commit_server_seed",
                vec![server_seed_hash, Felt::from(reveal_deadline)],
            )
            .await?;
        self.wait_for_baccarat_commitment(expected_commitment_id)
            .await?;
        Ok((expected_commitment_id, format!("{tx_hash:#x}")))
    }

    pub async fn fetch_baccarat_commitment(
        &self,
        commitment_id: u64,
    ) -> anyhow::Result<DiceCommitmentView> {
        let baccarat_table = self.baccarat_table()?;
        let values = self
            .call_contract(
                baccarat_table,
                "get_commitment",
                vec![Felt::from(commitment_id)],
            )
            .await
            .with_context(|| format!("failed to read baccarat commitment {commitment_id}"))?;
        Ok(DiceCommitmentView {
            commitment_id: felt_to_u64(
                values
                    .first()
                    .context("baccarat commitment missing commitment_id")?,
                "baccarat.commitment_id",
            )?,
            server_seed_hash: format!(
                "{:#x}",
                values
                    .get(1)
                    .context("baccarat commitment missing server_seed_hash")?
            ),
            reveal_deadline: felt_to_u64(
                values
                    .get(2)
                    .context("baccarat commitment missing reveal_deadline")?,
                "baccarat.reveal_deadline",
            )?,
            status: decode_dice_commitment_status(felt_to_u8(
                values
                    .get(3)
                    .context("baccarat commitment missing status")?,
                "baccarat_commitment.status",
            )?)?
            .to_string(),
            round_id: felt_to_u64(
                values
                    .get(4)
                    .context("baccarat commitment missing round_id")?,
                "baccarat_commitment.round_id",
            )?,
        })
    }

    pub async fn wait_for_baccarat_commitment(
        &self,
        commitment_id: u64,
    ) -> anyhow::Result<DiceCommitmentView> {
        let mut last_error = None;
        for _ in 0..HAND_POLL_ATTEMPTS {
            match self.fetch_baccarat_commitment(commitment_id).await {
                Ok(commitment) => return Ok(commitment),
                Err(error) => last_error = Some(error),
            }
            tokio::time::sleep(Duration::from_millis(HAND_POLL_DELAY_MS)).await;
        }
        if let Some(error) = last_error {
            return Err(error).context("timed out waiting for baccarat commitment");
        }
        bail!("timed out waiting for baccarat commitment");
    }

    pub async fn fetch_baccarat_round_for_commitment(
        &self,
        commitment_id: u64,
    ) -> anyhow::Result<u64> {
        let baccarat_table = self.baccarat_table()?;
        let values = self
            .call_contract(
                baccarat_table,
                "get_round_for_commitment",
                vec![Felt::from(commitment_id)],
            )
            .await
            .with_context(|| {
                format!("failed to read baccarat round for commitment {commitment_id}")
            })?;
        felt_to_u64(
            values
                .first()
                .context("get_round_for_commitment returned no value")?,
            "baccarat_round_for_commitment",
        )
    }

    pub async fn peek_next_baccarat_round_id(&self) -> anyhow::Result<u64> {
        let baccarat_table = self.baccarat_table()?;
        let values = self
            .call_contract(baccarat_table, "peek_next_round_id", vec![])
            .await
            .context("failed to read baccarat next_round_id")?;
        felt_to_u64(
            values
                .first()
                .context("peek_next_round_id returned no value")?,
            "baccarat.peek_next_round_id",
        )
    }

    pub async fn open_baccarat_round(
        &self,
        player: &str,
        session_key: &str,
        table_id: u64,
        wager: u128,
        bet_side: u8,
        client_seed: Felt,
        commitment_id: u64,
    ) -> anyhow::Result<(u64, String)> {
        let baccarat_table = self.baccarat_table()?;
        let expected_round_id = self.peek_next_baccarat_round_id().await?;
        let tx_hash = self
            .invoke(
                baccarat_table,
                "open_round",
                vec![
                    Felt::from(table_id),
                    felt_from_hex(player, "player address")?,
                    felt_from_hex(session_key, "session key")?,
                    Felt::from(wager),
                    Felt::from(bet_side),
                    client_seed,
                    Felt::from(commitment_id),
                ],
            )
            .await?;
        self.wait_for_baccarat_round_status(expected_round_id, "active")
            .await?;
        Ok((expected_round_id, format!("{tx_hash:#x}")))
    }

    pub async fn settle_baccarat_round(
        &self,
        round_id: u64,
        server_seed: Felt,
    ) -> anyhow::Result<(BaccaratRoundView, String)> {
        let baccarat_table = self.baccarat_table()?;
        let tx_hash = self
            .invoke(
                baccarat_table,
                "settle_round",
                vec![Felt::from(round_id), server_seed],
            )
            .await?;
        let round = self
            .wait_for_baccarat_round_status(round_id, "settled")
            .await?;
        Ok((round, format!("{tx_hash:#x}")))
    }

    pub async fn fetch_baccarat_round(&self, round_id: u64) -> anyhow::Result<BaccaratRoundView> {
        let baccarat_table = self.baccarat_table()?;
        let values = self
            .call_contract(baccarat_table, "get_round", vec![Felt::from(round_id)])
            .await
            .with_context(|| format!("failed to read baccarat round {round_id}"))?;
        let player_card_count = felt_to_u8(
            values
                .get(12)
                .context("baccarat round missing player_card_count")?,
            "baccarat.player_card_count",
        )?;
        let banker_card_count = felt_to_u8(
            values
                .get(13)
                .context("baccarat round missing banker_card_count")?,
            "baccarat.banker_card_count",
        )?;
        let mut player_cards = Vec::new();
        let mut player_card_positions = Vec::new();
        let mut player_card_draw_indices = Vec::new();
        let mut player_card_attempts = Vec::new();
        let mut player_card_commitments = Vec::new();
        for index in 0..player_card_count {
            player_cards.push(self.fetch_baccarat_card(round_id, 0, index).await?);
            player_card_positions.push(
                self.fetch_baccarat_card_position(round_id, 0, index)
                    .await?,
            );
            player_card_draw_indices.push(
                self.fetch_baccarat_card_draw_index(round_id, 0, index)
                    .await?,
            );
            player_card_attempts.push(self.fetch_baccarat_card_attempt(round_id, 0, index).await?);
            player_card_commitments.push(
                self.fetch_baccarat_card_commitment(round_id, 0, index)
                    .await?,
            );
        }
        let mut banker_cards = Vec::new();
        let mut banker_card_positions = Vec::new();
        let mut banker_card_draw_indices = Vec::new();
        let mut banker_card_attempts = Vec::new();
        let mut banker_card_commitments = Vec::new();
        for index in 0..banker_card_count {
            banker_cards.push(self.fetch_baccarat_card(round_id, 1, index).await?);
            banker_card_positions.push(
                self.fetch_baccarat_card_position(round_id, 1, index)
                    .await?,
            );
            banker_card_draw_indices.push(
                self.fetch_baccarat_card_draw_index(round_id, 1, index)
                    .await?,
            );
            banker_card_attempts.push(self.fetch_baccarat_card_attempt(round_id, 1, index).await?);
            banker_card_commitments.push(
                self.fetch_baccarat_card_commitment(round_id, 1, index)
                    .await?,
            );
        }
        Ok(BaccaratRoundView {
            round_id: felt_to_u64(
                values.first().context("baccarat round missing round_id")?,
                "baccarat.round_id",
            )?,
            table_id: felt_to_u64(
                values.get(1).context("baccarat round missing table_id")?,
                "baccarat.table_id",
            )?,
            player: format!(
                "{:#x}",
                values.get(2).context("baccarat round missing player")?
            ),
            wager: felt_to_u128(
                values.get(3).context("baccarat round missing wager")?,
                "baccarat.wager",
            )?
            .to_string(),
            status: decode_hand_status(felt_to_u8(
                values.get(4).context("baccarat round missing status")?,
                "baccarat.status",
            )?)?
            .to_string(),
            transcript_root: format!(
                "{:#x}",
                values
                    .get(5)
                    .context("baccarat round missing transcript_root")?
            ),
            commitment_id: felt_to_u64(
                values
                    .get(6)
                    .context("baccarat round missing commitment_id")?,
                "baccarat.commitment_id",
            )?,
            server_seed_hash: format!(
                "{:#x}",
                values
                    .get(7)
                    .context("baccarat round missing server_seed_hash")?
            ),
            client_seed: format!(
                "{:#x}",
                values
                    .get(8)
                    .context("baccarat round missing client_seed")?
            ),
            bet_side: felt_to_u8(
                values.get(9).context("baccarat round missing bet_side")?,
                "baccarat.bet_side",
            )?,
            player_total: felt_to_u8(
                values
                    .get(10)
                    .context("baccarat round missing player_total")?,
                "baccarat.player_total",
            )?,
            banker_total: felt_to_u8(
                values
                    .get(11)
                    .context("baccarat round missing banker_total")?,
                "baccarat.banker_total",
            )?,
            player_card_count,
            banker_card_count,
            winner: felt_to_u8(
                values.get(14).context("baccarat round missing winner")?,
                "baccarat.winner",
            )?,
            payout: felt_to_u128(
                values.get(15).context("baccarat round missing payout")?,
                "baccarat.payout",
            )?
            .to_string(),
            player_cards,
            banker_cards,
            player_card_positions,
            banker_card_positions,
            player_card_draw_indices,
            banker_card_draw_indices,
            player_card_attempts,
            banker_card_attempts,
            player_card_commitments,
            banker_card_commitments,
        })
    }

    async fn fetch_baccarat_card(
        &self,
        round_id: u64,
        hand_index: u8,
        card_index: u8,
    ) -> anyhow::Result<u8> {
        let baccarat_table = self.baccarat_table()?;
        let values = self
            .call_contract(
                baccarat_table,
                "get_card",
                vec![
                    Felt::from(round_id),
                    Felt::from(hand_index),
                    Felt::from(card_index),
                ],
            )
            .await
            .with_context(|| {
                format!(
                    "failed to read baccarat card {card_index} for round {round_id} hand {hand_index}"
                )
            })?;
        felt_to_u8(
            values.first().context("baccarat card missing value")?,
            "baccarat.card",
        )
    }

    async fn fetch_baccarat_card_position(
        &self,
        round_id: u64,
        hand_index: u8,
        card_index: u8,
    ) -> anyhow::Result<u16> {
        let values = self
            .call_contract(
                self.baccarat_table()?,
                "get_card_position",
                vec![
                    Felt::from(round_id),
                    Felt::from(hand_index),
                    Felt::from(card_index),
                ],
            )
            .await
            .with_context(|| {
                format!(
                    "failed to read baccarat card position {card_index} for round {round_id} hand {hand_index}"
                )
            })?;
        felt_to_u16(
            values
                .first()
                .context("baccarat card position missing value")?,
            "baccarat.card_position",
        )
    }

    async fn fetch_baccarat_card_draw_index(
        &self,
        round_id: u64,
        hand_index: u8,
        card_index: u8,
    ) -> anyhow::Result<u8> {
        let values = self
            .call_contract(
                self.baccarat_table()?,
                "get_card_draw_index",
                vec![
                    Felt::from(round_id),
                    Felt::from(hand_index),
                    Felt::from(card_index),
                ],
            )
            .await
            .with_context(|| {
                format!(
                    "failed to read baccarat card draw index {card_index} for round {round_id} hand {hand_index}"
                )
            })?;
        felt_to_u8(
            values
                .first()
                .context("baccarat card draw index missing value")?,
            "baccarat.card_draw_index",
        )
    }

    async fn fetch_baccarat_card_attempt(
        &self,
        round_id: u64,
        hand_index: u8,
        card_index: u8,
    ) -> anyhow::Result<u8> {
        let values = self
            .call_contract(
                self.baccarat_table()?,
                "get_card_attempt",
                vec![
                    Felt::from(round_id),
                    Felt::from(hand_index),
                    Felt::from(card_index),
                ],
            )
            .await
            .with_context(|| {
                format!(
                    "failed to read baccarat card attempt {card_index} for round {round_id} hand {hand_index}"
                )
            })?;
        felt_to_u8(
            values
                .first()
                .context("baccarat card attempt missing value")?,
            "baccarat.card_attempt",
        )
    }

    async fn fetch_baccarat_card_commitment(
        &self,
        round_id: u64,
        hand_index: u8,
        card_index: u8,
    ) -> anyhow::Result<String> {
        let values = self
            .call_contract(
                self.baccarat_table()?,
                "get_card_commitment",
                vec![
                    Felt::from(round_id),
                    Felt::from(hand_index),
                    Felt::from(card_index),
                ],
            )
            .await
            .with_context(|| {
                format!(
                    "failed to read baccarat card commitment {card_index} for round {round_id} hand {hand_index}"
                )
            })?;
        Ok(format!(
            "{:#x}",
            values
                .first()
                .context("baccarat card commitment missing value")?
        ))
    }

    pub async fn wait_for_baccarat_round_status(
        &self,
        round_id: u64,
        expected_status: &str,
    ) -> anyhow::Result<BaccaratRoundView> {
        let mut last_round = None;
        let mut last_error = None;
        for _ in 0..HAND_POLL_ATTEMPTS {
            match self.fetch_baccarat_round(round_id).await {
                Ok(round) if round.status == expected_status => return Ok(round),
                Ok(round) => last_round = Some(round),
                Err(error) => last_error = Some(error),
            }
            tokio::time::sleep(Duration::from_millis(HAND_POLL_DELAY_MS)).await;
        }
        if let Some(round) = last_round {
            bail!(
                "timed out waiting for baccarat round {round_id} to reach {expected_status}; latest status {}",
                round.status
            );
        }
        if let Some(error) = last_error {
            return Err(error).context("timed out waiting for baccarat round status");
        }
        bail!("timed out waiting for baccarat round status");
    }

    pub async fn open_hand_verified(
        &self,
        expected_hand_id: u64,
        player: &str,
        table_id: u64,
        wager: u128,
        transcript_root: &str,
        dealer_peek_required: bool,
        dealer_blackjack: bool,
        dealer_upcard: u8,
        dealer_upcard_proof: &BlackjackOnchainCardRevealProof,
        player_first_card: u8,
        player_first_card_proof: &BlackjackOnchainCardRevealProof,
        player_second_card: u8,
        player_second_card_proof: &BlackjackOnchainCardRevealProof,
        dealer_peek_proof: &[String],
    ) -> anyhow::Result<(u64, String)> {
        if dealer_blackjack && !dealer_peek_required {
            bail!("dealer blackjack flag cannot be set when dealer peek is not required");
        }
        let (transcript_root_low, transcript_root_high) =
            felt_pair_from_hex_u256(transcript_root, "transcript_root")?;
        let commitment_calldata = vec![
            Felt::from(expected_hand_id),
            Felt::from(table_id),
            transcript_root_low,
            transcript_root_high,
            Felt::from(BLACKJACK_TIMEOUT_BLOCKS),
            if dealer_peek_required {
                Felt::ONE
            } else {
                Felt::ZERO
            },
            if dealer_blackjack {
                Felt::ONE
            } else {
                Felt::ZERO
            },
        ];
        let mut open_calldata = vec![
            Felt::from(table_id),
            felt_from_hex(player, "player address")?,
            Felt::from(wager),
            transcript_root_low,
            transcript_root_high,
            Felt::from(dealer_upcard),
        ];
        append_blackjack_card_reveal_proof(
            &mut open_calldata,
            dealer_upcard_proof,
            "dealer_upcard_proof",
        )?;
        open_calldata.push(Felt::from(player_first_card));
        append_blackjack_card_reveal_proof(
            &mut open_calldata,
            player_first_card_proof,
            "player_first_card_proof",
        )?;
        open_calldata.push(Felt::from(player_second_card));
        append_blackjack_card_reveal_proof(
            &mut open_calldata,
            player_second_card_proof,
            "player_second_card_proof",
        )?;
        open_calldata.push(Felt::from(
            u64::try_from(dealer_peek_proof.len()).context("dealer_peek_proof too long")?,
        ));
        for (index, value) in dealer_peek_proof.iter().enumerate() {
            open_calldata.push(felt_from_hex(
                value,
                &format!("dealer_peek_proof[{index}]"),
            )?);
        }
        let commitment_selector = get_selector_from_name("post_hand_commitment")
            .context("missing selector for post_hand_commitment")?;
        let open_selector = get_selector_from_name("open_hand_verified")
            .context("missing selector for open_hand_verified")?;
        let tx_hash = self
            .account
            .execute_v3(vec![
                Call {
                    to: self.contracts.deck_commitment,
                    selector: commitment_selector,
                    calldata: commitment_calldata,
                },
                Call {
                    to: self.contracts.blackjack_table,
                    selector: open_selector,
                    calldata: open_calldata,
                },
            ])
            .send()
            .await?;
        self.wait_for_hand(expected_hand_id, |_| true).await?;
        Ok((expected_hand_id, format!("{:#x}", tx_hash.transaction_hash)))
    }

    pub async fn submit_hit(
        &self,
        player: &str,
        hand_id: u64,
        seat_index: u8,
        drawn_card: u8,
    ) -> anyhow::Result<String> {
        self.invoke(
            self.contracts.blackjack_table,
            "submit_hit",
            vec![
                felt_from_hex(player, "player address")?,
                Felt::from(hand_id),
                Felt::from(seat_index),
                Felt::from(drawn_card),
            ],
        )
        .await
        .map(|hash| format!("{hash:#x}"))
    }

    pub async fn submit_hit_verified(
        &self,
        player: &str,
        hand_id: u64,
        seat_index: u8,
        drawn_card: u8,
        drawn_card_proof: &BlackjackOnchainCardRevealProof,
    ) -> anyhow::Result<String> {
        let mut calldata = vec![
            felt_from_hex(player, "player address")?,
            Felt::from(hand_id),
            Felt::from(seat_index),
            Felt::from(drawn_card),
        ];
        append_blackjack_card_reveal_proof(&mut calldata, drawn_card_proof, "hit_draw_proof")?;
        self.invoke(
            self.contracts.blackjack_table,
            "submit_hit_verified",
            calldata,
        )
        .await
        .map(|hash| format!("{hash:#x}"))
    }

    pub async fn submit_stand(
        &self,
        player: &str,
        hand_id: u64,
        seat_index: u8,
    ) -> anyhow::Result<String> {
        self.invoke(
            self.contracts.blackjack_table,
            "submit_stand",
            vec![
                felt_from_hex(player, "player address")?,
                Felt::from(hand_id),
                Felt::from(seat_index),
            ],
        )
        .await
        .map(|hash| format!("{hash:#x}"))
    }

    pub async fn submit_double(
        &self,
        player: &str,
        hand_id: u64,
        seat_index: u8,
        drawn_card: u8,
    ) -> anyhow::Result<String> {
        self.invoke(
            self.contracts.blackjack_table,
            "submit_double",
            vec![
                felt_from_hex(player, "player address")?,
                Felt::from(hand_id),
                Felt::from(seat_index),
                Felt::from(drawn_card),
            ],
        )
        .await
        .map(|hash| format!("{hash:#x}"))
    }

    pub async fn submit_double_verified(
        &self,
        player: &str,
        hand_id: u64,
        seat_index: u8,
        drawn_card: u8,
        drawn_card_proof: &BlackjackOnchainCardRevealProof,
    ) -> anyhow::Result<String> {
        let mut calldata = vec![
            felt_from_hex(player, "player address")?,
            Felt::from(hand_id),
            Felt::from(seat_index),
            Felt::from(drawn_card),
        ];
        append_blackjack_card_reveal_proof(&mut calldata, drawn_card_proof, "double_draw_proof")?;
        self.invoke(
            self.contracts.blackjack_table,
            "submit_double_verified",
            calldata,
        )
        .await
        .map(|hash| format!("{hash:#x}"))
    }

    pub async fn submit_split(
        &self,
        player: &str,
        hand_id: u64,
        seat_index: u8,
        left_drawn_card: u8,
        right_drawn_card: u8,
    ) -> anyhow::Result<String> {
        self.invoke(
            self.contracts.blackjack_table,
            "submit_split",
            vec![
                felt_from_hex(player, "player address")?,
                Felt::from(hand_id),
                Felt::from(seat_index),
                Felt::from(left_drawn_card),
                Felt::from(right_drawn_card),
            ],
        )
        .await
        .map(|hash| format!("{hash:#x}"))
    }

    pub async fn submit_split_verified(
        &self,
        player: &str,
        hand_id: u64,
        seat_index: u8,
        left_drawn_card: u8,
        left_drawn_card_proof: &BlackjackOnchainCardRevealProof,
        right_drawn_card: u8,
        right_drawn_card_proof: &BlackjackOnchainCardRevealProof,
    ) -> anyhow::Result<String> {
        let mut calldata = vec![
            felt_from_hex(player, "player address")?,
            Felt::from(hand_id),
            Felt::from(seat_index),
            Felt::from(left_drawn_card),
        ];
        append_blackjack_card_reveal_proof(
            &mut calldata,
            left_drawn_card_proof,
            "split_left_draw_proof",
        )?;
        calldata.push(Felt::from(right_drawn_card));
        append_blackjack_card_reveal_proof(
            &mut calldata,
            right_drawn_card_proof,
            "split_right_draw_proof",
        )?;
        self.invoke(
            self.contracts.blackjack_table,
            "submit_split_verified",
            calldata,
        )
        .await
        .map(|hash| format!("{hash:#x}"))
    }

    pub async fn submit_take_insurance(
        &self,
        player: &str,
        hand_id: u64,
        dealer_blackjack: bool,
    ) -> anyhow::Result<String> {
        self.invoke(
            self.contracts.blackjack_table,
            "submit_take_insurance",
            vec![
                felt_from_hex(player, "player address")?,
                Felt::from(hand_id),
                if dealer_blackjack {
                    Felt::ONE
                } else {
                    Felt::ZERO
                },
            ],
        )
        .await
        .map(|hash| format!("{hash:#x}"))
    }

    pub async fn submit_decline_insurance(
        &self,
        player: &str,
        hand_id: u64,
        dealer_blackjack: bool,
    ) -> anyhow::Result<String> {
        self.invoke(
            self.contracts.blackjack_table,
            "submit_decline_insurance",
            vec![
                felt_from_hex(player, "player address")?,
                Felt::from(hand_id),
                if dealer_blackjack {
                    Felt::ONE
                } else {
                    Felt::ZERO
                },
            ],
        )
        .await
        .map(|hash| format!("{hash:#x}"))
    }

    pub async fn submit_surrender(
        &self,
        player: &str,
        hand_id: u64,
        seat_index: u8,
    ) -> anyhow::Result<String> {
        self.invoke(
            self.contracts.blackjack_table,
            "submit_surrender",
            vec![
                felt_from_hex(player, "player address")?,
                Felt::from(hand_id),
                Felt::from(seat_index),
            ],
        )
        .await
        .map(|hash| format!("{hash:#x}"))
    }

    pub async fn reveal_dealer_card(&self, hand_id: u64, drawn_card: u8) -> anyhow::Result<String> {
        self.invoke(
            self.contracts.blackjack_table,
            "reveal_dealer_card",
            vec![Felt::from(hand_id), Felt::from(drawn_card)],
        )
        .await
        .map(|hash| format!("{hash:#x}"))
    }

    pub async fn reveal_dealer_card_verified(
        &self,
        hand_id: u64,
        drawn_card: u8,
        drawn_card_proof: &BlackjackOnchainCardRevealProof,
    ) -> anyhow::Result<String> {
        let mut calldata = vec![Felt::from(hand_id), Felt::from(drawn_card)];
        append_blackjack_card_reveal_proof(&mut calldata, drawn_card_proof, "dealer_draw_proof")?;
        self.invoke(
            self.contracts.blackjack_table,
            "reveal_dealer_card_verified",
            calldata,
        )
        .await
        .map(|hash| format!("{hash:#x}"))
    }

    pub async fn finalize_hand(&self, hand_id: u64) -> anyhow::Result<String> {
        self.invoke(
            self.contracts.blackjack_table,
            "finalize_hand",
            vec![Felt::from(hand_id)],
        )
        .await
        .map(|hash| format!("{hash:#x}"))
    }

    pub async fn force_expired_insurance_decline(&self, hand_id: u64) -> anyhow::Result<String> {
        self.invoke(
            self.contracts.blackjack_table,
            "force_expired_insurance_decline",
            vec![Felt::from(hand_id)],
        )
        .await
        .map(|hash| format!("{hash:#x}"))
    }

    pub async fn force_expired_stand(&self, hand_id: u64) -> anyhow::Result<String> {
        self.invoke(
            self.contracts.blackjack_table,
            "force_expired_stand",
            vec![Felt::from(hand_id)],
        )
        .await
        .map(|hash| format!("{hash:#x}"))
    }

    pub async fn void_expired_blackjack_hand(&self, hand_id: u64) -> anyhow::Result<String> {
        self.invoke(
            self.contracts.blackjack_table,
            "void_expired_hand",
            vec![Felt::from(hand_id)],
        )
        .await
        .map(|hash| format!("{hash:#x}"))
    }

    pub async fn fetch_blackjack_hand(&self, hand_id: u64) -> anyhow::Result<BlackjackChainHand> {
        let hand = self
            .provider
            .call(
                FunctionCall {
                    contract_address: self.contracts.blackjack_table,
                    entry_point_selector: get_selector_from_name("get_hand")
                        .context("missing selector for get_hand")?,
                    calldata: vec![Felt::from(hand_id)],
                },
                BlockId::Tag(BlockTag::Latest),
            )
            .await
            .with_context(|| format!("failed to read get_hand for chain hand {hand_id}"))?;

        let seat_count = felt_to_u8(
            hand.get(13).context("hand missing seat_count")?,
            "seat_count",
        )?;
        let dealer_card_count = felt_to_u8(
            hand.get(8).context("hand missing dealer_card_count")?,
            "dealer_card_count",
        )?;
        let split_count = felt_to_u8(
            hand.get(15).context("hand missing split_count")?,
            "split_count",
        )?;
        let action_count = felt_to_u8(
            hand.get(12).context("hand missing action_count")?,
            "action_count",
        )?;
        let player = format!("{:#x}", hand.get(2).context("hand missing player")?);
        let status = decode_hand_status(felt_to_u8(
            hand.get(4).context("hand missing status")?,
            "status",
        )?)?;

        let mut seats = Vec::with_capacity(seat_count as usize);
        for seat_index in 0..seat_count {
            let seat = self
                .provider
                .call(
                    FunctionCall {
                        contract_address: self.contracts.blackjack_table,
                        entry_point_selector: get_selector_from_name("get_seat")
                            .context("missing selector for get_seat")?,
                        calldata: vec![Felt::from(hand_id), Felt::from(seat_index)],
                    },
                    BlockId::Tag(BlockTag::Latest),
                )
                .await
                .with_context(|| {
                    format!("failed to read get_seat for hand {hand_id} seat {seat_index}")
                })?;

            let card_count = felt_to_u8(
                seat.get(2).context("seat missing card_count")?,
                "seat.card_count",
            )?;
            let mut cards = Vec::with_capacity(card_count as usize);
            for card_index in 0..card_count {
                let card = self
                    .provider
                    .call(
                        FunctionCall {
                            contract_address: self.contracts.blackjack_table,
                            entry_point_selector: get_selector_from_name("get_player_card")
                                .context("missing selector for get_player_card")?,
                            calldata: vec![
                                Felt::from(hand_id),
                                Felt::from(seat_index),
                                Felt::from(card_index),
                            ],
                        },
                        BlockId::Tag(BlockTag::Latest),
                    )
                    .await
                    .with_context(|| format!("failed to read player card {card_index} for hand {hand_id} seat {seat_index}"))?;
                cards.push(felt_to_u8(
                    card.first().context("player card missing value")?,
                    "player_card",
                )?);
            }

            seats.push(BlackjackChainSeat {
                seat_index,
                wager: felt_to_u128(seat.get(0).context("seat missing wager")?, "seat.wager")?
                    .to_string(),
                status: decode_seat_status(felt_to_u8(
                    seat.get(1).context("seat missing status")?,
                    "seat.status",
                )?)?,
                outcome: Some(decode_hand_outcome(felt_to_u8(
                    seat.get(8).context("seat missing outcome")?,
                    "seat.outcome",
                )?)?)
                .filter(|outcome| outcome != "pending"),
                payout: felt_to_u128(seat.get(9).context("seat missing payout")?, "seat.payout")?
                    .to_string(),
                doubled: felt_to_bool(seat.get(7).context("seat missing doubled")?),
                cards,
            });
        }

        let mut dealer_cards = Vec::with_capacity(dealer_card_count as usize);
        for card_index in 0..dealer_card_count {
            let card = self
                .provider
                .call(
                    FunctionCall {
                        contract_address: self.contracts.blackjack_table,
                        entry_point_selector: get_selector_from_name("get_dealer_card")
                            .context("missing selector for get_dealer_card")?,
                        calldata: vec![Felt::from(hand_id), Felt::from(card_index)],
                    },
                    BlockId::Tag(BlockTag::Latest),
                )
                .await
                .with_context(|| {
                    format!("failed to read dealer card {card_index} for hand {hand_id}")
                })?;
            dealer_cards.push(felt_to_u8(
                card.first().context("dealer card missing value")?,
                "dealer_card",
            )?);
        }

        let insurance_wager = self
            .provider
            .call(
                FunctionCall {
                    contract_address: self.contracts.blackjack_table,
                    entry_point_selector: get_selector_from_name("get_insurance_wager")
                        .context("missing selector for get_insurance_wager")?,
                    calldata: vec![Felt::from(hand_id)],
                },
                BlockId::Tag(BlockTag::Latest),
            )
            .await
            .with_context(|| format!("failed to read insurance wager for hand {hand_id}"))?;
        let insurance_wager = felt_to_u128(
            insurance_wager
                .first()
                .context("insurance wager missing value")?,
            "insurance_wager",
        )?;
        let dealer_blackjack = dealer_cards.len() == 2 && {
            let total = dealer_cards
                .iter()
                .map(|card| match card {
                    1 => 11_u8,
                    10..=13 => 10_u8,
                    value => *value,
                })
                .sum::<u8>();
            if dealer_cards.contains(&1) && total > 21 {
                total - 10 == 21
            } else {
                total == 21
            }
        };

        let total_payout = (seats.iter().try_fold(0_u128, |acc, seat| {
            seat.payout
                .parse::<u128>()
                .map(|value| acc + value)
                .context("failed to sum seat payout")
        })? + if dealer_blackjack && insurance_wager > 0 {
            insurance_wager * 3
        } else {
            0
        })
        .to_string();

        Ok(BlackjackChainHand {
            hand_id,
            player,
            table_id: felt_to_u64(hand.get(1).context("hand missing table_id")?, "table_id")?,
            wager: felt_to_u128(hand.get(3).context("hand missing wager")?, "wager")?.to_string(),
            status: status.to_string(),
            phase: phase_for_status(status),
            transcript_root: felt_pair_to_hex_u256(
                hand.get(5).context("hand missing transcript_root_low")?,
                hand.get(6).context("hand missing transcript_root_high")?,
            ),
            active_seat: felt_to_u8(
                hand.get(14).context("hand missing active_seat")?,
                "active_seat",
            )?,
            seat_count,
            action_count,
            split_count,
            dealer_cards,
            seats,
            total_payout,
        })
    }

    pub async fn wait_for_hand<F>(
        &self,
        hand_id: u64,
        predicate: F,
    ) -> anyhow::Result<BlackjackChainHand>
    where
        F: Fn(&BlackjackChainHand) -> bool,
    {
        let mut last_error = None;
        for _ in 0..HAND_POLL_ATTEMPTS {
            match self.fetch_blackjack_hand(hand_id).await {
                Ok(hand) if predicate(&hand) => return Ok(hand),
                Ok(_) => {}
                Err(error) => last_error = Some(error),
            }
            tokio::time::sleep(Duration::from_millis(HAND_POLL_DELAY_MS)).await;
        }

        if let Some(error) = last_error {
            return Err(error).context("timed out waiting for chain hand state");
        }
        bail!("timed out waiting for chain hand state");
    }

    pub async fn wait_for_hand_state(
        &self,
        hand_id: u64,
        expected_status: &str,
        expected_phase: &str,
        expected_action_count: u8,
        expected_seat_count: u8,
        expected_dealer_cards: usize,
    ) -> anyhow::Result<BlackjackChainHand> {
        self.wait_for_hand(hand_id, |hand| {
            hand.status == expected_status
                && hand.phase == expected_phase
                && hand.action_count >= expected_action_count
                && hand.seat_count == expected_seat_count
                && hand.dealer_cards.len() >= expected_dealer_cards
        })
        .await
    }

    async fn invoke(
        &self,
        to: Felt,
        selector_name: &str,
        calldata: Vec<Felt>,
    ) -> anyhow::Result<Felt> {
        let selector = get_selector_from_name(selector_name)
            .with_context(|| format!("missing selector for {selector_name}"))?;
        let result = self
            .account
            .execute_v3(vec![Call {
                to,
                selector,
                calldata,
            }])
            .send()
            .await
            .with_context(|| format!("failed to invoke {selector_name}"))?;
        Ok(result.transaction_hash)
    }

    async fn call_contract(
        &self,
        contract_address: Felt,
        selector_name: &str,
        calldata: Vec<Felt>,
    ) -> anyhow::Result<Vec<Felt>> {
        let selector = get_selector_from_name(selector_name)
            .with_context(|| format!("missing selector for {selector_name}"))?;
        self.provider_call_with_retry(
            FunctionCall {
                contract_address,
                entry_point_selector: selector,
                calldata,
            },
            &format!("failed to call {selector_name}"),
        )
        .await
    }

    fn dice_table(&self) -> anyhow::Result<Felt> {
        self.contracts
            .dice_table
            .context("MOROS_DICE_TABLE_ADDRESS is not configured")
    }

    fn roulette_table(&self) -> anyhow::Result<Felt> {
        self.contracts
            .roulette_table
            .context("MOROS_ROULETTE_TABLE_ADDRESS is not configured")
    }

    fn baccarat_table(&self) -> anyhow::Result<Felt> {
        self.contracts
            .baccarat_table
            .context("MOROS_BACCARAT_TABLE_ADDRESS is not configured")
    }
}

fn felt_from_hex(value: &str, label: &str) -> anyhow::Result<Felt> {
    Felt::from_hex(value).with_context(|| format!("invalid {label}: {value}"))
}

fn append_blackjack_card_reveal_proof(
    calldata: &mut Vec<Felt>,
    proof: &BlackjackOnchainCardRevealProof,
    label: &str,
) -> anyhow::Result<()> {
    if proof.siblings.len() != 9 {
        bail!("{label} must contain exactly 9 Merkle siblings");
    }
    calldata.push(Felt::from(proof.deck_index));
    calldata.push(Felt::from(proof.card_id));
    let (salt_low, salt_high) = felt_pair_from_hex_u256(&proof.salt, &format!("{label}.salt"))?;
    calldata.push(salt_low);
    calldata.push(salt_high);
    for (index, sibling) in proof.siblings.iter().enumerate() {
        let (sibling_low, sibling_high) =
            felt_pair_from_hex_u256(sibling, &format!("{label}.sibling_{index}"))?;
        calldata.push(sibling_low);
        calldata.push(sibling_high);
    }
    Ok(())
}

fn felt_pair_from_hex_u256(value: &str, label: &str) -> anyhow::Result<(Felt, Felt)> {
    let trimmed = value.trim();
    let normalized = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);
    let parsed = if normalized.is_empty() {
        BigUint::default()
    } else {
        BigUint::parse_bytes(normalized.as_bytes(), 16)
            .with_context(|| format!("invalid {label}: {value}"))?
    };
    let low_mask = (BigUint::from(1_u8) << 128) - BigUint::from(1_u8);
    let low = &parsed & &low_mask;
    let high: BigUint = parsed >> 128;
    Ok((
        felt_from_hex(
            &format!("0x{}", low.to_str_radix(16)),
            &format!("{label}.low"),
        )?,
        felt_from_hex(
            &format!("0x{}", high.to_str_radix(16)),
            &format!("{label}.high"),
        )?,
    ))
}

fn felt_pair_to_hex_u256(low: &Felt, high: &Felt) -> String {
    let low_hex = format!("{low:#x}");
    let high_hex = format!("{high:#x}");
    let low_bn =
        BigUint::parse_bytes(low_hex.trim_start_matches("0x").as_bytes(), 16).unwrap_or_default();
    let high_bn =
        BigUint::parse_bytes(high_hex.trim_start_matches("0x").as_bytes(), 16).unwrap_or_default();
    let combined: BigUint = low_bn + (high_bn << 128);
    if combined == BigUint::default() {
        "0x0".to_string()
    } else {
        format!("0x{}", combined.to_str_radix(16))
    }
}

fn felt_to_u64(value: &Felt, label: &str) -> anyhow::Result<u64> {
    value
        .to_string()
        .parse::<u64>()
        .with_context(|| format!("failed to parse {label} as u64"))
}

fn felt_to_u128(value: &Felt, label: &str) -> anyhow::Result<u128> {
    value
        .to_string()
        .parse::<u128>()
        .with_context(|| format!("failed to parse {label} as u128"))
}

fn felt_to_u8(value: &Felt, label: &str) -> anyhow::Result<u8> {
    value
        .to_string()
        .parse::<u8>()
        .with_context(|| format!("failed to parse {label} as u8"))
}

fn felt_to_u16(value: &Felt, label: &str) -> anyhow::Result<u16> {
    value
        .to_string()
        .parse::<u16>()
        .with_context(|| format!("failed to parse {label} as u16"))
}

fn felt_to_u32(value: &Felt, label: &str) -> anyhow::Result<u32> {
    value
        .to_string()
        .parse::<u32>()
        .with_context(|| format!("failed to parse {label} as u32"))
}

fn felt_to_bool(value: &Felt) -> bool {
    *value != Felt::ZERO
}

fn decode_hand_status(index: u8) -> anyhow::Result<&'static str> {
    match index {
        0 => Ok("none"),
        1 => Ok("active"),
        2 => Ok("awaiting_dealer"),
        3 => Ok("settled"),
        4 => Ok("voided"),
        5 => Ok("awaiting_insurance"),
        other => bail!("unknown hand status variant {other}"),
    }
}

fn decode_dice_commitment_status(index: u8) -> anyhow::Result<&'static str> {
    match index {
        0 => Ok("none"),
        1 => Ok("available"),
        2 => Ok("locked"),
        3 => Ok("revealed"),
        4 => Ok("voided"),
        other => bail!("unknown dice commitment status variant {other}"),
    }
}

fn decode_game_kind(index: u8) -> anyhow::Result<&'static str> {
    match index {
        0 => Ok("blackjack"),
        1 => Ok("roulette"),
        2 => Ok("baccarat"),
        3 => Ok("dice"),
        other => bail!("unknown game kind variant {other}"),
    }
}

fn decode_table_status(index: u8) -> anyhow::Result<&'static str> {
    match index {
        0 => Ok("inactive"),
        1 => Ok("active"),
        2 => Ok("paused"),
        other => bail!("unknown table status variant {other}"),
    }
}

fn decode_seat_status(index: u8) -> anyhow::Result<String> {
    Ok(match index {
        0 => "none",
        1 => "active",
        2 => "standing",
        3 => "blackjack",
        4 => "busted",
        5 => "surrendered",
        6 => "settled",
        other => bail!("unknown seat status variant {other}"),
    }
    .to_string())
}

fn decode_hand_outcome(index: u8) -> anyhow::Result<String> {
    Ok(match index {
        0 => "pending",
        1 => "loss",
        2 => "push",
        3 => "win",
        4 => "blackjack",
        5 => "surrender",
        other => bail!("unknown hand outcome variant {other}"),
    }
    .to_string())
}

fn phase_for_status(status: &str) -> String {
    match status {
        "active" => "player_turn",
        "awaiting_dealer" => "dealer_turn",
        "awaiting_insurance" => "insurance",
        "settled" => "settled",
        "voided" => "voided",
        _ => "unknown",
    }
    .to_string()
}
