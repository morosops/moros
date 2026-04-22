CREATE TABLE IF NOT EXISTS chain_sync_cursors (
    service_name TEXT NOT NULL,
    stream_name TEXT NOT NULL,
    cursor_block BIGINT NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (service_name, stream_name)
);

CREATE TABLE IF NOT EXISTS vault_indexed_events (
    id UUID PRIMARY KEY,
    stream_name TEXT NOT NULL,
    event_fingerprint TEXT NOT NULL,
    block_number BIGINT NOT NULL,
    transaction_hash TEXT NOT NULL,
    event_name TEXT NOT NULL,
    player_wallet TEXT,
    reference_kind TEXT,
    reference_id TEXT,
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (stream_name, event_fingerprint)
);

CREATE INDEX IF NOT EXISTS idx_vault_indexed_events_block_created
    ON vault_indexed_events (stream_name, block_number DESC, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_vault_indexed_events_player_created
    ON vault_indexed_events (player_wallet, created_at DESC)
    WHERE player_wallet IS NOT NULL;
