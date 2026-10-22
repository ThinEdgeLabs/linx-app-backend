-- Per-market APY readings fetched from the IRM contract on a scheduler.
-- Rates are stored already-annualized as decimals (e.g. 0.0543 = 5.43%) so the
-- 30-day rolling average query is a plain AVG() and decoupled from on-chain scale.
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

-- Track collateral USD on position snapshots so the stats aggregate can SUM
-- it directly instead of re-walking every user's events per request.
ALTER TABLE lending_position_snapshots
    ADD COLUMN collateral_amount NUMERIC NOT NULL DEFAULT 0,
    ADD COLUMN collateral_amount_usd NUMERIC NOT NULL DEFAULT 0;
