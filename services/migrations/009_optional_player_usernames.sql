ALTER TABLE player_profiles
    ALTER COLUMN username DROP NOT NULL;

ALTER TABLE player_profiles
    DROP CONSTRAINT IF EXISTS player_profiles_username_key;

DROP INDEX IF EXISTS player_profiles_username_idx;

CREATE UNIQUE INDEX IF NOT EXISTS player_profiles_username_unique_idx
    ON player_profiles (username)
    WHERE username IS NOT NULL;

CREATE INDEX IF NOT EXISTS player_profiles_username_idx
    ON player_profiles (username)
    WHERE username IS NOT NULL;
