CREATE TABLE IF NOT EXISTS privy_starknet_wallets (
    privy_user_id TEXT PRIMARY KEY,
    wallet_id TEXT NOT NULL UNIQUE,
    wallet_address TEXT NOT NULL UNIQUE,
    public_key TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
