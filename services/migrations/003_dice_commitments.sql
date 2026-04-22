CREATE TABLE IF NOT EXISTS dice_server_commitments (
    commitment_id BIGINT PRIMARY KEY,
    server_seed TEXT NOT NULL,
    server_seed_hash TEXT NOT NULL,
    reveal_deadline_block BIGINT NOT NULL,
    status TEXT NOT NULL DEFAULT 'available',
    round_id BIGINT,
    commit_tx_hash TEXT,
    settle_tx_hash TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_dice_server_commitments_status_created
    ON dice_server_commitments (status, created_at DESC);
