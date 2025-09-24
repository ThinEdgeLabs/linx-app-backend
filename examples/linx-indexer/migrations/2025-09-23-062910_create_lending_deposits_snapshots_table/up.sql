CREATE TABLE lending_deposits_snapshots (
    id BIGSERIAL PRIMARY KEY,
    address TEXT NOT NULL,
    market_id TEXT NOT NULL,
    amount NUMERIC NOT NULL,
    amount_usd NUMERIC NOT NULL,
    timestamp TIMESTAMP NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_lending_deposits_snapshots_created_at ON lending_deposits_snapshots(address, market_id, timestamp DESC);