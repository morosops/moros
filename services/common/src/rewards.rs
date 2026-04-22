use crate::{accounts, runtime};
use anyhow::{Context, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Row, Transaction, postgres::PgRow};
use std::{
    collections::{BTreeMap, HashMap},
    env,
};
use uuid::Uuid;

const BPS_DENOMINATOR: u128 = 10_000;
const STRK_WEI: u128 = 1_000_000_000_000_000_000;
const DEFAULT_RAKEBACK_SHARE_BPS: u32 = 6_500;
const DEFAULT_WEEKLY_SHARE_BPS: u32 = 2_500;
const DEFAULT_LEVEL_UP_SHARE_BPS: u32 = 1_000;
const DEFAULT_CLAIM_RESERVATION_TTL_SECONDS: i64 = 300;
const DEFAULT_BLACKJACK_REWARD_HOUSE_EDGE_BPS: u32 = 50;
const DEFAULT_DICE_REWARD_HOUSE_EDGE_BPS: u32 = 100;
const DEFAULT_ROULETTE_REWARD_HOUSE_EDGE_BPS: u32 = 270;
const DEFAULT_BACCARAT_REWARD_HOUSE_EDGE_BPS: u32 = 120;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RewardKind {
    Rakeback,
    Weekly,
    Monthly,
    LevelUp,
    Referral,
    Coupon,
}

impl RewardKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rakeback => "rakeback",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
            Self::LevelUp => "level_up",
            Self::Referral => "referral",
            Self::Coupon => "coupon",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "rakeback" => Some(Self::Rakeback),
            "weekly" => Some(Self::Weekly),
            "monthly" => Some(Self::Monthly),
            "level_up" | "levelup" => Some(Self::LevelUp),
            "referral" => Some(Self::Referral),
            "coupon" => Some(Self::Coupon),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RewardsTierConfig {
    pub level: u8,
    pub name: String,
    pub threshold_raw: u128,
    pub rakeback_bps: u32,
    pub weekly_bps: u32,
    pub level_up_bonus_raw: u128,
}

#[derive(Debug, Clone)]
pub struct RewardsConfig {
    pub budget_share_bps: u32,
    pub rakeback_share_bps: u32,
    pub weekly_share_bps: u32,
    pub level_up_share_bps: u32,
    pub referral_rate_bps: u32,
    pub max_counted_wager_per_bet_raw: u128,
    pub rewards_pool_cap_raw: Option<u128>,
    pub rakeback_user_cap_raw: u128,
    pub weekly_user_cap_raw: u128,
    pub global_epoch_cap_raw: u128,
    pub referral_user_cap_raw: u128,
    pub referral_global_cap_raw: u128,
    pub weekly_min_weighted_volume_raw: u128,
    pub claim_reservation_ttl_seconds: i64,
    pub blackjack_reward_house_edge_bps: u32,
    pub dice_reward_house_edge_bps: u32,
    pub roulette_reward_house_edge_bps: u32,
    pub baccarat_reward_house_edge_bps: u32,
    pub tiers: Vec<RewardsTierConfig>,
}

impl Default for RewardsConfig {
    fn default() -> Self {
        Self {
            budget_share_bps: 2_000,
            rakeback_share_bps: DEFAULT_RAKEBACK_SHARE_BPS,
            weekly_share_bps: DEFAULT_WEEKLY_SHARE_BPS,
            level_up_share_bps: DEFAULT_LEVEL_UP_SHARE_BPS,
            referral_rate_bps: 2_500,
            max_counted_wager_per_bet_raw: 250_000 * STRK_WEI,
            rewards_pool_cap_raw: None,
            rakeback_user_cap_raw: 10_000 * STRK_WEI,
            weekly_user_cap_raw: 2_500 * STRK_WEI,
            global_epoch_cap_raw: 100_000 * STRK_WEI,
            referral_user_cap_raw: 2_500 * STRK_WEI,
            referral_global_cap_raw: 25_000 * STRK_WEI,
            weekly_min_weighted_volume_raw: STRK_WEI,
            claim_reservation_ttl_seconds: DEFAULT_CLAIM_RESERVATION_TTL_SECONDS,
            blackjack_reward_house_edge_bps: DEFAULT_BLACKJACK_REWARD_HOUSE_EDGE_BPS,
            dice_reward_house_edge_bps: DEFAULT_DICE_REWARD_HOUSE_EDGE_BPS,
            roulette_reward_house_edge_bps: DEFAULT_ROULETTE_REWARD_HOUSE_EDGE_BPS,
            baccarat_reward_house_edge_bps: DEFAULT_BACCARAT_REWARD_HOUSE_EDGE_BPS,
            tiers: default_rewards_tiers(),
        }
    }
}

impl RewardsConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        let mut config = Self::default();
        if let Some(value) = read_env_u32("MOROS_REWARDS_BUDGET_SHARE_BPS")? {
            config.budget_share_bps = value;
        }
        if let Some(value) = read_env_u32("MOROS_REWARDS_RAKEBACK_SHARE_BPS")? {
            config.rakeback_share_bps = value;
        }
        if let Some(value) = read_env_u32("MOROS_REWARDS_WEEKLY_SHARE_BPS")? {
            config.weekly_share_bps = value;
        }
        if let Some(value) = read_env_u32("MOROS_REWARDS_LEVEL_UP_SHARE_BPS")? {
            config.level_up_share_bps = value;
        }
        if let Some(value) = read_env_u32("MOROS_REWARDS_REFERRAL_RATE_BPS")? {
            config.referral_rate_bps = value;
        }
        if let Some(value) = read_env_u128("MOROS_REWARDS_MAX_COUNTED_WAGER_PER_BET_RAW")? {
            config.max_counted_wager_per_bet_raw = value;
        }
        if let Some(value) = read_env_u128("MOROS_REWARDS_POOL_CAP_RAW")? {
            config.rewards_pool_cap_raw = Some(value);
        }
        if let Some(value) = read_env_u128("MOROS_REWARDS_RAKEBACK_USER_CAP_RAW")? {
            config.rakeback_user_cap_raw = value;
        }
        if let Some(value) = read_env_u128("MOROS_REWARDS_WEEKLY_USER_CAP_RAW")? {
            config.weekly_user_cap_raw = value;
        }
        if let Some(value) = read_env_u128("MOROS_REWARDS_GLOBAL_EPOCH_CAP_RAW")? {
            config.global_epoch_cap_raw = value;
        }
        if let Some(value) = read_env_u128("MOROS_REWARDS_REFERRAL_USER_CAP_RAW")? {
            config.referral_user_cap_raw = value;
        }
        if let Some(value) = read_env_u128("MOROS_REWARDS_REFERRAL_GLOBAL_CAP_RAW")? {
            config.referral_global_cap_raw = value;
        }
        if let Some(value) = read_env_u128("MOROS_REWARDS_WEEKLY_MIN_WEIGHTED_VOLUME_RAW")? {
            config.weekly_min_weighted_volume_raw = value;
        }
        if let Some(value) = read_env_u32("MOROS_REWARDS_BLACKJACK_HOUSE_EDGE_BPS")? {
            config.blackjack_reward_house_edge_bps = value;
        }
        if let Some(value) = read_env_u32("MOROS_REWARDS_DICE_HOUSE_EDGE_BPS")? {
            config.dice_reward_house_edge_bps = value;
        }
        if let Some(value) = read_env_u32("MOROS_REWARDS_ROULETTE_HOUSE_EDGE_BPS")? {
            config.roulette_reward_house_edge_bps = value;
        }
        if let Some(value) = read_env_u32("MOROS_REWARDS_BACCARAT_HOUSE_EDGE_BPS")? {
            config.baccarat_reward_house_edge_bps = value;
        }
        if let Some(value) = read_env_i64("MOROS_REWARDS_CLAIM_RESERVATION_TTL_SECONDS")? {
            config.claim_reservation_ttl_seconds = value.max(60);
        }
        if let Some(tiers) = read_env_rewards_tiers("MOROS_REWARDS_TIERS_JSON")? {
            config.tiers = tiers;
        }
        Ok(config)
    }

    pub fn config_view(&self) -> RewardsConfigView {
        RewardsConfigView {
            budget_share_bps: self.budget_share_bps,
            rakeback_share_bps: self.rakeback_share_bps,
            weekly_share_bps: self.weekly_share_bps,
            level_up_share_bps: self.level_up_share_bps,
            referral_rate_bps: self.referral_rate_bps,
            max_counted_wager_per_bet_raw: self.max_counted_wager_per_bet_raw.to_string(),
            rewards_pool_cap_raw: self.rewards_pool_cap_raw.map(|value| value.to_string()),
            rakeback_user_cap_raw: self.rakeback_user_cap_raw.to_string(),
            weekly_user_cap_raw: self.weekly_user_cap_raw.to_string(),
            global_epoch_cap_raw: self.global_epoch_cap_raw.to_string(),
            referral_user_cap_raw: self.referral_user_cap_raw.to_string(),
            referral_global_cap_raw: self.referral_global_cap_raw.to_string(),
            weekly_min_weighted_volume_raw: self.weekly_min_weighted_volume_raw.to_string(),
            claim_reservation_ttl_seconds: self.claim_reservation_ttl_seconds,
            blackjack_reward_house_edge_bps: self.blackjack_reward_house_edge_bps,
            dice_reward_house_edge_bps: self.dice_reward_house_edge_bps,
            roulette_reward_house_edge_bps: self.roulette_reward_house_edge_bps,
            baccarat_reward_house_edge_bps: self.baccarat_reward_house_edge_bps,
            tiers: self
                .tiers
                .iter()
                .map(|tier| RewardsTierView {
                    level: tier.level,
                    name: tier.name.clone(),
                    threshold_raw: tier.threshold_raw.to_string(),
                    rakeback_bps: tier.rakeback_bps,
                    weekly_bps: tier.weekly_bps,
                    level_up_bonus_raw: tier.level_up_bonus_raw.to_string(),
                })
                .collect(),
        }
    }
}

