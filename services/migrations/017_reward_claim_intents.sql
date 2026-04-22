ALTER TABLE reward_claims
    ADD COLUMN IF NOT EXISTS claim_id UUID;

CREATE INDEX IF NOT EXISTS idx_reward_claims_claim_id
    ON reward_claims (claim_id)
    WHERE claim_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS reward_claim_intents (
    claim_id UUID PRIMARY KEY,
    player_id UUID NOT NULL REFERENCES players(id) ON DELETE CASCADE,
    wallet_address TEXT NOT NULL,
    reward_kind TEXT NOT NULL,
    amount_raw TEXT NOT NULL,
    claim_rows JSONB NOT NULL DEFAULT '[]'::jsonb,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    status TEXT NOT NULL DEFAULT 'reserved',
    tx_hash TEXT,
    error TEXT,
    reserved_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,
    submitted_at TIMESTAMPTZ,
    confirmed_at TIMESTAMPTZ,
    failed_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CHECK (status IN ('reserved', 'submitted', 'claimed', 'failed', 'expired'))
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_reward_claim_intents_player_kind_active_unique
    ON reward_claim_intents (player_id, reward_kind)
    WHERE status IN ('reserved', 'submitted');

CREATE INDEX IF NOT EXISTS idx_reward_claim_intents_player_status
    ON reward_claim_intents (player_id, status, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_reward_claim_intents_status_expires
    ON reward_claim_intents (status, expires_at);

CREATE INDEX IF NOT EXISTS idx_vault_indexed_events_rewards_scan
    ON vault_indexed_events (
        event_name,
        player_wallet,
        reference_kind,
        reference_id,
        created_at,
        block_number,
        id
    )
    WHERE event_name IN ('HandReserved', 'HandSettled', 'HandVoided')
      AND player_wallet IS NOT NULL
      AND reference_kind IS NOT NULL
      AND reference_id IS NOT NULL;
