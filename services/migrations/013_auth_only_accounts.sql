ALTER TABLE players
    ALTER COLUMN wallet_address DROP NOT NULL;

ALTER TABLE player_profiles
    DROP CONSTRAINT IF EXISTS player_profiles_pkey;

ALTER TABLE player_profiles
    ALTER COLUMN wallet_address DROP NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_player_profiles_wallet_address_unique
    ON player_profiles (wallet_address)
    WHERE wallet_address IS NOT NULL;

ALTER TABLE withdrawal_requests
    ALTER COLUMN requested_by_wallet DROP NOT NULL;
