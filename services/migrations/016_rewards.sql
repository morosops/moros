CREATE TABLE IF NOT EXISTS reward_referrals (
    referred_player_id UUID PRIMARY KEY REFERENCES players(id) ON DELETE CASCADE,
    referrer_player_id UUID NOT NULL REFERENCES players(id) ON DELETE RESTRICT,
    linked_by_wallet TEXT,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CHECK (referred_player_id <> referrer_player_id)
);

CREATE INDEX IF NOT EXISTS idx_reward_referrals_referrer_created
    ON reward_referrals (referrer_player_id, created_at DESC);

CREATE TABLE IF NOT EXISTS reward_claims (
    id UUID PRIMARY KEY,
    player_id UUID NOT NULL REFERENCES players(id) ON DELETE CASCADE,
    reward_kind TEXT NOT NULL,
    epoch_key TEXT,
    tier_level INTEGER,
    amount_raw TEXT NOT NULL,
    tx_hash TEXT,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_reward_claims_player_created
    ON reward_claims (player_id, created_at DESC);

CREATE UNIQUE INDEX IF NOT EXISTS idx_reward_claims_player_epoch_unique
    ON reward_claims (player_id, reward_kind, epoch_key)
    WHERE epoch_key IS NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_reward_claims_player_tier_unique
    ON reward_claims (player_id, reward_kind, tier_level)
    WHERE tier_level IS NOT NULL;
