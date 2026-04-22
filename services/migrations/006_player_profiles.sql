CREATE TABLE IF NOT EXISTS player_profiles (
    wallet_address TEXT PRIMARY KEY,
    username TEXT NOT NULL UNIQUE,
    auth_provider TEXT NOT NULL DEFAULT 'wallet',
    auth_subject TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS player_profiles_username_idx
    ON player_profiles (username);
