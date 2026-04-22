CREATE TABLE IF NOT EXISTS blackjack_hand_views (
    hand_id UUID PRIMARY KEY REFERENCES blackjack_hands(id) ON DELETE CASCADE,
    snapshot JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
