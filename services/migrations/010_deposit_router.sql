CREATE TABLE IF NOT EXISTS deposit_supported_assets (
    id TEXT NOT NULL,
    chain_key TEXT NOT NULL,
    chain_family TEXT NOT NULL,
    network TEXT NOT NULL,
    chain_id TEXT NOT NULL,
    asset_symbol TEXT NOT NULL,
    asset_address TEXT NOT NULL,
    asset_decimals INTEGER NOT NULL,
    route_kind TEXT NOT NULL,
    watch_mode TEXT NOT NULL DEFAULT 'erc20_transfer',
    min_amount TEXT NOT NULL DEFAULT '0',
    max_amount TEXT NOT NULL DEFAULT '0',
    confirmations_required INTEGER NOT NULL DEFAULT 12,
    status TEXT NOT NULL DEFAULT 'enabled',
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (id, chain_key)
);

CREATE INDEX IF NOT EXISTS idx_deposit_supported_assets_chain_status
    ON deposit_supported_assets (chain_key, status);

CREATE TABLE IF NOT EXISTS deposit_channels (
    id UUID PRIMARY KEY,
    player_id UUID NOT NULL REFERENCES players(id) ON DELETE CASCADE,
    asset_id TEXT NOT NULL,
    chain_key TEXT NOT NULL,
    deposit_address TEXT NOT NULL UNIQUE,
    qr_payload TEXT NOT NULL,
    route_kind TEXT NOT NULL,
    address_index INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'active',
    watch_from_block BIGINT,
    last_scanned_block BIGINT,
    last_seen_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    FOREIGN KEY (asset_id, chain_key) REFERENCES deposit_supported_assets(id, chain_key)
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_deposit_channels_active_unique
    ON deposit_channels (player_id, asset_id, chain_key)
    WHERE status = 'active';

CREATE INDEX IF NOT EXISTS idx_deposit_channels_asset_chain
    ON deposit_channels (asset_id, chain_key, created_at DESC);

CREATE TABLE IF NOT EXISTS deposit_transfers (
    id UUID PRIMARY KEY,
    channel_id UUID NOT NULL REFERENCES deposit_channels(id) ON DELETE CASCADE,
    asset_id TEXT NOT NULL,
    chain_key TEXT NOT NULL,
    deposit_address TEXT NOT NULL,
    sender_address TEXT,
    tx_hash TEXT NOT NULL,
    block_number BIGINT,
    block_hash TEXT,
    amount_raw TEXT NOT NULL,
    confirmations INTEGER NOT NULL DEFAULT 0,
    required_confirmations INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'DEPOSIT_DETECTED',
    risk_state TEXT NOT NULL DEFAULT 'clear',
    destination_chain_key TEXT NOT NULL DEFAULT 'starknet',
    destination_asset_symbol TEXT NOT NULL DEFAULT 'STRK',
    credit_target TEXT,
    destination_tx_hash TEXT,
    detected_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    confirmed_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (chain_key, tx_hash, asset_id, deposit_address)
);

CREATE INDEX IF NOT EXISTS idx_deposit_transfers_channel_created
    ON deposit_transfers (channel_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_deposit_transfers_status_created
    ON deposit_transfers (status, risk_state, created_at DESC);

CREATE TABLE IF NOT EXISTS deposit_route_jobs (
    id UUID PRIMARY KEY,
    transfer_id UUID NOT NULL REFERENCES deposit_transfers(id) ON DELETE CASCADE,
    job_type TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'queued',
    attempts INTEGER NOT NULL DEFAULT 0,
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    response JSONB,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_deposit_route_jobs_transfer_type_unique
    ON deposit_route_jobs (transfer_id, job_type);

CREATE INDEX IF NOT EXISTS idx_deposit_route_jobs_status_created
    ON deposit_route_jobs (status, created_at ASC);

CREATE TABLE IF NOT EXISTS deposit_events (
    id UUID PRIMARY KEY,
    entity_type TEXT NOT NULL,
    entity_id UUID NOT NULL,
    event_type TEXT NOT NULL,
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_deposit_events_entity_created
    ON deposit_events (entity_type, entity_id, created_at ASC);

CREATE TABLE IF NOT EXISTS deposit_risk_flags (
    id UUID PRIMARY KEY,
    transfer_id UUID NOT NULL REFERENCES deposit_transfers(id) ON DELETE CASCADE,
    code TEXT NOT NULL,
    severity TEXT NOT NULL,
    description TEXT NOT NULL,
    resolution_status TEXT NOT NULL DEFAULT 'open',
    resolution_notes TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    resolved_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_deposit_risk_flags_transfer_created
    ON deposit_risk_flags (transfer_id, created_at DESC);

CREATE TABLE IF NOT EXISTS deposit_recoveries (
    id UUID PRIMARY KEY,
    transfer_id UUID NOT NULL REFERENCES deposit_transfers(id) ON DELETE CASCADE,
    reason TEXT NOT NULL,
    notes TEXT,
    requested_by TEXT,
    status TEXT NOT NULL DEFAULT 'open',
    resolution_notes TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    resolved_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_deposit_recoveries_transfer_created
    ON deposit_recoveries (transfer_id, created_at DESC);
