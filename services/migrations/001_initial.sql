CREATE TABLE IF NOT EXISTS players (
    id UUID PRIMARY KEY,
    wallet_address TEXT NOT NULL UNIQUE,
    first_seen_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS bankroll_accounts (
    player_id UUID PRIMARY KEY REFERENCES players(id) ON DELETE CASCADE,
    public_balance TEXT NOT NULL DEFAULT '0',
    reserved_balance TEXT NOT NULL DEFAULT '0',
    bankroll_status TEXT NOT NULL DEFAULT 'active',
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS blackjack_hands (
    id UUID PRIMARY KEY,
    player_id UUID NOT NULL REFERENCES players(id) ON DELETE CASCADE,
    table_id BIGINT NOT NULL,
    wager TEXT NOT NULL,
    status TEXT NOT NULL,
    phase TEXT NOT NULL,
    transcript_root TEXT NOT NULL,
    active_seat SMALLINT NOT NULL DEFAULT 0,
    seat_count SMALLINT NOT NULL DEFAULT 1,
    dealer_upcard SMALLINT,
    chain_hand_id BIGINT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_blackjack_hands_player_created
    ON blackjack_hands (player_id, created_at DESC);

CREATE TABLE IF NOT EXISTS game_sessions (
    id UUID PRIMARY KEY,
    player_id UUID NOT NULL REFERENCES players(id) ON DELETE CASCADE,
    hand_id UUID NOT NULL UNIQUE REFERENCES blackjack_hands(id) ON DELETE CASCADE,
    session_key TEXT NOT NULL,
    table_id BIGINT NOT NULL,
    game TEXT NOT NULL,
    status TEXT NOT NULL,
    phase TEXT NOT NULL,
    transcript_root TEXT NOT NULL,
    max_wager TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_game_sessions_player_created
    ON game_sessions (player_id, created_at DESC);

CREATE TABLE IF NOT EXISTS blackjack_hand_events (
    id UUID PRIMARY KEY,
    hand_id UUID NOT NULL REFERENCES blackjack_hands(id) ON DELETE CASCADE,
    session_id UUID REFERENCES game_sessions(id) ON DELETE SET NULL,
    event_type TEXT NOT NULL,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_blackjack_hand_events_hand_created
    ON blackjack_hand_events (hand_id, created_at ASC);