fn default_rewards_tiers() -> Vec<RewardsTierConfig> {
    vec![
        RewardsTierConfig {
            level: 0,
            name: "Base".to_string(),
            threshold_raw: 0,
            rakeback_bps: 0,
            weekly_bps: 0,
            level_up_bonus_raw: 0,
        },
        RewardsTierConfig {
            level: 1,
            name: "Bronze".to_string(),
            threshold_raw: 10_000 * STRK_WEI,
            rakeback_bps: 350,
            weekly_bps: 100,
            level_up_bonus_raw: 20 * STRK_WEI,
        },
        RewardsTierConfig {
            level: 2,
            name: "Silver".to_string(),
            threshold_raw: 50_000 * STRK_WEI,
            rakeback_bps: 350,
            weekly_bps: 150,
            level_up_bonus_raw: 50 * STRK_WEI,
        },
        RewardsTierConfig {
            level: 3,
            name: "Gold".to_string(),
            threshold_raw: 100_000 * STRK_WEI,
            rakeback_bps: 350,
            weekly_bps: 200,
            level_up_bonus_raw: 100 * STRK_WEI,
        },
        RewardsTierConfig {
            level: 4,
            name: "Platinum I".to_string(),
            threshold_raw: 250_000 * STRK_WEI,
            rakeback_bps: 350,
            weekly_bps: 250,
            level_up_bonus_raw: 200 * STRK_WEI,
        },
        RewardsTierConfig {
            level: 5,
            name: "Platinum II".to_string(),
            threshold_raw: 500_000 * STRK_WEI,
            rakeback_bps: 350,
            weekly_bps: 250,
            level_up_bonus_raw: 250 * STRK_WEI,
        },
        RewardsTierConfig {
            level: 6,
            name: "Platinum III".to_string(),
            threshold_raw: 1_000_000 * STRK_WEI,
            rakeback_bps: 350,
            weekly_bps: 250,
            level_up_bonus_raw: 300 * STRK_WEI,
        },
        RewardsTierConfig {
            level: 7,
            name: "Platinum IV".to_string(),
            threshold_raw: 2_500_000 * STRK_WEI,
            rakeback_bps: 350,
            weekly_bps: 250,
            level_up_bonus_raw: 400 * STRK_WEI,
        },
        RewardsTierConfig {
            level: 8,
            name: "Platinum V".to_string(),
            threshold_raw: 5_000_000 * STRK_WEI,
            rakeback_bps: 350,
            weekly_bps: 250,
            level_up_bonus_raw: 500 * STRK_WEI,
        },
        RewardsTierConfig {
            level: 9,
            name: "Platinum VI".to_string(),
            threshold_raw: 10_000_000 * STRK_WEI,
            rakeback_bps: 350,
            weekly_bps: 250,
            level_up_bonus_raw: 750 * STRK_WEI,
        },
        RewardsTierConfig {
            level: 10,
            name: "Diamond I".to_string(),
            threshold_raw: 25_000_000 * STRK_WEI,
            rakeback_bps: 350,
            weekly_bps: 300,
            level_up_bonus_raw: 1_250 * STRK_WEI,
        },
        RewardsTierConfig {
            level: 11,
            name: "Diamond II".to_string(),
            threshold_raw: 50_000_000 * STRK_WEI,
            rakeback_bps: 350,
            weekly_bps: 300,
            level_up_bonus_raw: 2_000 * STRK_WEI,
        },
        RewardsTierConfig {
            level: 12,
            name: "Diamond III".to_string(),
            threshold_raw: 100_000_000 * STRK_WEI,
            rakeback_bps: 350,
            weekly_bps: 300,
            level_up_bonus_raw: 3_500 * STRK_WEI,
        },
        RewardsTierConfig {
            level: 13,
            name: "Diamond IV".to_string(),
            threshold_raw: 250_000_000 * STRK_WEI,
            rakeback_bps: 350,
            weekly_bps: 300,
            level_up_bonus_raw: 6_000 * STRK_WEI,
        },
        RewardsTierConfig {
            level: 14,
            name: "Diamond V".to_string(),
            threshold_raw: 500_000_000 * STRK_WEI,
            rakeback_bps: 350,
            weekly_bps: 300,
            level_up_bonus_raw: 10_000 * STRK_WEI,
        },
        RewardsTierConfig {
            level: 15,
            name: "Tanzanite".to_string(),
            threshold_raw: 1_000_000_000 * STRK_WEI,
            rakeback_bps: 350,
            weekly_bps: 400,
            level_up_bonus_raw: 25_000 * STRK_WEI,
        },
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewardsTierView {
    pub level: u8,
    pub name: String,
    pub threshold_raw: String,
    pub rakeback_bps: u32,
    pub weekly_bps: u32,
    pub level_up_bonus_raw: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewardsConfigView {
    pub budget_share_bps: u32,
    pub rakeback_share_bps: u32,
    pub weekly_share_bps: u32,
    pub level_up_share_bps: u32,
    pub referral_rate_bps: u32,
    pub max_counted_wager_per_bet_raw: String,
    pub rewards_pool_cap_raw: Option<String>,
    pub rakeback_user_cap_raw: String,
    pub weekly_user_cap_raw: String,
    pub global_epoch_cap_raw: String,
    pub referral_user_cap_raw: String,
    pub referral_global_cap_raw: String,
    pub weekly_min_weighted_volume_raw: String,
    pub claim_reservation_ttl_seconds: i64,
    pub blackjack_reward_house_edge_bps: u32,
    pub dice_reward_house_edge_bps: u32,
    pub roulette_reward_house_edge_bps: u32,
    pub baccarat_reward_house_edge_bps: u32,
    pub tiers: Vec<RewardsTierView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VipProgressView {
    pub lifetime_wager_raw: String,
    pub wager_7d_raw: String,
    pub wager_30d_raw: String,
    pub lifetime_weighted_volume_raw: String,
    pub weighted_volume_7d_raw: String,
    pub weighted_volume_30d_raw: String,
    pub vip_points_raw: String,
    pub current_tier_level: u8,
    pub current_tier_name: String,
    pub next_tier_level: Option<u8>,
    pub next_tier_name: Option<String>,
    pub next_tier_threshold_raw: Option<String>,
    pub progress_bps: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewardBucketView {
    pub accrued_raw: String,
    pub claimed_raw: String,
    pub claimable_raw: String,
    pub scale_bps: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewardEpochView {
    pub epoch_key: String,
    pub tier_level: u8,
    pub tier_name: String,
    pub wager_volume_raw: String,
    pub weighted_volume_raw: String,
    pub raw_bonus_raw: String,
    pub claimable_raw: String,
    pub scale_bps: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelUpRewardView {
    pub tier_level: u8,
    pub tier_name: String,
    pub bonus_raw: String,
    pub claimable_raw: String,
    pub crossed_at_unix: i64,
    pub scale_bps: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferralView {
    pub referrer_wallet_address: Option<String>,
    pub referrer_username: Option<String>,
    pub linked_at_unix: Option<i64>,
    pub referred_users: u64,
    pub accrued_raw: String,
    pub claimed_raw: String,
    pub claimable_raw: String,
    pub referral_rate_bps: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalRewardsVolumeView {
    pub lifetime_wager_raw: String,
    pub lifetime_weighted_volume_raw: String,
    pub weighted_volume_7d_raw: String,
    pub weighted_volume_30d_raw: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewardsStateView {
    pub wallet_address: String,
    pub vip: VipProgressView,
    pub global_volume: GlobalRewardsVolumeView,
    pub rakeback: RewardBucketView,
    pub weekly: RewardBucketView,
    pub level_up: RewardBucketView,
    pub referral: ReferralView,
    pub rakeback_epochs: Vec<RewardEpochView>,
    pub weekly_epochs: Vec<RewardEpochView>,
    pub level_up_rewards: Vec<LevelUpRewardView>,
    pub claimable_total_raw: String,
    pub config: RewardsConfigView,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferralBindingView {
    pub referrer_wallet_address: Option<String>,
    pub referrer_username: Option<String>,
    pub linked_at_unix: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewardCouponRecord {
    pub id: String,
    pub code: String,
    pub description: Option<String>,
    pub amount_raw: String,
    pub max_global_redemptions: i64,
    pub max_per_user_redemptions: i64,
    pub redeemed_count: i64,
    pub active: bool,
    pub starts_at_unix: Option<i64>,
    pub expires_at_unix: Option<i64>,
    pub created_by: Option<String>,
    pub created_at_unix: i64,
    pub updated_at_unix: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewardCouponRedemptionRecord {
    pub id: String,
    pub coupon_id: String,
    pub code: String,
    pub player_id: String,
    pub wallet_address: String,
    pub amount_raw: String,
    pub status: String,
    pub tx_hash: Option<String>,
    pub created_at_unix: i64,
    pub updated_at_unix: i64,
}

#[derive(Debug, Clone)]
pub struct CreateRewardCouponInput {
    pub code: Option<String>,
    pub description: Option<String>,
    pub amount_raw: u128,
    pub max_global_redemptions: i64,
    pub max_per_user_redemptions: i64,
    pub starts_at_unix: Option<i64>,
    pub expires_at_unix: Option<i64>,
    pub active: bool,
    pub created_by: Option<String>,
    pub metadata: Value,
}

#[derive(Debug, Clone)]
pub struct ReservedRewardCouponRedemption {
    pub redemption_id: Uuid,
    pub coupon_id: Uuid,
    pub code: String,
    pub player_id: Uuid,
    pub wallet_address: String,
    pub amount_raw: u128,
    pub status: String,
    pub tx_hash: Option<String>,
}

#[derive(Debug, Clone)]
pub enum RewardCouponRedemptionState {
    Ready(ReservedRewardCouponRedemption),
    Submitted(ReservedRewardCouponRedemption),
    Claimed(ReservedRewardCouponRedemption),
}

#[derive(Debug, Clone)]
pub struct PreparedRewardClaim {
    pub reward_kind: RewardKind,
    pub player_id: Uuid,
    pub wallet_address: String,
    pub amount_raw: u128,
    pub claim_rows: Vec<PreparedRewardClaimRow>,
    pub metadata: Value,
}

#[derive(Debug, Clone)]
pub struct PreparedRewardClaimRow {
    pub reward_kind: RewardKind,
    pub epoch_key: Option<String>,
    pub tier_level: Option<i32>,
    pub amount_raw: u128,
    pub metadata: Value,
}

#[derive(Debug, Clone)]
pub struct ReservedRewardClaim {
    pub claim_id: Uuid,
    pub reward_kind: RewardKind,
    pub player_id: Uuid,
    pub wallet_address: String,
    pub amount_raw: u128,
    pub claim_rows: Vec<PreparedRewardClaimRow>,
    pub metadata: Value,
    pub status: String,
    pub tx_hash: Option<String>,
    pub expires_at_unix: i64,
}

#[derive(Debug, Clone)]
pub enum RewardClaimSubmissionState {
    Ready(ReservedRewardClaim),
    Submitted(ReservedRewardClaim),
    Claimed(ReservedRewardClaim),
}

#[derive(Debug, Clone)]
struct RewardEventRow {
    player_wallet: String,
    reference_kind: String,
    reference_id: String,
    event_name: String,
    amount_raw: u128,
    created_at_unix: i64,
    week_key: String,
    month_key: String,
}

#[derive(Debug, Clone)]
struct WagerIncrement {
    amount_raw: u128,
    weighted_volume_raw: u128,
    house_edge_bps: u32,
    created_at_unix: i64,
    week_key: String,
    month_key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WagerOutcome {
    Pending,
    Settled,
    Voided,
}

#[derive(Debug, Clone)]
struct WagerReference {
    player_id: Uuid,
    game_kind: String,
    first_created_at_unix: i64,
    first_week_key: String,
    first_month_key: String,
    increments: Vec<WagerIncrement>,
    total_wager_raw: u128,
    last_reserved_total: u128,
    payout_raw: u128,
    outcome: WagerOutcome,
}

#[derive(Debug, Clone)]
struct RewardClaimRow {
    reward_kind: RewardKind,
    epoch_key: Option<String>,
    tier_level: Option<i32>,
    amount_raw: u128,
}

#[derive(Debug, Clone)]
struct UserEpochRaw {
    wager_volume_raw: u128,
    weighted_volume_raw: u128,
    raw_bonus_raw: u128,
    tier_level: u8,
    tier_name: String,
}

#[derive(Debug, Clone)]
struct LevelUpEventRaw {
    tier_level: u8,
    tier_name: String,
    bonus_raw: u128,
    crossed_at_unix: i64,
}

#[derive(Debug, Clone)]
struct UserRewardRaw {
    lifetime_wager_raw: u128,
    wager_7d_raw: u128,
    wager_30d_raw: u128,
    lifetime_weighted_volume_raw: u128,
    weighted_volume_7d_raw: u128,
    weighted_volume_30d_raw: u128,
    rakeback_epochs: BTreeMap<String, UserEpochRaw>,
    weekly_epochs: BTreeMap<String, UserEpochRaw>,
    level_up_events: Vec<LevelUpEventRaw>,
}

#[derive(Debug, Clone)]
struct ReferralLinkRow {
    referred_player_id: Uuid,
    referrer_player_id: Uuid,
    referrer_wallet_address: Option<String>,
    referrer_username: Option<String>,
    created_at_unix: i64,
}

#[derive(Debug, Clone)]
struct RewardsContext {
    current_week_key: String,
    current_month_key: String,
    wallet_to_player: HashMap<String, Uuid>,
    user_raw: HashMap<Uuid, UserRewardRaw>,
    reward_claims: HashMap<Uuid, Vec<RewardClaimRow>>,
    active_reward_claims: HashMap<Uuid, Vec<RewardClaimRow>>,
    referral_links: Vec<ReferralLinkRow>,
    rakeback_scale_bps: HashMap<String, u32>,
    level_up_scale_bps: u32,
    referral_scale_bps: u32,
    weekly_scale_bps: HashMap<String, u32>,
    house_profit_by_player: HashMap<Uuid, i128>,
    house_profit_by_player_since: HashMap<(Uuid, i64), i128>,
    global_lifetime_wager_raw: u128,
    global_lifetime_weighted_volume_raw: u128,
    global_weighted_volume_7d_raw: u128,
    global_weighted_volume_30d_raw: u128,
}

pub async fn get_rewards_state(
    pool: &PgPool,
    wallet_address: &str,
    config: &RewardsConfig,
) -> anyhow::Result<RewardsStateView> {
    let normalized_wallet = accounts::normalize_wallet_address(wallet_address);
    let context = build_rewards_context(pool, config).await?;
    let player_id = context
        .wallet_to_player
        .get(&normalized_wallet)
        .copied()
        .ok_or_else(|| anyhow!("Moros account not found for reward wallet"))?;
    Ok(build_rewards_state_from_context(
        &context,
        config,
        player_id,
        normalized_wallet,
    ))
}

pub async fn prepare_reward_claim(
    pool: &PgPool,
    wallet_address: &str,
    reward_kind: RewardKind,
    config: &RewardsConfig,
) -> anyhow::Result<PreparedRewardClaim> {
    let normalized_wallet = accounts::normalize_wallet_address(wallet_address);
    let context = build_rewards_context(pool, config).await?;
    let player_id = context
        .wallet_to_player
        .get(&normalized_wallet)
        .copied()
        .ok_or_else(|| anyhow!("Moros account not found for reward wallet"))?;
    let state =
        build_rewards_state_from_context(&context, config, player_id, normalized_wallet.clone());
    let claims = context
        .reward_claims
        .get(&player_id)
        .cloned()
        .unwrap_or_default();

    match reward_kind {
        RewardKind::Rakeback => {
            let claimed_epochs: HashMap<String, u128> = claims
                .into_iter()
                .filter(|row| row.reward_kind == RewardKind::Rakeback)
                .filter_map(|row| row.epoch_key.map(|key| (key, row.amount_raw)))
                .collect();
            let mut claim_rows = Vec::new();
            for epoch in &state.rakeback_epochs {
                let claimable_raw = epoch
                    .claimable_raw
                    .parse::<u128>()
                    .context("invalid rakeback claimable amount")?;
                if claimable_raw == 0 || claimed_epochs.contains_key(&epoch.epoch_key) {
                    continue;
                }
                claim_rows.push(PreparedRewardClaimRow {
                    reward_kind,
                    epoch_key: Some(epoch.epoch_key.clone()),
                    tier_level: None,
                    amount_raw: claimable_raw,
                    metadata: serde_json::json!({
                        "wager_volume_raw": epoch.wager_volume_raw,
                        "weighted_volume_raw": epoch.weighted_volume_raw,
                        "raw_rakeback_raw": epoch.raw_bonus_raw,
                        "tier_level": epoch.tier_level,
                        "tier_name": epoch.tier_name,
                        "scale_bps": epoch.scale_bps,
                    }),
                });
            }
            finalize_epoch_claim(player_id, normalized_wallet, reward_kind, claim_rows)
        }
        RewardKind::Weekly => {
            let claimed_epochs: HashMap<String, u128> = claims
                .into_iter()
                .filter(|row| row.reward_kind == RewardKind::Weekly)
                .filter_map(|row| row.epoch_key.map(|key| (key, row.amount_raw)))
                .collect();
            let mut claim_rows = Vec::new();
            for epoch in &state.weekly_epochs {
                let claimable_raw = epoch
                    .claimable_raw
                    .parse::<u128>()
                    .context("invalid weekly claimable amount")?;
                if claimable_raw == 0 || claimed_epochs.contains_key(&epoch.epoch_key) {
                    continue;
                }
                claim_rows.push(PreparedRewardClaimRow {
                    reward_kind,
                    epoch_key: Some(epoch.epoch_key.clone()),
                    tier_level: None,
                    amount_raw: claimable_raw,
                    metadata: serde_json::json!({
                        "wager_volume_raw": epoch.wager_volume_raw,
                        "weighted_volume_raw": epoch.weighted_volume_raw,
                        "raw_bonus_raw": epoch.raw_bonus_raw,
                        "tier_level": epoch.tier_level,
                        "tier_name": epoch.tier_name,
                        "scale_bps": epoch.scale_bps,
                    }),
                });
            }
            finalize_epoch_claim(player_id, normalized_wallet, reward_kind, claim_rows)
        }
        RewardKind::Monthly => Err(anyhow!("monthly bonus rewards are retired")),
        RewardKind::LevelUp => {
            let claimed_tiers: BTreeMap<i32, u128> = claims
                .into_iter()
                .filter(|row| row.reward_kind == RewardKind::LevelUp)
                .filter_map(|row| row.tier_level.map(|level| (level, row.amount_raw)))
                .collect();
            let mut claim_rows = Vec::new();
            for reward in &state.level_up_rewards {
                let claimable_raw = reward
                    .claimable_raw
                    .parse::<u128>()
                    .context("invalid level-up claimable amount")?;
                let tier_level = i32::from(reward.tier_level);
                if claimable_raw == 0 || claimed_tiers.contains_key(&tier_level) {
                    continue;
                }
                claim_rows.push(PreparedRewardClaimRow {
                    reward_kind,
                    epoch_key: None,
                    tier_level: Some(tier_level),
                    amount_raw: claimable_raw,
                    metadata: serde_json::json!({
                        "tier_level": reward.tier_level,
                        "tier_name": reward.tier_name,
                        "bonus_raw": reward.bonus_raw,
                        "scale_bps": reward.scale_bps,
                        "crossed_at_unix": reward.crossed_at_unix,
                    }),
                });
            }
            finalize_epoch_claim(player_id, normalized_wallet, reward_kind, claim_rows)
        }
        RewardKind::Referral => {
            let amount_raw = state
                .referral
                .claimable_raw
                .parse::<u128>()
                .context("invalid referral claimable amount")?;
            if amount_raw == 0 {
                return Err(anyhow!("no referral rewards are currently claimable"));
            }
            Ok(PreparedRewardClaim {
                reward_kind,
                player_id,
                wallet_address: normalized_wallet,
                amount_raw,
                claim_rows: vec![PreparedRewardClaimRow {
                    reward_kind,
                    epoch_key: None,
                    tier_level: None,
                    amount_raw,
                    metadata: serde_json::json!({
                        "referral_rate_bps": state.referral.referral_rate_bps,
                        "referred_users": state.referral.referred_users,
                    }),
                }],
                metadata: serde_json::json!({
                    "reward_kind": reward_kind.as_str(),
                    "referral_rate_bps": state.referral.referral_rate_bps,
                }),
            })
        }
        RewardKind::Coupon => Err(anyhow!(
            "coupon rewards must be redeemed through the coupon endpoint"
        )),
    }
}

fn finalize_epoch_claim(
    player_id: Uuid,
    wallet_address: String,
    reward_kind: RewardKind,
    claim_rows: Vec<PreparedRewardClaimRow>,
) -> anyhow::Result<PreparedRewardClaim> {
    let amount_raw = claim_rows
        .iter()
        .fold(0u128, |total, row| total.saturating_add(row.amount_raw));
    if amount_raw == 0 {
        return Err(anyhow!(
            "no {} rewards are currently claimable",
            reward_kind.as_str()
        ));
    }
    Ok(PreparedRewardClaim {
        reward_kind,
        player_id,
        wallet_address,
        amount_raw,
        metadata: serde_json::json!({
            "reward_kind": reward_kind.as_str(),
            "claim_rows": claim_rows.len(),
        }),
        claim_rows,
    })
}

pub async fn reserve_reward_claim(
    pool: &PgPool,
    wallet_address: &str,
    reward_kind: RewardKind,
    config: &RewardsConfig,
) -> anyhow::Result<ReservedRewardClaim> {
    expire_stale_claim_intents(pool).await?;
    let normalized_wallet = accounts::normalize_wallet_address(wallet_address);
    let player_id = accounts::resolve_player_id_by_wallet(pool, &normalized_wallet)
        .await?
        .ok_or_else(|| anyhow!("Moros account not found for reward wallet"))?;
    let mut tx = pool
        .begin()
        .await
        .context("failed to open reward claim reservation transaction")?;
    lock_reward_claim(&mut tx, player_id, reward_kind).await?;

    if let Some(existing) =
        load_active_claim_intent_for_update(&mut tx, player_id, reward_kind).await?
    {
        if existing.status == "reserved" || existing.tx_hash.is_some() {
            tx.commit()
                .await
                .context("failed to commit existing reward claim reservation")?;
            return Ok(existing);
        }
        return Err(anyhow!(
            "reward claim is already being settled for {}",
            reward_kind.as_str()
        ));
    }

    let prepared = prepare_reward_claim(pool, &normalized_wallet, reward_kind, config).await?;
    if prepared.player_id != player_id {
        return Err(anyhow!("reward claim wallet changed during reservation"));
    }
    let claim_id = Uuid::now_v7();
    let expires_at_unix = runtime::now_unix() + config.claim_reservation_ttl_seconds;
    let claim_rows_json = prepared_claim_rows_json(&prepared.claim_rows);
    sqlx::query(
        r#"
        INSERT INTO reward_claim_intents (
            claim_id,
            player_id,
            wallet_address,
            reward_kind,
            amount_raw,
            claim_rows,
            metadata,
            expires_at
        )
        VALUES (
            $1,
            $2,
            $3,
            $4,
            $5,
            $6::jsonb,
            $7::jsonb,
            TO_TIMESTAMP($8)
        )
        "#,
    )
    .bind(claim_id)
    .bind(player_id)
    .bind(&normalized_wallet)
    .bind(reward_kind.as_str())
    .bind(prepared.amount_raw.to_string())
    .bind(claim_rows_json.to_string())
    .bind(prepared.metadata.to_string())
    .bind(expires_at_unix)
    .execute(&mut *tx)
    .await
    .context("failed to insert reward claim reservation")?;
    tx.commit()
        .await
        .context("failed to commit reward claim reservation")?;

    Ok(ReservedRewardClaim {
        claim_id,
        reward_kind,
        player_id,
        wallet_address: normalized_wallet,
        amount_raw: prepared.amount_raw,
        claim_rows: prepared.claim_rows,
        metadata: prepared.metadata,
        status: "reserved".to_string(),
        tx_hash: None,
        expires_at_unix,
    })
}

pub async fn begin_reward_claim_submission(
    pool: &PgPool,
    claim_id: Uuid,
    wallet_address: &str,
    reward_kind: RewardKind,
) -> anyhow::Result<RewardClaimSubmissionState> {
    expire_stale_claim_intents(pool).await?;
    let normalized_wallet = accounts::normalize_wallet_address(wallet_address);
    let player_id = accounts::resolve_player_id_by_wallet(pool, &normalized_wallet)
        .await?
        .ok_or_else(|| anyhow!("Moros account not found for reward wallet"))?;
    let mut tx = pool
        .begin()
        .await
        .context("failed to open reward claim submission transaction")?;
    lock_reward_claim(&mut tx, player_id, reward_kind).await?;
    let mut reserved = load_claim_intent_for_update(&mut tx, claim_id)
        .await?
        .ok_or_else(|| anyhow!("reward claim reservation not found"))?;
    if reserved.player_id != player_id
        || reserved.reward_kind != reward_kind
        || !accounts::normalize_wallet_address(&reserved.wallet_address).eq(&normalized_wallet)
    {
        return Err(anyhow!("reward claim reservation does not match request"));
    }

    let state = match reserved.status.as_str() {
        "reserved" => {
            if reserved.expires_at_unix < runtime::now_unix() {
                sqlx::query(
                    r#"
                    UPDATE reward_claim_intents
                    SET status = 'expired', updated_at = NOW()
                    WHERE claim_id = $1
                    "#,
                )
                .bind(claim_id)
                .execute(&mut *tx)
                .await
                .context("failed to expire reward claim reservation")?;
                return Err(anyhow!("reward claim reservation expired"));
            }
            sqlx::query(
                r#"
                UPDATE reward_claim_intents
                SET status = 'submitted',
                    submitted_at = COALESCE(submitted_at, NOW()),
                    updated_at = NOW()
                WHERE claim_id = $1
                "#,
            )
            .bind(claim_id)
            .execute(&mut *tx)
            .await
            .context("failed to mark reward claim as submitting")?;
            reserved.status = "submitted".to_string();
            RewardClaimSubmissionState::Ready(reserved)
        }
        "submitted" => {
            if reserved.tx_hash.is_some() {
                RewardClaimSubmissionState::Submitted(reserved)
            } else {
                return Err(anyhow!("reward claim is already being submitted"));
            }
        }
        "claimed" => RewardClaimSubmissionState::Claimed(reserved),
        "failed" => return Err(anyhow!("reward claim reservation failed")),
        "expired" => return Err(anyhow!("reward claim reservation expired")),
        _ => return Err(anyhow!("reward claim reservation has invalid status")),
    };
    tx.commit()
        .await
        .context("failed to commit reward claim submission state")?;
    Ok(state)
}

pub async fn mark_reward_claim_submitted(
    pool: &PgPool,
    claim_id: Uuid,
    tx_hash: &str,
) -> anyhow::Result<ReservedRewardClaim> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to open reward claim tx-hash transaction")?;
    let mut reserved = load_claim_intent_for_update(&mut tx, claim_id)
        .await?
        .ok_or_else(|| anyhow!("reward claim reservation not found"))?;
    if reserved.status == "claimed" {
        tx.commit()
            .await
            .context("failed to commit claimed reward claim tx-hash transaction")?;
        return Ok(reserved);
    }
    if reserved.status != "submitted" {
        return Err(anyhow!("reward claim reservation is not submitted"));
    }
    sqlx::query(
        r#"
        UPDATE reward_claim_intents
        SET tx_hash = $2,
            submitted_at = COALESCE(submitted_at, NOW()),
            updated_at = NOW()
        WHERE claim_id = $1
        "#,
    )
    .bind(claim_id)
    .bind(tx_hash)
    .execute(&mut *tx)
    .await
    .context("failed to store reward claim tx hash")?;
    reserved.tx_hash = Some(tx_hash.to_string());
    tx.commit()
        .await
        .context("failed to commit reward claim tx hash")?;
    Ok(reserved)
}

pub async fn mark_reward_claim_confirmed(
    pool: &PgPool,
    claim_id: Uuid,
    tx_hash: &str,
) -> anyhow::Result<ReservedRewardClaim> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to open reward claim confirmation transaction")?;
    let mut reserved = load_claim_intent_for_update(&mut tx, claim_id)
        .await?
        .ok_or_else(|| anyhow!("reward claim reservation not found"))?;
    if reserved.status == "claimed" {
        tx.commit()
            .await
            .context("failed to commit already confirmed reward claim")?;
        return Ok(reserved);
    }
    if reserved.status != "submitted" {
        return Err(anyhow!("reward claim reservation is not submitted"));
    }
    if let Some(stored_tx_hash) = reserved.tx_hash.as_deref() {
        if stored_tx_hash != tx_hash {
            return Err(anyhow!("reward claim tx hash does not match reservation"));
        }
    }
    for claim_row in &reserved.claim_rows {
        sqlx::query(
            r#"
            INSERT INTO reward_claims (
                id,
                claim_id,
                player_id,
                reward_kind,
                epoch_key,
                tier_level,
                amount_raw,
                tx_hash,
                metadata
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9::jsonb)
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(claim_id)
        .bind(reserved.player_id)
        .bind(claim_row.reward_kind.as_str())
        .bind(claim_row.epoch_key.as_deref())
        .bind(claim_row.tier_level)
        .bind(claim_row.amount_raw.to_string())
        .bind(tx_hash)
        .bind(claim_row.metadata.to_string())
        .execute(&mut *tx)
        .await
        .context("failed to insert confirmed reward claim")?;
    }
    sqlx::query(
        r#"
        UPDATE reward_claim_intents
        SET status = 'claimed',
            tx_hash = COALESCE(tx_hash, $2),
            confirmed_at = COALESCE(confirmed_at, NOW()),
            updated_at = NOW()
        WHERE claim_id = $1
        "#,
    )
    .bind(claim_id)
    .bind(tx_hash)
    .execute(&mut *tx)
    .await
    .context("failed to mark reward claim confirmed")?;
    reserved.status = "claimed".to_string();
    reserved.tx_hash = Some(tx_hash.to_string());
    tx.commit()
        .await
        .context("failed to commit confirmed reward claim")?;
    Ok(reserved)
}

pub async fn mark_reward_claim_failed(
    pool: &PgPool,
    claim_id: Uuid,
    error: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        UPDATE reward_claim_intents
        SET status = 'failed',
            error = $2,
            failed_at = COALESCE(failed_at, NOW()),
            updated_at = NOW()
        WHERE claim_id = $1
          AND status <> 'claimed'
        "#,
    )
    .bind(claim_id)
    .bind(error)
    .execute(pool)
    .await
    .context("failed to mark reward claim failed")?;
    Ok(())
}

pub fn generate_reward_coupon_code() -> String {
    let raw = Uuid::now_v7().simple().to_string().to_ascii_uppercase();
    format!("MOROS-{}-{}-{}", &raw[0..6], &raw[6..12], &raw[12..18])
}

pub fn normalize_reward_coupon_code(code: &str) -> anyhow::Result<String> {
    let normalized = code
        .trim()
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>()
        .to_ascii_uppercase();
    if !(4..=64).contains(&normalized.len()) {
        return Err(anyhow!("coupon code must be 4 to 64 characters"));
    }
    if !normalized
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '-' || character == '_')
    {
        return Err(anyhow!(
            "coupon code can only contain letters, digits, hyphen, or underscore"
        ));
    }
    Ok(normalized)
}

pub async fn create_reward_coupon(
    pool: &PgPool,
    input: CreateRewardCouponInput,
) -> anyhow::Result<RewardCouponRecord> {
    if input.amount_raw == 0 {
        return Err(anyhow!("coupon amount must be greater than zero"));
    }
    if input.max_global_redemptions <= 0 {
        return Err(anyhow!(
            "coupon global redemption cap must be greater than zero"
        ));
    }
    if input.max_per_user_redemptions <= 0 {
        return Err(anyhow!(
            "coupon per-user redemption cap must be greater than zero"
        ));
    }
    if let (Some(starts_at), Some(expires_at)) = (input.starts_at_unix, input.expires_at_unix) {
        if starts_at >= expires_at {
            return Err(anyhow!("coupon expiry must be after start time"));
        }
    }
    let code = match input.code.as_deref() {
        Some(code) => normalize_reward_coupon_code(code)?,
        None => generate_reward_coupon_code(),
    };
    let id = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO reward_coupons (
            id,
            code,
            description,
            amount_raw,
            max_global_redemptions,
            max_per_user_redemptions,
            active,
            starts_at,
            expires_at,
            created_by,
            metadata
        )
        VALUES (
            $1,
            $2,
            $3,
            $4,
            $5,
            $6,
            $7,
            CASE WHEN $8::BIGINT IS NULL THEN NULL ELSE TO_TIMESTAMP($8) END,
            CASE WHEN $9::BIGINT IS NULL THEN NULL ELSE TO_TIMESTAMP($9) END,
            $10,
            $11::jsonb
        )
        "#,
    )
    .bind(id)
    .bind(&code)
    .bind(input.description.as_deref())
    .bind(input.amount_raw.to_string())
    .bind(input.max_global_redemptions)
    .bind(input.max_per_user_redemptions)
    .bind(input.active)
    .bind(input.starts_at_unix)
    .bind(input.expires_at_unix)
    .bind(input.created_by.as_deref())
    .bind(input.metadata.to_string())
    .execute(pool)
    .await
    .context("failed to create reward coupon")?;
    load_reward_coupon_by_id(pool, id)
        .await?
        .ok_or_else(|| anyhow!("created reward coupon was not found"))
}

pub async fn reserve_reward_coupon_redemption(
    pool: &PgPool,
    player_id: Uuid,
    wallet_address: &str,
    code: &str,
) -> anyhow::Result<RewardCouponRedemptionState> {
    let normalized_wallet = accounts::normalize_wallet_address(wallet_address);
    let code = normalize_reward_coupon_code(code)?;
    let mut tx = pool
        .begin()
        .await
        .context("failed to open coupon redemption transaction")?;
    let coupon = load_reward_coupon_for_update(&mut tx, &code)
        .await?
        .ok_or_else(|| anyhow!("coupon code not found"))?;
    validate_coupon_window(&coupon)?;

    if let Some(existing) =
        load_active_coupon_redemption_for_update(&mut tx, coupon.id, player_id).await?
    {
        tx.commit()
            .await
            .context("failed to commit existing coupon redemption")?;
        return Ok(match existing.status.as_str() {
            "claimed" => RewardCouponRedemptionState::Claimed(existing),
            "submitted" => RewardCouponRedemptionState::Submitted(existing),
            _ => RewardCouponRedemptionState::Ready(existing),
        });
    }

    let global_redemptions = count_coupon_redemptions(&mut tx, coupon.id, None).await?;
    if global_redemptions >= coupon.max_global_redemptions {
        return Err(anyhow!("coupon redemption cap reached"));
    }
    let user_redemptions = count_coupon_redemptions(&mut tx, coupon.id, Some(player_id)).await?;
    if user_redemptions >= coupon.max_per_user_redemptions {
        return Err(anyhow!("coupon has already been redeemed by this user"));
    }

    let redemption_id = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO reward_coupon_redemptions (
            id,
            coupon_id,
            player_id,
            wallet_address,
            amount_raw,
            status,
            metadata
        )
        VALUES ($1, $2, $3, $4, $5, 'reserved', $6::jsonb)
        "#,
    )
    .bind(redemption_id)
    .bind(coupon.id)
    .bind(player_id)
    .bind(&normalized_wallet)
    .bind(coupon.amount_raw.to_string())
    .bind(
        serde_json::json!({
            "coupon_code": coupon.code,
            "description": coupon.description,
        })
        .to_string(),
    )
    .execute(&mut *tx)
    .await
    .context("failed to reserve coupon redemption")?;
    tx.commit()
        .await
        .context("failed to commit coupon redemption reservation")?;

    Ok(RewardCouponRedemptionState::Ready(
        ReservedRewardCouponRedemption {
            redemption_id,
            coupon_id: coupon.id,
            code: coupon.code,
            player_id,
            wallet_address: normalized_wallet,
            amount_raw: coupon.amount_raw,
            status: "reserved".to_string(),
            tx_hash: None,
        },
    ))
}

pub async fn mark_reward_coupon_redemption_submitted(
    pool: &PgPool,
    redemption_id: Uuid,
    tx_hash: &str,
) -> anyhow::Result<ReservedRewardCouponRedemption> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to open coupon redemption tx transaction")?;
    let mut redemption = load_coupon_redemption_for_update(&mut tx, redemption_id)
        .await?
        .ok_or_else(|| anyhow!("coupon redemption not found"))?;
    if redemption.status == "claimed" {
        tx.commit()
            .await
            .context("failed to commit claimed coupon redemption transaction")?;
        return Ok(redemption);
    }
    if redemption.status != "reserved" && redemption.status != "submitted" {
        return Err(anyhow!("coupon redemption is not ready to submit"));
    }
    sqlx::query(
        r#"
        UPDATE reward_coupon_redemptions
        SET status = 'submitted',
            tx_hash = $2,
            submitted_at = COALESCE(submitted_at, NOW()),
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(redemption_id)
    .bind(tx_hash)
    .execute(&mut *tx)
    .await
    .context("failed to mark coupon redemption submitted")?;
    redemption.status = "submitted".to_string();
    redemption.tx_hash = Some(tx_hash.to_string());
    tx.commit()
        .await
        .context("failed to commit coupon redemption tx hash")?;
    Ok(redemption)
}

pub async fn mark_reward_coupon_redemption_confirmed(
    pool: &PgPool,
    redemption_id: Uuid,
    tx_hash: &str,
) -> anyhow::Result<ReservedRewardCouponRedemption> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to open coupon redemption confirmation transaction")?;
    let mut redemption = load_coupon_redemption_for_update(&mut tx, redemption_id)
        .await?
        .ok_or_else(|| anyhow!("coupon redemption not found"))?;
    if redemption.status == "claimed" {
        tx.commit()
            .await
            .context("failed to commit already claimed coupon redemption")?;
        return Ok(redemption);
    }
    if redemption.status != "submitted" {
        return Err(anyhow!("coupon redemption is not submitted"));
    }
    if let Some(stored_tx_hash) = redemption.tx_hash.as_deref() {
        if stored_tx_hash != tx_hash {
            return Err(anyhow!(
                "coupon redemption tx hash does not match reservation"
            ));
        }
    }
    sqlx::query(
        r#"
        INSERT INTO reward_claims (
            id,
            claim_id,
            player_id,
            reward_kind,
            epoch_key,
            tier_level,
            amount_raw,
            tx_hash,
            metadata
        )
        VALUES ($1, $2, $3, 'coupon', NULL, NULL, $4, $5, $6::jsonb)
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(redemption_id)
    .bind(redemption.player_id)
    .bind(redemption.amount_raw.to_string())
    .bind(tx_hash)
    .bind(
        serde_json::json!({
            "coupon_id": redemption.coupon_id.to_string(),
            "coupon_code": redemption.code,
            "redemption_id": redemption.redemption_id.to_string(),
        })
        .to_string(),
    )
    .execute(&mut *tx)
    .await
    .context("failed to insert coupon reward claim")?;
    sqlx::query(
        r#"
        UPDATE reward_coupon_redemptions
        SET status = 'claimed',
            tx_hash = COALESCE(tx_hash, $2),
            confirmed_at = COALESCE(confirmed_at, NOW()),
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(redemption_id)
    .bind(tx_hash)
    .execute(&mut *tx)
    .await
    .context("failed to mark coupon redemption confirmed")?;
    redemption.status = "claimed".to_string();
    redemption.tx_hash = Some(tx_hash.to_string());
    tx.commit()
        .await
        .context("failed to commit confirmed coupon redemption")?;
    Ok(redemption)
}

pub async fn mark_reward_coupon_redemption_failed(
    pool: &PgPool,
    redemption_id: Uuid,
    error: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        UPDATE reward_coupon_redemptions
        SET status = 'failed',
            error = $2,
            failed_at = COALESCE(failed_at, NOW()),
            updated_at = NOW()
        WHERE id = $1
          AND status <> 'claimed'
        "#,
    )
    .bind(redemption_id)
    .bind(error)
    .execute(pool)
    .await
    .context("failed to mark coupon redemption failed")?;
    Ok(())
}

#[derive(Debug, Clone)]
struct RewardCouponRow {
    id: Uuid,
    code: String,
    description: Option<String>,
    amount_raw: u128,
    max_global_redemptions: i64,
    max_per_user_redemptions: i64,
    redeemed_count: i64,
    active: bool,
    starts_at_unix: Option<i64>,
    expires_at_unix: Option<i64>,
    created_by: Option<String>,
    created_at_unix: i64,
    updated_at_unix: i64,
}

async fn load_reward_coupon_by_id(
    pool: &PgPool,
    coupon_id: Uuid,
) -> anyhow::Result<Option<RewardCouponRecord>> {
    let sql = reward_coupon_select_sql("WHERE c.id = $1", false);
    let row = sqlx::query(&sql)
        .bind(coupon_id)
        .fetch_optional(pool)
        .await
        .context("failed to load reward coupon")?;
    row.map(hydrate_reward_coupon_row)
        .transpose()
        .map(|row| row.map(reward_coupon_record_from_row))
}

async fn load_reward_coupon_for_update(
    tx: &mut Transaction<'_, Postgres>,
    code: &str,
) -> anyhow::Result<Option<RewardCouponRow>> {
    let sql = reward_coupon_select_sql("WHERE c.code = $1", true);
    let row = sqlx::query(&sql)
        .bind(code)
        .fetch_optional(&mut **tx)
        .await
        .context("failed to load reward coupon for update")?;
    row.map(hydrate_reward_coupon_row).transpose()
}

fn reward_coupon_select_sql(where_clause: &str, for_update: bool) -> String {
    format!(
        r#"
        SELECT
            c.id,
            c.code,
            c.description,
            c.amount_raw,
            c.max_global_redemptions,
            c.max_per_user_redemptions,
            c.active,
            EXTRACT(EPOCH FROM c.starts_at)::BIGINT AS starts_at_unix,
            EXTRACT(EPOCH FROM c.expires_at)::BIGINT AS expires_at_unix,
            c.created_by,
            EXTRACT(EPOCH FROM c.created_at)::BIGINT AS created_at_unix,
            EXTRACT(EPOCH FROM c.updated_at)::BIGINT AS updated_at_unix,
            COALESCE((
                SELECT COUNT(*)::BIGINT
                FROM reward_coupon_redemptions r
                WHERE r.coupon_id = c.id
                  AND r.status IN ('reserved', 'submitted', 'claimed')
            ), 0) AS redeemed_count
        FROM reward_coupons c
        {where_clause}
        {}
        "#,
        if for_update { "FOR UPDATE" } else { "" }
    )
}

fn hydrate_reward_coupon_row(row: PgRow) -> anyhow::Result<RewardCouponRow> {
    Ok(RewardCouponRow {
        id: row.try_get("id").context("missing coupon id")?,
        code: row.try_get("code").context("missing coupon code")?,
        description: row.try_get("description").ok().flatten(),
        amount_raw: parse_raw_u128(
            &row.try_get::<String, _>("amount_raw")
                .context("missing coupon amount")?,
        )?,
        max_global_redemptions: row
            .try_get("max_global_redemptions")
            .context("missing coupon global cap")?,
        max_per_user_redemptions: row
            .try_get("max_per_user_redemptions")
            .context("missing coupon user cap")?,
        redeemed_count: row
            .try_get("redeemed_count")
            .context("missing coupon redeemed count")?,
        active: row
            .try_get("active")
            .context("missing coupon active state")?,
        starts_at_unix: row.try_get("starts_at_unix").ok().flatten(),
        expires_at_unix: row.try_get("expires_at_unix").ok().flatten(),
        created_by: row.try_get("created_by").ok().flatten(),
        created_at_unix: row.try_get("created_at_unix").unwrap_or_default(),
        updated_at_unix: row.try_get("updated_at_unix").unwrap_or_default(),
    })
}

fn reward_coupon_record_from_row(row: RewardCouponRow) -> RewardCouponRecord {
    RewardCouponRecord {
        id: row.id.to_string(),
        code: row.code,
        description: row.description,
        amount_raw: row.amount_raw.to_string(),
        max_global_redemptions: row.max_global_redemptions,
        max_per_user_redemptions: row.max_per_user_redemptions,
        redeemed_count: row.redeemed_count,
        active: row.active,
        starts_at_unix: row.starts_at_unix,
        expires_at_unix: row.expires_at_unix,
        created_by: row.created_by,
        created_at_unix: row.created_at_unix,
        updated_at_unix: row.updated_at_unix,
    }
}

fn validate_coupon_window(coupon: &RewardCouponRow) -> anyhow::Result<()> {
    if !coupon.active {
        return Err(anyhow!("coupon is not active"));
    }
    let now = runtime::now_unix();
    if let Some(starts_at) = coupon.starts_at_unix {
        if now < starts_at {
            return Err(anyhow!("coupon is not active yet"));
        }
    }
    if let Some(expires_at) = coupon.expires_at_unix {
        if now >= expires_at {
            return Err(anyhow!("coupon has expired"));
        }
    }
    Ok(())
}

async fn count_coupon_redemptions(
    tx: &mut Transaction<'_, Postgres>,
    coupon_id: Uuid,
    player_id: Option<Uuid>,
) -> anyhow::Result<i64> {
    let row = if let Some(player_id) = player_id {
        sqlx::query(
            r#"
            SELECT COUNT(*)::BIGINT AS count
            FROM reward_coupon_redemptions
            WHERE coupon_id = $1
              AND player_id = $2
              AND status IN ('reserved', 'submitted', 'claimed')
            "#,
        )
        .bind(coupon_id)
        .bind(player_id)
        .fetch_one(&mut **tx)
        .await
        .context("failed to count player coupon redemptions")?
    } else {
        sqlx::query(
            r#"
            SELECT COUNT(*)::BIGINT AS count
            FROM reward_coupon_redemptions
            WHERE coupon_id = $1
              AND status IN ('reserved', 'submitted', 'claimed')
            "#,
        )
        .bind(coupon_id)
        .fetch_one(&mut **tx)
        .await
        .context("failed to count coupon redemptions")?
    };
    row.try_get("count")
        .context("coupon redemption count missing")
}

async fn load_active_coupon_redemption_for_update(
    tx: &mut Transaction<'_, Postgres>,
    coupon_id: Uuid,
    player_id: Uuid,
) -> anyhow::Result<Option<ReservedRewardCouponRedemption>> {
    let row = sqlx::query(
        r#"
        SELECT
            r.id,
            r.coupon_id,
            c.code,
            r.player_id,
            r.wallet_address,
            r.amount_raw,
            r.status,
            r.tx_hash
        FROM reward_coupon_redemptions r
        INNER JOIN reward_coupons c ON c.id = r.coupon_id
        WHERE r.coupon_id = $1
          AND r.player_id = $2
          AND r.status IN ('reserved', 'submitted')
        ORDER BY r.updated_at DESC
        LIMIT 1
        FOR UPDATE
        "#,
    )
    .bind(coupon_id)
    .bind(player_id)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to load active coupon redemption")?;
    row.map(hydrate_reserved_coupon_redemption).transpose()
}

async fn load_coupon_redemption_for_update(
    tx: &mut Transaction<'_, Postgres>,
    redemption_id: Uuid,
) -> anyhow::Result<Option<ReservedRewardCouponRedemption>> {
    let row = sqlx::query(
        r#"
        SELECT
            r.id,
            r.coupon_id,
            c.code,
            r.player_id,
            r.wallet_address,
            r.amount_raw,
            r.status,
            r.tx_hash
        FROM reward_coupon_redemptions r
        INNER JOIN reward_coupons c ON c.id = r.coupon_id
        WHERE r.id = $1
        FOR UPDATE
        "#,
    )
    .bind(redemption_id)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to load coupon redemption")?;
    row.map(hydrate_reserved_coupon_redemption).transpose()
}

fn hydrate_reserved_coupon_redemption(
    row: PgRow,
) -> anyhow::Result<ReservedRewardCouponRedemption> {
    Ok(ReservedRewardCouponRedemption {
        redemption_id: row.try_get("id").context("missing coupon redemption id")?,
        coupon_id: row
            .try_get("coupon_id")
            .context("missing coupon redemption coupon_id")?,
        code: row
            .try_get("code")
            .context("missing coupon redemption code")?,
        player_id: row
            .try_get("player_id")
            .context("missing coupon redemption player_id")?,
        wallet_address: row
            .try_get("wallet_address")
            .context("missing coupon redemption wallet")?,
        amount_raw: parse_raw_u128(
            &row.try_get::<String, _>("amount_raw")
                .context("missing coupon redemption amount")?,
        )?,
        status: row
            .try_get("status")
            .context("missing coupon redemption status")?,
        tx_hash: row.try_get("tx_hash").ok().flatten(),
    })
}

async fn expire_stale_claim_intents(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        UPDATE reward_claim_intents
        SET status = 'expired',
            updated_at = NOW()
        WHERE status = 'reserved'
          AND expires_at <= NOW()
        "#,
    )
    .execute(pool)
    .await
    .context("failed to expire stale reward claim reservations")?;
    Ok(())
}

async fn lock_reward_claim(
    tx: &mut Transaction<'_, Postgres>,
    player_id: Uuid,
    reward_kind: RewardKind,
) -> anyhow::Result<()> {
    let lock_key = format!("reward_claim:{player_id}:{}", reward_kind.as_str());
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1::text, 0))")
        .bind(lock_key)
        .execute(&mut **tx)
        .await
        .context("failed to lock reward claim")?;
    Ok(())
}

async fn load_active_claim_intent_for_update(
    tx: &mut Transaction<'_, Postgres>,
    player_id: Uuid,
    reward_kind: RewardKind,
) -> anyhow::Result<Option<ReservedRewardClaim>> {
    let row = sqlx::query(
        r#"
        SELECT
            claim_id,
            player_id,
            wallet_address,
            reward_kind,
            amount_raw,
            claim_rows,
            metadata,
            status,
            tx_hash,
            EXTRACT(EPOCH FROM expires_at)::BIGINT AS expires_at_unix
        FROM reward_claim_intents
        WHERE player_id = $1
          AND reward_kind = $2
          AND status IN ('reserved', 'submitted')
        ORDER BY updated_at DESC
        LIMIT 1
        FOR UPDATE
        "#,
    )
    .bind(player_id)
    .bind(reward_kind.as_str())
    .fetch_optional(&mut **tx)
    .await
    .context("failed to load active reward claim reservation")?;
    row.map(hydrate_reserved_reward_claim).transpose()
}

async fn load_claim_intent_for_update(
    tx: &mut Transaction<'_, Postgres>,
    claim_id: Uuid,
) -> anyhow::Result<Option<ReservedRewardClaim>> {
    let row = sqlx::query(
        r#"
        SELECT
            claim_id,
            player_id,
            wallet_address,
            reward_kind,
            amount_raw,
            claim_rows,
            metadata,
            status,
            tx_hash,
            EXTRACT(EPOCH FROM expires_at)::BIGINT AS expires_at_unix
        FROM reward_claim_intents
        WHERE claim_id = $1
        FOR UPDATE
        "#,
    )
    .bind(claim_id)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to load reward claim reservation")?;
    row.map(hydrate_reserved_reward_claim).transpose()
}

fn hydrate_reserved_reward_claim(row: PgRow) -> anyhow::Result<ReservedRewardClaim> {
    let reward_kind = row
        .try_get::<String, _>("reward_kind")
        .context("missing reserved reward kind")
        .ok()
        .and_then(|value| RewardKind::parse(&value))
        .ok_or_else(|| anyhow!("invalid reserved reward claim kind"))?;
    let claim_rows_value: Value = row
        .try_get("claim_rows")
        .context("missing reserved reward claim rows")?;
    let metadata = row
        .try_get("metadata")
        .context("missing reserved reward claim metadata")?;
    Ok(ReservedRewardClaim {
        claim_id: row.try_get("claim_id").context("missing claim_id")?,
        player_id: row
            .try_get("player_id")
            .context("missing reserved reward player_id")?,
        wallet_address: row
            .try_get("wallet_address")
            .context("missing reserved reward wallet_address")?,
        reward_kind,
        amount_raw: parse_raw_u128(
            &row.try_get::<String, _>("amount_raw")
                .context("missing reserved reward amount_raw")?,
        )?,
        claim_rows: parse_prepared_claim_rows_value(&claim_rows_value)?,
        metadata,
        status: row
            .try_get("status")
            .context("missing reward claim status")?,
        tx_hash: row.try_get("tx_hash").ok().flatten(),
        expires_at_unix: row
            .try_get("expires_at_unix")
            .context("missing reserved reward expires_at_unix")?,
    })
}

fn prepared_claim_rows_json(claim_rows: &[PreparedRewardClaimRow]) -> Value {
    Value::Array(
        claim_rows
            .iter()
            .map(|row| {
                serde_json::json!({
                    "reward_kind": row.reward_kind.as_str(),
                    "epoch_key": row.epoch_key.as_deref(),
                    "tier_level": row.tier_level,
                    "amount_raw": row.amount_raw.to_string(),
                    "metadata": row.metadata.clone(),
                })
            })
            .collect(),
    )
}

fn parse_prepared_claim_rows_value(value: &Value) -> anyhow::Result<Vec<PreparedRewardClaimRow>> {
    let rows = value
        .as_array()
        .ok_or_else(|| anyhow!("reward claim rows must be an array"))?;
    rows.iter()
        .map(|row| {
            let reward_kind = row
                .get("reward_kind")
                .and_then(Value::as_str)
                .and_then(RewardKind::parse)
                .ok_or_else(|| anyhow!("invalid reward claim row kind"))?;
            let amount_raw = parse_raw_u128(
                row.get("amount_raw")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow!("reward claim row amount is missing"))?,
            )?;
            Ok(PreparedRewardClaimRow {
                reward_kind,
                epoch_key: row
                    .get("epoch_key")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                tier_level: row
                    .get("tier_level")
                    .and_then(Value::as_i64)
                    .map(i32::try_from)
                    .transpose()
                    .context("reward claim tier level is invalid")?,
                amount_raw,
                metadata: row.get("metadata").cloned().unwrap_or(Value::Null),
            })
        })
        .collect()
}

pub async fn bind_referrer(
    pool: &PgPool,
    wallet_address: &str,
    referrer_input: &str,
) -> anyhow::Result<ReferralBindingView> {
    let normalized_wallet = accounts::normalize_wallet_address(wallet_address);
    let referred_player_id = accounts::resolve_player_id_by_wallet(pool, &normalized_wallet)
        .await?
        .ok_or_else(|| anyhow!("wallet is not linked to a Moros user"))?;
    let existing = get_referral_link_for_referred(pool, referred_player_id).await?;
    if existing.is_some() {
        return Err(anyhow!(
            "referrer has already been set for this Moros account"
        ));
    }

    let referrer = resolve_referrer_target(pool, referrer_input)
        .await?
        .ok_or_else(|| anyhow!("referrer not found"))?;
    if referrer.referrer_player_id == referred_player_id {
        return Err(anyhow!("self-referrals are not allowed"));
    }

    sqlx::query(
        r#"
        INSERT INTO reward_referrals (
            referred_player_id,
            referrer_player_id,
            linked_by_wallet,
            metadata
        )
        VALUES ($1, $2, $3, $4::jsonb)
        "#,
    )
    .bind(referred_player_id)
    .bind(referrer.referrer_player_id)
    .bind(&normalized_wallet)
    .bind(
        serde_json::json!({
            "referrer_input": referrer_input,
        })
        .to_string(),
    )
    .execute(pool)
    .await
    .context("failed to insert reward referral link")?;

    Ok(ReferralBindingView {
        referrer_wallet_address: referrer.referrer_wallet_address,
        referrer_username: referrer.referrer_username,
        linked_at_unix: runtime::now_unix(),
    })
}

async fn build_rewards_context(
    pool: &PgPool,
    config: &RewardsConfig,
) -> anyhow::Result<RewardsContext> {
    let wallet_to_player = load_wallet_to_player_map(pool).await?;
    let reward_events = load_reward_event_rows(pool).await?;
    let reward_claims = load_reward_claims(pool).await?;
    let active_reward_claims = load_active_reward_claims(pool).await?;
    let referral_links = load_referral_links(pool).await?;
    let epoch_keys = load_current_epoch_keys(pool).await?;
    let now_unix = runtime::now_unix();

    let mut references: HashMap<(Uuid, String, String), WagerReference> = HashMap::new();
    for event in reward_events {
        let normalized_wallet = accounts::normalize_wallet_address(&event.player_wallet);
        let Some(player_id) = wallet_to_player.get(&normalized_wallet).copied() else {
            continue;
        };
        let key = (
            player_id,
            event.reference_kind.clone(),
            event.reference_id.clone(),
        );
        let reference = references.entry(key).or_insert_with(|| WagerReference {
            player_id,
            game_kind: event.reference_kind.clone(),
            first_created_at_unix: event.created_at_unix,
            first_week_key: event.week_key.clone(),
            first_month_key: event.month_key.clone(),
            increments: Vec::new(),
            total_wager_raw: 0,
            last_reserved_total: 0,
            payout_raw: 0,
            outcome: WagerOutcome::Pending,
        });
        if event.created_at_unix < reference.first_created_at_unix {
            reference.first_created_at_unix = event.created_at_unix;
            reference.first_week_key = event.week_key.clone();
            reference.first_month_key = event.month_key.clone();
        }
        match event.event_name.as_str() {
            "HandReserved" => {
                let incremental = event
                    .amount_raw
                    .saturating_sub(reference.last_reserved_total);
                reference.last_reserved_total = event.amount_raw;
                if incremental > 0 {
                    let counted = incremental.min(config.max_counted_wager_per_bet_raw);
                    let house_edge_bps = reward_house_edge_bps(config, &reference.game_kind);
                    let weighted_volume_raw = mul_bps(counted, house_edge_bps);
                    reference.total_wager_raw = reference.total_wager_raw.saturating_add(counted);
                    reference.increments.push(WagerIncrement {
                        amount_raw: counted,
                        weighted_volume_raw,
                        house_edge_bps,
                        created_at_unix: event.created_at_unix,
                        week_key: event.week_key,
                        month_key: event.month_key,
                    });
                }
            }
            "HandSettled" => {
                reference.payout_raw = event.amount_raw;
                reference.outcome = WagerOutcome::Settled;
            }
            "HandVoided" => {
                reference.payout_raw = event.amount_raw;
                reference.outcome = WagerOutcome::Voided;
            }
            _ => {}
        }
    }

    let mut increments_by_player: HashMap<Uuid, Vec<WagerIncrement>> = HashMap::new();
    let mut house_profit_by_player: HashMap<Uuid, i128> = HashMap::new();
    let mut house_profit_by_week: HashMap<String, i128> = HashMap::new();
    let mut house_profit_by_month: HashMap<String, i128> = HashMap::new();
    let mut house_profit_by_player_since: HashMap<(Uuid, i64), i128> = HashMap::new();
    let referral_link_by_referred: HashMap<Uuid, &ReferralLinkRow> = referral_links
        .iter()
        .map(|link| (link.referred_player_id, link))
        .collect();

    for reference in references.into_values() {
        if reference.outcome != WagerOutcome::Settled || reference.total_wager_raw == 0 {
            continue;
        }
        increments_by_player
            .entry(reference.player_id)
            .or_default()
            .extend(reference.increments.clone());
        let profit = checked_profit(reference.total_wager_raw, reference.payout_raw)?;
        *house_profit_by_player
            .entry(reference.player_id)
            .or_default() += profit;
        *house_profit_by_week
            .entry(reference.first_week_key.clone())
            .or_default() += profit;
        *house_profit_by_month
            .entry(reference.first_month_key.clone())
            .or_default() += profit;
        if let Some(link) = referral_link_by_referred.get(&reference.player_id) {
            if reference.first_created_at_unix >= link.created_at_unix {
                *house_profit_by_player_since
                    .entry((reference.player_id, link.created_at_unix))
                    .or_default() += profit;
            }
        }
    }

    let mut user_raw = HashMap::new();
    let mut weekly_raw_totals: HashMap<String, u128> = HashMap::new();
    let mut rakeback_raw_totals: HashMap<String, u128> = HashMap::new();
    let mut total_raw_level_up = 0u128;
    let mut total_house_profit = 0i128;
    let mut global_lifetime_wager_raw = 0u128;
    let mut global_lifetime_weighted_volume_raw = 0u128;
    let mut global_weighted_volume_7d_raw = 0u128;
    let mut global_weighted_volume_30d_raw = 0u128;

    for value in house_profit_by_player.values() {
        total_house_profit += *value;
    }

    for (player_id, mut increments) in increments_by_player {
        increments.sort_by(|left, right| {
            left.created_at_unix
                .cmp(&right.created_at_unix)
                .then(left.week_key.cmp(&right.week_key))
                .then(left.month_key.cmp(&right.month_key))
                .then(left.amount_raw.cmp(&right.amount_raw))
        });
        let raw = build_user_reward_raw(config, &increments, now_unix);
        for (epoch_key, epoch) in &raw.rakeback_epochs {
            *rakeback_raw_totals.entry(epoch_key.clone()).or_default() = rakeback_raw_totals
                .get(epoch_key)
                .copied()
                .unwrap_or(0)
                .saturating_add(epoch.raw_bonus_raw);
        }
        for (epoch_key, epoch) in &raw.weekly_epochs {
            *weekly_raw_totals.entry(epoch_key.clone()).or_default() = weekly_raw_totals
                .get(epoch_key)
                .copied()
                .unwrap_or(0)
                .saturating_add(epoch.raw_bonus_raw);
        }
        for event in &raw.level_up_events {
            total_raw_level_up = total_raw_level_up.saturating_add(event.bonus_raw);
        }
        global_lifetime_wager_raw =
            global_lifetime_wager_raw.saturating_add(raw.lifetime_wager_raw);
        global_lifetime_weighted_volume_raw = global_lifetime_weighted_volume_raw
            .saturating_add(raw.lifetime_weighted_volume_raw);
        global_weighted_volume_7d_raw =
            global_weighted_volume_7d_raw.saturating_add(raw.weighted_volume_7d_raw);
        global_weighted_volume_30d_raw =
            global_weighted_volume_30d_raw.saturating_add(raw.weighted_volume_30d_raw);
        user_raw.insert(player_id, raw);
    }

    let positive_house_profit_raw = total_house_profit.max(0) as u128;
    let referral_raw_by_referrer = compute_referral_raw_by_referrer(
        &referral_links,
        &house_profit_by_player,
        &house_profit_by_player_since,
        config,
    );
    let total_raw_referral = referral_raw_by_referrer
        .values()
        .fold(0u128, |total, value| total.saturating_add(*value));
    let level_up_budget = category_budget(
        positive_house_profit_raw,
        config.budget_share_bps,
        config.level_up_share_bps,
        config.rewards_pool_cap_raw,
        None,
    );
    let referral_budget = config
        .rewards_pool_cap_raw
        .map(|cap| cap.min(config.referral_global_cap_raw))
        .unwrap_or(config.referral_global_cap_raw);

    let level_up_scale_bps = scale_bps(level_up_budget, total_raw_level_up);
    let referral_scale_bps = scale_bps(referral_budget, total_raw_referral);

    let mut rakeback_scale_bps = HashMap::new();
    for (epoch_key, raw_total) in rakeback_raw_totals {
        let epoch_profit = house_profit_by_month
            .get(&epoch_key)
            .copied()
            .unwrap_or(0)
            .max(0) as u128;
        let budget = category_budget(
            epoch_profit,
            config.budget_share_bps,
            config.rakeback_share_bps,
            config.rewards_pool_cap_raw,
            Some(config.global_epoch_cap_raw),
        );
        rakeback_scale_bps.insert(epoch_key, scale_bps(budget, raw_total));
    }

    let mut weekly_scale_bps = HashMap::new();
    for (epoch_key, raw_total) in weekly_raw_totals {
        let epoch_profit = house_profit_by_week
            .get(&epoch_key)
            .copied()
            .unwrap_or(0)
            .max(0) as u128;
        let budget = category_budget(
            epoch_profit,
            config.budget_share_bps,
            config.weekly_share_bps,
            config.rewards_pool_cap_raw,
            Some(config.global_epoch_cap_raw),
        );
        weekly_scale_bps.insert(epoch_key, scale_bps(budget, raw_total));
    }

    Ok(RewardsContext {
        current_week_key: epoch_keys.current_week_key,
        current_month_key: epoch_keys.current_month_key,
        wallet_to_player,
        user_raw,
        reward_claims,
        active_reward_claims,
        referral_links,
        rakeback_scale_bps,
        level_up_scale_bps,
        referral_scale_bps,
        weekly_scale_bps,
        house_profit_by_player,
        house_profit_by_player_since,
        global_lifetime_wager_raw,
        global_lifetime_weighted_volume_raw,
        global_weighted_volume_7d_raw,
        global_weighted_volume_30d_raw,
    })
}

fn build_rewards_state_from_context(
    context: &RewardsContext,
    config: &RewardsConfig,
    player_id: Uuid,
    wallet_address: String,
) -> RewardsStateView {
    let raw = context.user_raw.get(&player_id);
    let claims = context
        .reward_claims
        .get(&player_id)
        .cloned()
        .unwrap_or_default();
    let active_claims = context
        .active_reward_claims
        .get(&player_id)
        .cloned()
        .unwrap_or_default();
    let lifetime_wager_raw = raw.map(|value| value.lifetime_wager_raw).unwrap_or(0);
    let wager_7d_raw = raw.map(|value| value.wager_7d_raw).unwrap_or(0);
    let wager_30d_raw = raw.map(|value| value.wager_30d_raw).unwrap_or(0);
    let lifetime_weighted_volume_raw = raw
        .map(|value| value.lifetime_weighted_volume_raw)
        .unwrap_or(0);
    let weighted_volume_7d_raw = raw.map(|value| value.weighted_volume_7d_raw).unwrap_or(0);
    let weighted_volume_30d_raw = raw.map(|value| value.weighted_volume_30d_raw).unwrap_or(0);
    let current_tier = tier_for_points(config, lifetime_wager_raw);
    let next_tier = config
        .tiers
        .iter()
        .find(|tier| tier.threshold_raw > lifetime_wager_raw);
    let progress_bps = next_tier
        .map(|tier| {
            ((lifetime_wager_raw.saturating_mul(BPS_DENOMINATOR)) / tier.threshold_raw.max(1))
                .min(BPS_DENOMINATOR) as u32
        })
        .unwrap_or(10_000);

    let mut rakeback_epochs = Vec::new();
    let mut rakeback_accrued_raw = 0u128;
    let mut rakeback_claimed_raw = 0u128;
    let mut rakeback_claimable_raw = 0u128;
    if let Some(raw) = raw {
        for (epoch_key, epoch) in &raw.rakeback_epochs {
            if *epoch_key == context.current_month_key {
                continue;
            }
            let scale = context
                .rakeback_scale_bps
                .get(epoch_key)
                .copied()
                .unwrap_or(10_000);
            let accrued = mul_bps(epoch.raw_bonus_raw, scale).min(config.rakeback_user_cap_raw);
            let claimed = claimed_epoch_amount(&claims, RewardKind::Rakeback, epoch_key);
            let reserved = claimed_epoch_amount(&active_claims, RewardKind::Rakeback, epoch_key);
            let claimable = if claimed > 0 || reserved > 0 {
                0
            } else {
                accrued
            };
            rakeback_accrued_raw = rakeback_accrued_raw.saturating_add(accrued);
            rakeback_claimed_raw = rakeback_claimed_raw.saturating_add(claimed);
            rakeback_claimable_raw = rakeback_claimable_raw.saturating_add(claimable);
            rakeback_epochs.push(RewardEpochView {
                epoch_key: epoch_key.clone(),
                tier_level: epoch.tier_level,
                tier_name: epoch.tier_name.clone(),
                wager_volume_raw: epoch.wager_volume_raw.to_string(),
                weighted_volume_raw: epoch.weighted_volume_raw.to_string(),
                raw_bonus_raw: epoch.raw_bonus_raw.to_string(),
                claimable_raw: claimable.to_string(),
                scale_bps: scale,
            });
        }
    }
    rakeback_epochs.sort_by(|left, right| right.epoch_key.cmp(&left.epoch_key));

    let mut weekly_epochs = Vec::new();
    let mut weekly_accrued_raw = 0u128;
    let mut weekly_claimed_raw = 0u128;
    let mut weekly_claimable_raw = 0u128;
    if let Some(raw) = raw {
        for (epoch_key, epoch) in &raw.weekly_epochs {
            if *epoch_key == context.current_week_key {
                continue;
            }
            let scale = context
                .weekly_scale_bps
                .get(epoch_key)
                .copied()
                .unwrap_or(10_000);
            let accrued = mul_bps(epoch.raw_bonus_raw, scale).min(config.weekly_user_cap_raw);
            let claimed = claimed_epoch_amount(&claims, RewardKind::Weekly, epoch_key);
            let reserved = claimed_epoch_amount(&active_claims, RewardKind::Weekly, epoch_key);
            let claimable = if claimed > 0 || reserved > 0 {
                0
            } else {
                accrued
            };
            weekly_accrued_raw = weekly_accrued_raw.saturating_add(accrued);
            weekly_claimed_raw = weekly_claimed_raw.saturating_add(claimed);
            weekly_claimable_raw = weekly_claimable_raw.saturating_add(claimable);
            weekly_epochs.push(RewardEpochView {
                epoch_key: epoch_key.clone(),
                tier_level: epoch.tier_level,
                tier_name: epoch.tier_name.clone(),
                wager_volume_raw: epoch.wager_volume_raw.to_string(),
                weighted_volume_raw: epoch.weighted_volume_raw.to_string(),
                raw_bonus_raw: epoch.raw_bonus_raw.to_string(),
                claimable_raw: claimable.to_string(),
                scale_bps: scale,
            });
        }
    }
    weekly_epochs.sort_by(|left, right| right.epoch_key.cmp(&left.epoch_key));

    let mut level_up_rewards = Vec::new();
    let mut level_up_accrued_raw = 0u128;
    let mut level_up_claimed_raw = 0u128;
    let mut level_up_claimable_raw = 0u128;
    if let Some(raw) = raw {
        for reward in &raw.level_up_events {
            let accrued = mul_bps(reward.bonus_raw, context.level_up_scale_bps);
            let claimed =
                claimed_tier_amount(&claims, RewardKind::LevelUp, i32::from(reward.tier_level));
            let reserved = claimed_tier_amount(
                &active_claims,
                RewardKind::LevelUp,
                i32::from(reward.tier_level),
            );
            let claimable = if claimed > 0 || reserved > 0 {
                0
            } else {
                accrued
            };
            level_up_accrued_raw = level_up_accrued_raw.saturating_add(accrued);
            level_up_claimed_raw = level_up_claimed_raw.saturating_add(claimed);
            level_up_claimable_raw = level_up_claimable_raw.saturating_add(claimable);
            level_up_rewards.push(LevelUpRewardView {
                tier_level: reward.tier_level,
                tier_name: reward.tier_name.clone(),
                bonus_raw: reward.bonus_raw.to_string(),
                claimable_raw: claimable.to_string(),
                crossed_at_unix: reward.crossed_at_unix,
                scale_bps: context.level_up_scale_bps,
            });
        }
    }
    level_up_rewards.sort_by(|left, right| right.tier_level.cmp(&left.tier_level));

    let referred_by = context
        .referral_links
        .iter()
        .find(|link| link.referred_player_id == player_id);
    let referred_users: Vec<&ReferralLinkRow> = context
        .referral_links
        .iter()
        .filter(|link| link.referrer_player_id == player_id)
        .collect();
    let referral_accrued_raw = mul_bps(
        referred_users.iter().fold(0u128, |total, link| {
            let house_profit = context
                .house_profit_by_player_since
                .get(&(link.referred_player_id, link.created_at_unix))
                .copied()
                .unwrap_or_else(|| {
                    context
                        .house_profit_by_player
                        .get(&link.referred_player_id)
                        .copied()
                        .unwrap_or(0)
                });
            let positive_profit = house_profit.max(0) as u128;
            total.saturating_add(mul_bps(positive_profit, config.referral_rate_bps))
        }),
        context.referral_scale_bps,
    )
    .min(config.referral_user_cap_raw);
    let referral_claimed_raw = sum_claimed_amount(&claims, RewardKind::Referral, None, None);
    let referral_reserved_raw =
        sum_claimed_amount(&active_claims, RewardKind::Referral, None, None);
    let referral_claimable_raw = referral_accrued_raw
        .saturating_sub(referral_claimed_raw)
        .saturating_sub(referral_reserved_raw);

    let claimable_total_raw = rakeback_claimable_raw
        .saturating_add(weekly_claimable_raw)
        .saturating_add(level_up_claimable_raw)
        .saturating_add(referral_claimable_raw);

    RewardsStateView {
        wallet_address,
        vip: VipProgressView {
            lifetime_wager_raw: lifetime_wager_raw.to_string(),
            wager_7d_raw: wager_7d_raw.to_string(),
            wager_30d_raw: wager_30d_raw.to_string(),
            lifetime_weighted_volume_raw: lifetime_weighted_volume_raw.to_string(),
            weighted_volume_7d_raw: weighted_volume_7d_raw.to_string(),
            weighted_volume_30d_raw: weighted_volume_30d_raw.to_string(),
            vip_points_raw: lifetime_wager_raw.to_string(),
            current_tier_level: current_tier.level,
            current_tier_name: current_tier.name.clone(),
            next_tier_level: next_tier.map(|tier| tier.level),
            next_tier_name: next_tier.map(|tier| tier.name.clone()),
            next_tier_threshold_raw: next_tier.map(|tier| tier.threshold_raw.to_string()),
            progress_bps,
        },
        global_volume: GlobalRewardsVolumeView {
            lifetime_wager_raw: context.global_lifetime_wager_raw.to_string(),
            lifetime_weighted_volume_raw: context.global_lifetime_weighted_volume_raw.to_string(),
            weighted_volume_7d_raw: context.global_weighted_volume_7d_raw.to_string(),
            weighted_volume_30d_raw: context.global_weighted_volume_30d_raw.to_string(),
        },
        rakeback: RewardBucketView {
            accrued_raw: rakeback_accrued_raw.to_string(),
            claimed_raw: rakeback_claimed_raw.to_string(),
            claimable_raw: rakeback_claimable_raw.to_string(),
            scale_bps: 10_000,
        },
        weekly: RewardBucketView {
            accrued_raw: weekly_accrued_raw.to_string(),
            claimed_raw: weekly_claimed_raw.to_string(),
            claimable_raw: weekly_claimable_raw.to_string(),
            scale_bps: 10_000,
        },
        level_up: RewardBucketView {
            accrued_raw: level_up_accrued_raw.to_string(),
            claimed_raw: level_up_claimed_raw.to_string(),
            claimable_raw: level_up_claimable_raw.to_string(),
            scale_bps: context.level_up_scale_bps,
        },
        referral: ReferralView {
            referrer_wallet_address: referred_by
                .and_then(|link| link.referrer_wallet_address.clone()),
            referrer_username: referred_by.and_then(|link| link.referrer_username.clone()),
            linked_at_unix: referred_by.map(|link| link.created_at_unix),
            referred_users: referred_users.len() as u64,
            accrued_raw: referral_accrued_raw.to_string(),
            claimed_raw: referral_claimed_raw.to_string(),
            claimable_raw: referral_claimable_raw.to_string(),
            referral_rate_bps: config.referral_rate_bps,
        },
        rakeback_epochs,
        weekly_epochs,
        level_up_rewards,
        claimable_total_raw: claimable_total_raw.to_string(),
        config: config.config_view(),
    }
}

fn build_user_reward_raw(
    config: &RewardsConfig,
    increments: &[WagerIncrement],
    now_unix: i64,
) -> UserRewardRaw {
    let seven_days_ago = now_unix - 7 * 24 * 60 * 60;
    let thirty_days_ago = now_unix - 30 * 24 * 60 * 60;
    let mut cumulative = 0u128;
    let mut lifetime_wager_raw = 0u128;
    let mut wager_7d_raw = 0u128;
    let mut wager_30d_raw = 0u128;
    let mut lifetime_weighted_volume_raw = 0u128;
    let mut weighted_volume_7d_raw = 0u128;
    let mut weighted_volume_30d_raw = 0u128;
    let mut rakeback_monthly_raw: BTreeMap<String, u128> = BTreeMap::new();
    let mut weekly_wager_volumes: BTreeMap<String, u128> = BTreeMap::new();
    let mut weekly_weighted_volumes: BTreeMap<String, u128> = BTreeMap::new();
    let mut weekly_end_points: BTreeMap<String, u128> = BTreeMap::new();
    let mut monthly_end_points: BTreeMap<String, u128> = BTreeMap::new();
    let mut monthly_wager_volumes: BTreeMap<String, u128> = BTreeMap::new();
    let mut monthly_weighted_volumes: BTreeMap<String, u128> = BTreeMap::new();
    let mut level_up_events = Vec::new();

    for increment in increments {
        if increment.amount_raw == 0 {
            continue;
        }
        if increment.created_at_unix >= seven_days_ago {
            wager_7d_raw = wager_7d_raw.saturating_add(increment.amount_raw);
            weighted_volume_7d_raw =
                weighted_volume_7d_raw.saturating_add(increment.weighted_volume_raw);
        }
        if increment.created_at_unix >= thirty_days_ago {
            wager_30d_raw = wager_30d_raw.saturating_add(increment.amount_raw);
            weighted_volume_30d_raw =
                weighted_volume_30d_raw.saturating_add(increment.weighted_volume_raw);
        }
        let rakeback_increment =
            segmented_reward(config, cumulative, increment, |tier| tier.rakeback_bps);
        *rakeback_monthly_raw
            .entry(increment.month_key.clone())
            .or_default() = rakeback_monthly_raw
            .get(&increment.month_key)
            .copied()
            .unwrap_or(0)
            .saturating_add(rakeback_increment);
        let next_cumulative = cumulative.saturating_add(increment.amount_raw);
        for tier in config.tiers.iter().skip(1) {
            if cumulative < tier.threshold_raw && next_cumulative >= tier.threshold_raw {
                level_up_events.push(LevelUpEventRaw {
                    tier_level: tier.level,
                    tier_name: tier.name.clone(),
                    bonus_raw: tier.level_up_bonus_raw,
                    crossed_at_unix: increment.created_at_unix,
                });
            }
        }
        cumulative = next_cumulative;
        lifetime_wager_raw = lifetime_wager_raw.saturating_add(increment.amount_raw);
        lifetime_weighted_volume_raw =
            lifetime_weighted_volume_raw.saturating_add(increment.weighted_volume_raw);
        *weekly_wager_volumes
            .entry(increment.week_key.clone())
            .or_default() = weekly_wager_volumes
            .get(&increment.week_key)
            .copied()
            .unwrap_or(0)
            .saturating_add(increment.amount_raw);
        *weekly_weighted_volumes
            .entry(increment.week_key.clone())
            .or_default() = weekly_weighted_volumes
            .get(&increment.week_key)
            .copied()
            .unwrap_or(0)
            .saturating_add(increment.weighted_volume_raw);
        weekly_end_points.insert(increment.week_key.clone(), cumulative);
        *monthly_wager_volumes
            .entry(increment.month_key.clone())
            .or_default() = monthly_wager_volumes
            .get(&increment.month_key)
            .copied()
            .unwrap_or(0)
            .saturating_add(increment.amount_raw);
        *monthly_weighted_volumes
            .entry(increment.month_key.clone())
            .or_default() = monthly_weighted_volumes
            .get(&increment.month_key)
            .copied()
            .unwrap_or(0)
            .saturating_add(increment.weighted_volume_raw);
        monthly_end_points.insert(increment.month_key.clone(), cumulative);
    }

    let mut weekly_epochs = BTreeMap::new();
    for (epoch_key, weighted_volume_raw) in weekly_weighted_volumes {
        let tier = tier_for_points(
            config,
            weekly_end_points
                .get(&epoch_key)
                .copied()
                .unwrap_or(cumulative),
        );
        let raw_bonus_raw = if weighted_volume_raw >= config.weekly_min_weighted_volume_raw {
            mul_bps(weighted_volume_raw, tier.weekly_bps)
        } else {
            0
        };
        weekly_epochs.insert(
            epoch_key.clone(),
            UserEpochRaw {
                wager_volume_raw: weekly_wager_volumes.get(&epoch_key).copied().unwrap_or(0),
                weighted_volume_raw,
                raw_bonus_raw,
                tier_level: tier.level,
                tier_name: tier.name.clone(),
            },
        );
    }

    let mut rakeback_epochs = BTreeMap::new();
    for (epoch_key, weighted_volume_raw) in monthly_weighted_volumes {
        let raw_rakeback_raw = rakeback_monthly_raw.get(&epoch_key).copied().unwrap_or(0);
        let tier = tier_for_points(
            config,
            monthly_end_points
                .get(&epoch_key)
                .copied()
                .unwrap_or(cumulative),
        );
        rakeback_epochs.insert(
            epoch_key.clone(),
            UserEpochRaw {
                wager_volume_raw: monthly_wager_volumes.get(&epoch_key).copied().unwrap_or(0),
                weighted_volume_raw,
                raw_bonus_raw: raw_rakeback_raw,
                tier_level: tier.level,
                tier_name: tier.name.clone(),
            },
        );
    }

    UserRewardRaw {
        lifetime_wager_raw,
        wager_7d_raw,
        wager_30d_raw,
        lifetime_weighted_volume_raw,
        weighted_volume_7d_raw,
        weighted_volume_30d_raw,
        rakeback_epochs,
        weekly_epochs,
        level_up_events,
    }
}

async fn load_reward_event_rows(pool: &PgPool) -> anyhow::Result<Vec<RewardEventRow>> {
    let rows = sqlx::query(
        r#"
        SELECT
            player_wallet,
            reference_kind,
            reference_id,
            event_name,
            COALESCE(payload->>'amount', payload->>'payout', payload->>'refunded', '0') AS amount_raw,
            EXTRACT(EPOCH FROM created_at)::BIGINT AS created_at_unix,
            TO_CHAR(DATE_TRUNC('week', created_at AT TIME ZONE 'UTC'), 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS week_key,
            TO_CHAR(DATE_TRUNC('month', created_at AT TIME ZONE 'UTC'), 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS month_key
        FROM vault_indexed_events
        WHERE event_name IN ('HandReserved', 'HandSettled', 'HandVoided')
          AND player_wallet IS NOT NULL
          AND reference_kind IS NOT NULL
          AND reference_id IS NOT NULL
        ORDER BY created_at ASC, block_number ASC, id ASC
        "#,
    )
    .fetch_all(pool)
    .await
    .context("failed to query reward event rows")?;

    rows.into_iter()
        .map(|row| {
            Ok(RewardEventRow {
                player_wallet: row
                    .try_get("player_wallet")
                    .context("missing player_wallet")?,
                reference_kind: row
                    .try_get("reference_kind")
                    .context("missing reference_kind")?,
                reference_id: row
                    .try_get("reference_id")
                    .context("missing reference_id")?,
                event_name: row.try_get("event_name").context("missing event_name")?,
                amount_raw: parse_raw_u128(
                    &row.try_get::<String, _>("amount_raw")
                        .context("missing amount_raw")?,
                )?,
                created_at_unix: row
                    .try_get("created_at_unix")
                    .context("missing created_at_unix")?,
                week_key: row.try_get("week_key").context("missing week_key")?,
                month_key: row.try_get("month_key").context("missing month_key")?,
            })
        })
        .collect()
}

async fn load_wallet_to_player_map(pool: &PgPool) -> anyhow::Result<HashMap<String, Uuid>> {
    let rows = sqlx::query(
        r#"
        SELECT wallet_address, player_id
        FROM player_wallets
        "#,
    )
    .fetch_all(pool)
    .await
    .context("failed to query wallet links for rewards")?;

    let mut map = HashMap::new();
    for row in rows {
        let wallet_address: String = row
            .try_get("wallet_address")
            .context("missing wallet_address")?;
        let player_id: Uuid = row.try_get("player_id").context("missing player_id")?;
        map.insert(
            accounts::normalize_wallet_address(&wallet_address),
            player_id,
        );
    }
    Ok(map)
}

async fn load_reward_claims(pool: &PgPool) -> anyhow::Result<HashMap<Uuid, Vec<RewardClaimRow>>> {
    let rows = sqlx::query(
        r#"
        SELECT
            player_id,
            reward_kind,
            epoch_key,
            tier_level,
            amount_raw
        FROM reward_claims
        ORDER BY created_at DESC
        "#,
    )
    .fetch_all(pool)
    .await
    .context("failed to query reward claims")?;

    let mut claims = HashMap::<Uuid, Vec<RewardClaimRow>>::new();
    for row in rows {
        let player_id: Uuid = row
            .try_get("player_id")
            .context("missing reward claim player_id")?;
        let reward_kind = row
            .try_get::<String, _>("reward_kind")
            .context("missing reward_kind")
            .ok()
            .and_then(|value| RewardKind::parse(&value))
            .ok_or_else(|| anyhow!("invalid reward claim kind"))?;
        claims.entry(player_id).or_default().push(RewardClaimRow {
            reward_kind,
            epoch_key: row.try_get("epoch_key").ok(),
            tier_level: row.try_get("tier_level").ok(),
            amount_raw: parse_raw_u128(
                &row.try_get::<String, _>("amount_raw")
                    .context("missing reward claim amount_raw")?,
            )?,
        });
    }
    Ok(claims)
}

async fn load_active_reward_claims(
    pool: &PgPool,
) -> anyhow::Result<HashMap<Uuid, Vec<RewardClaimRow>>> {
    let rows = sqlx::query(
        r#"
        SELECT player_id, claim_rows
        FROM reward_claim_intents
        WHERE status = 'submitted'
           OR (status = 'reserved' AND expires_at > NOW())
        "#,
    )
    .fetch_all(pool)
    .await
    .context("failed to query active reward claim reservations")?;

    let mut claims = HashMap::<Uuid, Vec<RewardClaimRow>>::new();
    for row in rows {
        let player_id: Uuid = row
            .try_get("player_id")
            .context("missing active reward claim player_id")?;
        let claim_rows_value: Value = row
            .try_get("claim_rows")
            .context("missing active reward claim rows")?;
        for claim_row in parse_prepared_claim_rows_value(&claim_rows_value)? {
            claims.entry(player_id).or_default().push(RewardClaimRow {
                reward_kind: claim_row.reward_kind,
                epoch_key: claim_row.epoch_key,
                tier_level: claim_row.tier_level,
                amount_raw: claim_row.amount_raw,
            });
        }
    }
    Ok(claims)
}

async fn load_referral_links(pool: &PgPool) -> anyhow::Result<Vec<ReferralLinkRow>> {
    let rows = sqlx::query(
        r#"
        SELECT
            rr.referred_player_id,
            rr.referrer_player_id,
            p.wallet_address AS referrer_wallet_address,
            pp.username AS referrer_username,
            EXTRACT(EPOCH FROM rr.created_at)::BIGINT AS created_at_unix
        FROM reward_referrals rr
        INNER JOIN players p ON p.id = rr.referrer_player_id
        LEFT JOIN player_profiles pp ON pp.player_id = rr.referrer_player_id
        "#,
    )
    .fetch_all(pool)
    .await
    .context("failed to query reward referrals")?;

    rows.into_iter()
        .map(|row| {
            Ok(ReferralLinkRow {
                referred_player_id: row
                    .try_get("referred_player_id")
                    .context("missing referred_player_id")?,
                referrer_player_id: row
                    .try_get("referrer_player_id")
                    .context("missing referrer_player_id")?,
                referrer_wallet_address: row
                    .try_get::<Option<String>, _>("referrer_wallet_address")
                    .ok()
                    .flatten()
                    .map(|value| accounts::normalize_wallet_address(&value)),
                referrer_username: row.try_get("referrer_username").ok().flatten(),
                created_at_unix: row
                    .try_get("created_at_unix")
                    .context("missing referral created_at_unix")?,
            })
        })
        .collect()
}

async fn get_referral_link_for_referred(
    pool: &PgPool,
    referred_player_id: Uuid,
) -> anyhow::Result<Option<ReferralLinkRow>> {
    Ok(load_referral_links(pool)
        .await?
        .into_iter()
        .find(|link| link.referred_player_id == referred_player_id))
}

async fn resolve_referrer_target(
    pool: &PgPool,
    input: &str,
) -> anyhow::Result<Option<ReferralLinkRow>> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let is_wallet = trimmed.starts_with("0x");
    let username = if is_wallet {
        None
    } else {
        Some(trimmed.to_ascii_lowercase())
    };
    let wallet_address = if is_wallet {
        Some(accounts::normalize_wallet_address(trimmed))
    } else {
        None
    };
    let row = sqlx::query(
        r#"
        SELECT
            p.id AS referrer_player_id,
            p.wallet_address AS referrer_wallet_address,
            pp.username AS referrer_username
        FROM players p
        LEFT JOIN player_profiles pp ON pp.player_id = p.id
        WHERE ($1::TEXT IS NOT NULL AND LOWER(p.wallet_address) = $1)
           OR ($2::TEXT IS NOT NULL AND pp.username = $2)
        LIMIT 1
        "#,
    )
    .bind(wallet_address.as_deref())
    .bind(username.as_deref())
    .fetch_optional(pool)
    .await
    .context("failed to resolve referrer target")?;
    row.map(|row| {
        Ok(ReferralLinkRow {
            referred_player_id: Uuid::nil(),
            referrer_player_id: row
                .try_get("referrer_player_id")
                .context("missing resolved referrer_player_id")?,
            referrer_wallet_address: row
                .try_get::<Option<String>, _>("referrer_wallet_address")
                .ok()
                .flatten()
                .map(|value| accounts::normalize_wallet_address(&value)),
            referrer_username: row.try_get("referrer_username").ok().flatten(),
            created_at_unix: 0,
        })
    })
    .transpose()
}

struct EpochKeys {
    current_week_key: String,
    current_month_key: String,
}

async fn load_current_epoch_keys(pool: &PgPool) -> anyhow::Result<EpochKeys> {
    let row = sqlx::query(
        r#"
        SELECT
            TO_CHAR(DATE_TRUNC('week', NOW() AT TIME ZONE 'UTC'), 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS current_week_key,
            TO_CHAR(DATE_TRUNC('month', NOW() AT TIME ZONE 'UTC'), 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS current_month_key
        "#,
    )
    .fetch_one(pool)
    .await
    .context("failed to query current rewards epoch keys")?;
    Ok(EpochKeys {
        current_week_key: row
            .try_get("current_week_key")
            .context("missing current_week_key")?,
        current_month_key: row
            .try_get("current_month_key")
            .context("missing current_month_key")?,
    })
}

fn compute_referral_raw_by_referrer(
    referral_links: &[ReferralLinkRow],
    house_profit_by_player: &HashMap<Uuid, i128>,
    house_profit_by_player_since: &HashMap<(Uuid, i64), i128>,
    config: &RewardsConfig,
) -> HashMap<Uuid, u128> {
    let mut referral_raw_by_referrer: HashMap<Uuid, u128> = HashMap::new();
    for link in referral_links {
        let house_profit = house_profit_by_player_since
            .get(&(link.referred_player_id, link.created_at_unix))
            .copied()
            .unwrap_or_else(|| {
                house_profit_by_player
                    .get(&link.referred_player_id)
                    .copied()
                    .unwrap_or(0)
            });
        let positive_profit = house_profit.max(0) as u128;
        let raw = mul_bps(positive_profit, config.referral_rate_bps);
        if raw > 0 {
            let current = referral_raw_by_referrer
                .get(&link.referrer_player_id)
                .copied()
                .unwrap_or(0);
            referral_raw_by_referrer.insert(link.referrer_player_id, current.saturating_add(raw));
        }
    }
    referral_raw_by_referrer
}

fn tier_for_points<'a>(config: &'a RewardsConfig, points_raw: u128) -> &'a RewardsTierConfig {
    config
        .tiers
        .iter()
        .rev()
        .find(|tier| points_raw >= tier.threshold_raw)
        .unwrap_or(&config.tiers[0])
}

fn reward_house_edge_bps(config: &RewardsConfig, game_kind: &str) -> u32 {
    match game_kind {
        "blackjack" => config.blackjack_reward_house_edge_bps,
        "dice" => config.dice_reward_house_edge_bps,
        "roulette" => config.roulette_reward_house_edge_bps,
        "baccarat" => config.baccarat_reward_house_edge_bps,
        _ => DEFAULT_DICE_REWARD_HOUSE_EDGE_BPS,
    }
}

fn segmented_reward(
    config: &RewardsConfig,
    mut cumulative_before: u128,
    increment: &WagerIncrement,
    rate_selector: impl Fn(&RewardsTierConfig) -> u32,
) -> u128 {
    let mut amount_raw = increment.amount_raw;
    let mut reward = 0u128;
    while amount_raw > 0 {
        let tier = tier_for_points(config, cumulative_before);
        let next_threshold = config
            .tiers
            .iter()
            .find(|candidate| candidate.threshold_raw > cumulative_before)
            .map(|tier| tier.threshold_raw)
            .unwrap_or(u128::MAX);
        let headroom = next_threshold.saturating_sub(cumulative_before);
        let segment = if headroom == 0 {
            amount_raw
        } else {
            amount_raw.min(headroom)
        };
        let weighted_segment = mul_bps(segment, increment.house_edge_bps);
        reward = reward.saturating_add(mul_bps(weighted_segment, rate_selector(tier)));
        cumulative_before = cumulative_before.saturating_add(segment);
        amount_raw = amount_raw.saturating_sub(segment);
    }
    reward
}

fn category_budget(
    positive_house_profit_raw: u128,
    budget_share_bps: u32,
    allocation_bps: u32,
    rewards_pool_cap_raw: Option<u128>,
    global_cap_raw: Option<u128>,
) -> u128 {
    let base_budget = rewards_pool_cap_raw
        .map(|cap| mul_bps(positive_house_profit_raw, budget_share_bps).min(cap))
        .unwrap_or_else(|| mul_bps(positive_house_profit_raw, budget_share_bps));
    let category_budget = mul_bps(base_budget, allocation_bps);
    global_cap_raw
        .map(|cap| category_budget.min(cap))
        .unwrap_or(category_budget)
}

fn scale_bps(budget_raw: u128, raw_total: u128) -> u32 {
    if raw_total == 0 || budget_raw >= raw_total {
        return 10_000;
    }
    ((budget_raw.saturating_mul(BPS_DENOMINATOR) / raw_total).min(BPS_DENOMINATOR)) as u32
}

fn mul_bps(amount_raw: u128, bps: u32) -> u128 {
    amount_raw.saturating_mul(u128::from(bps)) / BPS_DENOMINATOR
}

#[derive(Debug, Deserialize)]
struct RewardsTierEnvConfig {
    level: Option<u8>,
    name: String,
    threshold_raw: Value,
    rakeback_bps: u32,
    weekly_bps: u32,
    level_up_bonus_raw: Value,
}

fn read_env_u32(name: &str) -> anyhow::Result<Option<u32>> {
    env::var(name)
        .ok()
        .map(|value| {
            value
                .trim()
                .parse::<u32>()
                .with_context(|| format!("invalid {name}"))
        })
        .transpose()
}

fn read_env_u128(name: &str) -> anyhow::Result<Option<u128>> {
    env::var(name)
        .ok()
        .map(|value| {
            value
                .trim()
                .parse::<u128>()
                .with_context(|| format!("invalid {name}"))
        })
        .transpose()
}

fn read_env_rewards_tiers(name: &str) -> anyhow::Result<Option<Vec<RewardsTierConfig>>> {
    env::var(name)
        .ok()
        .map(|value| {
            let rows: Vec<RewardsTierEnvConfig> =
                serde_json::from_str(&value).with_context(|| format!("invalid {name}"))?;
            if rows.is_empty() {
                return Err(anyhow!("{name} must contain at least one tier"));
            }
            let mut tiers = Vec::with_capacity(rows.len());
            let mut previous_threshold = None;
            for (index, row) in rows.into_iter().enumerate() {
                let threshold_raw = parse_json_u128(&row.threshold_raw, name, "threshold_raw")?;
                let level_up_bonus_raw =
                    parse_json_u128(&row.level_up_bonus_raw, name, "level_up_bonus_raw")?;
                if row.name.trim().is_empty() {
                    return Err(anyhow!("{name} tier {} has an empty name", index + 1));
                }
                if let Some(previous_threshold) = previous_threshold {
                    if threshold_raw < previous_threshold {
                        return Err(anyhow!(
                            "{name} tiers must be sorted by ascending threshold_raw"
                        ));
                    }
                }
                previous_threshold = Some(threshold_raw);
                tiers.push(RewardsTierConfig {
                    level: row.level.unwrap_or(index as u8),
                    name: row.name.trim().to_string(),
                    threshold_raw,
                    rakeback_bps: row.rakeback_bps,
                    weekly_bps: row.weekly_bps,
                    level_up_bonus_raw,
                });
            }
            Ok(tiers)
        })
        .transpose()
}

fn read_env_i64(name: &str) -> anyhow::Result<Option<i64>> {
    env::var(name)
        .ok()
        .map(|value| {
            value
                .trim()
                .parse::<i64>()
                .with_context(|| format!("invalid {name}"))
        })
        .transpose()
}

fn parse_json_u128(value: &Value, env_name: &str, field_name: &str) -> anyhow::Result<u128> {
    match value {
        Value::String(raw) => raw
            .trim()
            .parse::<u128>()
            .with_context(|| format!("invalid {field_name} in {env_name}")),
        Value::Number(raw) => raw
            .as_u64()
            .map(u128::from)
            .ok_or_else(|| anyhow!("invalid {field_name} in {env_name}")),
        _ => Err(anyhow!("invalid {field_name} in {env_name}")),
    }
}

fn parse_raw_u128(value: &str) -> anyhow::Result<u128> {
    value
        .trim()
        .parse::<u128>()
        .with_context(|| format!("invalid raw reward value: {value}"))
}

fn checked_profit(wager_raw: u128, payout_raw: u128) -> anyhow::Result<i128> {
    let wager = i128::try_from(wager_raw).context("wager does not fit in i128")?;
    let payout = i128::try_from(payout_raw).context("payout does not fit in i128")?;
    Ok(wager - payout)
}

fn sum_claimed_amount(
    claims: &[RewardClaimRow],
    reward_kind: RewardKind,
    epoch_key: Option<&str>,
    tier_level: Option<i32>,
) -> u128 {
    claims
        .iter()
        .filter(|row| row.reward_kind == reward_kind)
        .filter(|row| match (epoch_key, tier_level) {
            (Some(epoch_key), _) => row.epoch_key.as_deref() == Some(epoch_key),
            (_, Some(tier_level)) => row.tier_level == Some(tier_level),
            (None, None) => true,
        })
        .fold(0u128, |total, row| total.saturating_add(row.amount_raw))
}

fn claimed_epoch_amount(
    claims: &[RewardClaimRow],
    reward_kind: RewardKind,
    epoch_key: &str,
) -> u128 {
    sum_claimed_amount(claims, reward_kind, Some(epoch_key), None)
}

fn claimed_tier_amount(
    claims: &[RewardClaimRow],
    reward_kind: RewardKind,
    tier_level: i32,
) -> u128 {
    sum_claimed_amount(claims, reward_kind, None, Some(tier_level))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wei(strk: u128) -> u128 {
        strk * STRK_WEI
    }

    #[test]
    fn default_rewards_ladder_uses_new_thresholds() {
        let config = RewardsConfig::default();
        assert_eq!(
            config.tiers.first().map(|tier| tier.name.as_str()),
            Some("Base")
        );
        assert_eq!(config.tiers[1].name, "Bronze");
        assert_eq!(config.tiers[1].threshold_raw, wei(10_000));
        assert_eq!(config.tiers[2].name, "Silver");
        assert_eq!(config.tiers[2].threshold_raw, wei(50_000));
        assert_eq!(
            config.tiers.last().map(|tier| tier.name.as_str()),
            Some("Tanzanite")
        );
        assert_eq!(
            config.tiers.last().map(|tier| tier.threshold_raw),
            Some(wei(1_000_000_000))
        );
    }

    #[test]
    fn reward_coupon_codes_are_normalized_and_validated() {
        assert_eq!(
            normalize_reward_coupon_code(" moros launch_01 ").unwrap(),
            "MOROSLAUNCH_01"
        );
        assert!(normalize_reward_coupon_code("abc").is_err());
        assert!(normalize_reward_coupon_code("moros/launch").is_err());
    }

    #[test]
    fn build_user_reward_raw_buckets_rakeback_by_month() {
        let config = RewardsConfig::default();
        let month_key = "2026-04-01T00:00:00Z".to_string();
        let week_key = "2026-04-13T00:00:00Z".to_string();
        let raw = build_user_reward_raw(
            &config,
            &[
                WagerIncrement {
                    amount_raw: wei(20_000),
                    weighted_volume_raw: wei(200),
                    house_edge_bps: 100,
                    created_at_unix: 1_713_000_000,
                    week_key: week_key.clone(),
                    month_key: month_key.clone(),
                },
                WagerIncrement {
                    amount_raw: wei(40_000),
                    weighted_volume_raw: wei(400),
                    house_edge_bps: 100,
                    created_at_unix: 1_713_000_100,
                    week_key: week_key.clone(),
                    month_key: month_key.clone(),
                },
            ],
            1_713_000_200,
        );

        let rakeback_epoch = raw
            .rakeback_epochs
            .get(&month_key)
            .expect("rakeback epoch should exist");
        let weekly_epoch = raw
            .weekly_epochs
            .get(&week_key)
            .expect("weekly epoch should exist");

        assert_eq!(raw.lifetime_wager_raw, wei(60_000));
        assert_eq!(raw.lifetime_weighted_volume_raw, wei(600));
        assert_eq!(rakeback_epoch.tier_name, "Silver");
        assert_eq!(rakeback_epoch.wager_volume_raw, wei(60_000));
        assert_eq!(rakeback_epoch.weighted_volume_raw, wei(600));
        assert_eq!(rakeback_epoch.raw_bonus_raw, wei(17) + (STRK_WEI / 2));
        assert_eq!(weekly_epoch.tier_name, "Silver");
        assert_eq!(weekly_epoch.weighted_volume_raw, wei(600));
        assert_eq!(weekly_epoch.raw_bonus_raw, wei(9));
        assert_eq!(raw.level_up_events.len(), 2);
        assert_eq!(raw.level_up_events[0].tier_name, "Bronze");
        assert_eq!(raw.level_up_events[1].tier_name, "Silver");
    }
}
