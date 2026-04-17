CREATE TABLE lending_stats_snapshots (
    id SERIAL PRIMARY KEY,
    total_supply_usd NUMERIC NOT NULL,
    total_borrow_usd NUMERIC NOT NULL,
    total_collateral_usd NUMERIC NOT NULL,
    tvl_usd NUMERIC NOT NULL,
    apy_30d_avg NUMERIC NOT NULL,
    snapshot_timestamp TIMESTAMP NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_lending_stats_snapshots_timestamp
    ON lending_stats_snapshots(snapshot_timestamp DESC);
