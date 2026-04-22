CREATE TABLE IF NOT EXISTS player_funding_accounts (
    player_id UUID NOT NULL REFERENCES players(id) ON DELETE CASCADE,
    chain_key TEXT NOT NULL,
    chain_family TEXT NOT NULL,
    address TEXT NOT NULL,
    wallet_provider TEXT NOT NULL DEFAULT 'unknown',
    wallet_id TEXT,
    public_key TEXT,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (player_id, chain_key)
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_player_funding_accounts_chain_address_unique
    ON player_funding_accounts (chain_key, LOWER(address));

CREATE INDEX IF NOT EXISTS idx_player_funding_accounts_player_created
    ON player_funding_accounts (player_id, created_at DESC);
