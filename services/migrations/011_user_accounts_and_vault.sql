CREATE TABLE IF NOT EXISTS player_wallets (
    wallet_address TEXT PRIMARY KEY,
    player_id UUID NOT NULL REFERENCES players(id) ON DELETE CASCADE,
    linked_via TEXT NOT NULL DEFAULT 'wallet',
    wallet_kind TEXT NOT NULL DEFAULT 'execution',
    is_primary BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_player_wallets_player_id
    ON player_wallets (player_id, created_at DESC);

CREATE UNIQUE INDEX IF NOT EXISTS idx_player_wallets_primary_unique
    ON player_wallets (player_id)
    WHERE is_primary = TRUE;

INSERT INTO player_wallets (
    wallet_address,
    player_id,
    linked_via,
    wallet_kind,
    is_primary,
    created_at,
    last_seen_at
)
SELECT
    LOWER(wallet_address),
    id,
    'legacy',
    'execution',
    TRUE,
    first_seen_at,
    last_seen_at
FROM players
ON CONFLICT (wallet_address) DO NOTHING;

CREATE TABLE IF NOT EXISTS player_auth_identities (
    auth_provider TEXT NOT NULL,
    auth_subject TEXT NOT NULL,
    player_id UUID NOT NULL REFERENCES players(id) ON DELETE CASCADE,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (auth_provider, auth_subject)
);

CREATE INDEX IF NOT EXISTS idx_player_auth_identities_player_id
    ON player_auth_identities (player_id, created_at DESC);

DO $$
BEGIN
    IF to_regclass('public.privy_starknet_wallets') IS NOT NULL THEN
        INSERT INTO player_auth_identities (
            auth_provider,
            auth_subject,
            player_id,
            metadata
        )
        SELECT
            'privy',
            psw.privy_user_id,
            p.id,
            jsonb_build_object(
                'wallet_id', psw.wallet_id,
                'wallet_address', LOWER(psw.wallet_address),
                'public_key', psw.public_key
            )
        FROM privy_starknet_wallets psw
        INNER JOIN players p ON LOWER(p.wallet_address) = LOWER(psw.wallet_address)
        ON CONFLICT (auth_provider, auth_subject) DO NOTHING;
    END IF;
END $$;

ALTER TABLE player_profiles
    ADD COLUMN IF NOT EXISTS player_id UUID;

UPDATE player_profiles pp
SET player_id = p.id
FROM players p
WHERE pp.player_id IS NULL
  AND LOWER(pp.wallet_address) = LOWER(p.wallet_address);

CREATE UNIQUE INDEX IF NOT EXISTS idx_player_profiles_player_id
    ON player_profiles (player_id)
    WHERE player_id IS NOT NULL;

ALTER TABLE bankroll_accounts
    ADD COLUMN IF NOT EXISTS gambling_balance TEXT NOT NULL DEFAULT '0';

ALTER TABLE bankroll_accounts
    ADD COLUMN IF NOT EXISTS gambling_reserved TEXT NOT NULL DEFAULT '0';

ALTER TABLE bankroll_accounts
    ADD COLUMN IF NOT EXISTS vault_balance TEXT NOT NULL DEFAULT '0';

UPDATE bankroll_accounts
SET
    gambling_balance = public_balance,
    gambling_reserved = reserved_balance
WHERE gambling_balance = '0'
  AND gambling_reserved = '0'
  AND (public_balance <> '0' OR reserved_balance <> '0');

CREATE TABLE IF NOT EXISTS balance_reservations (
    id UUID PRIMARY KEY,
    player_id UUID NOT NULL REFERENCES players(id) ON DELETE CASCADE,
    game_kind TEXT NOT NULL,
    reference_id TEXT NOT NULL,
    amount TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    payout_amount TEXT,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_balance_reservations_active_unique
    ON balance_reservations (player_id, game_kind, reference_id)
    WHERE status = 'active';

CREATE INDEX IF NOT EXISTS idx_balance_reservations_player_created
    ON balance_reservations (player_id, created_at DESC);

CREATE TABLE IF NOT EXISTS balance_ledger_entries (
    id UUID PRIMARY KEY,
    player_id UUID NOT NULL REFERENCES players(id) ON DELETE CASCADE,
    balance_scope TEXT NOT NULL,
    entry_kind TEXT NOT NULL,
    amount_delta TEXT NOT NULL,
    reference_kind TEXT,
    reference_id TEXT,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_balance_ledger_entries_player_created
    ON balance_ledger_entries (player_id, created_at DESC);
