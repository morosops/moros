CREATE TABLE IF NOT EXISTS withdrawal_requests (
    id UUID PRIMARY KEY,
    player_id UUID NOT NULL REFERENCES players(id) ON DELETE CASCADE,
    requested_by_wallet TEXT NOT NULL,
    source_balance TEXT NOT NULL,
    destination_chain_key TEXT NOT NULL,
    destination_asset_symbol TEXT NOT NULL,
    destination_address TEXT NOT NULL,
    amount_raw TEXT NOT NULL,
    route_kind TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'queued',
    route_job_id UUID,
    destination_tx_hash TEXT,
    failure_reason TEXT,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_withdrawal_requests_player_created
    ON withdrawal_requests (player_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_withdrawal_requests_status_created
    ON withdrawal_requests (status, created_at ASC);

CREATE TABLE IF NOT EXISTS withdrawal_events (
    id UUID PRIMARY KEY,
    withdrawal_id UUID NOT NULL REFERENCES withdrawal_requests(id) ON DELETE CASCADE,
    event_type TEXT NOT NULL,
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_withdrawal_events_request_created
    ON withdrawal_events (withdrawal_id, created_at ASC);
