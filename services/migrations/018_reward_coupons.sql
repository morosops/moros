CREATE TABLE IF NOT EXISTS reward_coupons (
    id UUID PRIMARY KEY,
    code TEXT NOT NULL UNIQUE,
    description TEXT,
    amount_raw TEXT NOT NULL,
    max_global_redemptions BIGINT NOT NULL,
    max_per_user_redemptions BIGINT NOT NULL DEFAULT 1,
    active BOOLEAN NOT NULL DEFAULT TRUE,
    starts_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ,
    created_by TEXT,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CHECK (max_global_redemptions > 0),
    CHECK (max_per_user_redemptions > 0)
);

CREATE INDEX IF NOT EXISTS idx_reward_coupons_active
    ON reward_coupons (active, starts_at, expires_at);

CREATE TABLE IF NOT EXISTS reward_coupon_redemptions (
    id UUID PRIMARY KEY,
    coupon_id UUID NOT NULL REFERENCES reward_coupons(id) ON DELETE RESTRICT,
    player_id UUID NOT NULL REFERENCES players(id) ON DELETE CASCADE,
    wallet_address TEXT NOT NULL,
    amount_raw TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'reserved',
    tx_hash TEXT,
    error TEXT,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    reserved_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    submitted_at TIMESTAMPTZ,
    confirmed_at TIMESTAMPTZ,
    failed_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CHECK (status IN ('reserved', 'submitted', 'claimed', 'failed'))
);

CREATE INDEX IF NOT EXISTS idx_reward_coupon_redemptions_coupon_status
    ON reward_coupon_redemptions (coupon_id, status, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_reward_coupon_redemptions_player_created
    ON reward_coupon_redemptions (player_id, reserved_at DESC);

CREATE INDEX IF NOT EXISTS idx_reward_coupon_redemptions_tx_hash
    ON reward_coupon_redemptions (tx_hash)
    WHERE tx_hash IS NOT NULL;
