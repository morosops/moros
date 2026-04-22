ALTER TABLE deposit_channels
    DROP CONSTRAINT IF EXISTS deposit_channels_deposit_address_key;

CREATE INDEX IF NOT EXISTS idx_deposit_channels_deposit_address_lookup
    ON deposit_channels (LOWER(deposit_address));
