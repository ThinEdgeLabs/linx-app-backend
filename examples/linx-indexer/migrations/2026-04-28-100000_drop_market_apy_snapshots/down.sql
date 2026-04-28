CREATE TABLE market_apy_snapshots (
    id SERIAL PRIMARY KEY,
    market_id TEXT NOT NULL,
    borrow_rate NUMERIC NOT NULL,
    supply_rate NUMERIC NOT NULL,
    snapshot_timestamp TIMESTAMP NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    UNIQUE(market_id, snapshot_timestamp)
);

CREATE INDEX idx_market_apy_market_time
    ON market_apy_snapshots(market_id, snapshot_timestamp DESC);

CREATE INDEX idx_market_apy_time
    ON market_apy_snapshots(snapshot_timestamp DESC);
