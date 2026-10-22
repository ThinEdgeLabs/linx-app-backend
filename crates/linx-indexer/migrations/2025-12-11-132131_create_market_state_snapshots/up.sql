-- Create market state snapshots table
-- This table stores periodic snapshots of market totals to track how they change over time
-- Used for calculating historical position values and interest accrual

CREATE TABLE market_state_snapshots (
    id SERIAL PRIMARY KEY,
    market_id TEXT NOT NULL,

    -- Market totals (source of truth for share-to-asset conversion)
    total_supply_assets NUMERIC NOT NULL,
    total_supply_shares NUMERIC NOT NULL,
    total_borrow_assets NUMERIC NOT NULL,
    total_borrow_shares NUMERIC NOT NULL,

    -- Interest rate (useful for analytics)
    interest_rate NUMERIC,

    snapshot_timestamp TIMESTAMP NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),

    UNIQUE(market_id, snapshot_timestamp)
);

-- Index for querying market state at specific times
CREATE INDEX idx_market_state_market_time
    ON market_state_snapshots(market_id, snapshot_timestamp DESC);

-- Index for efficient lookups by market
CREATE INDEX idx_market_state_market
    ON market_state_snapshots(market_id);
