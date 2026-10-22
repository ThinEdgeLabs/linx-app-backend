-- Drop indexes
DROP INDEX IF EXISTS idx_lending_position_snapshots_timestamp;
DROP INDEX IF EXISTS idx_lending_position_snapshots_address_market_timestamp;

-- Add back created_at column
ALTER TABLE lending_position_snapshots ADD COLUMN created_at TIMESTAMP NOT NULL DEFAULT NOW();

-- Drop borrow columns
ALTER TABLE lending_position_snapshots DROP COLUMN borrow_amount_usd;
ALTER TABLE lending_position_snapshots DROP COLUMN borrow_amount;

-- Rename columns back
ALTER TABLE lending_position_snapshots RENAME COLUMN supply_amount_usd TO amount_usd;
ALTER TABLE lending_position_snapshots RENAME COLUMN supply_amount TO amount;

-- Rename table back
ALTER TABLE lending_position_snapshots RENAME TO lending_deposits_snapshots;

-- Recreate original index if it existed
CREATE INDEX IF NOT EXISTS idx_lending_deposits_snapshots_address_market_timestamp
    ON lending_deposits_snapshots(address, market_id, timestamp DESC);
