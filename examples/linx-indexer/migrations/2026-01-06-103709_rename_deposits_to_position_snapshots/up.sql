-- Rename table from lending_deposits_snapshots to lending_position_snapshots
ALTER TABLE lending_deposits_snapshots RENAME TO lending_position_snapshots;

-- Rename existing columns
ALTER TABLE lending_position_snapshots RENAME COLUMN amount TO supply_amount;
ALTER TABLE lending_position_snapshots RENAME COLUMN amount_usd TO supply_amount_usd;

-- Add borrow columns
ALTER TABLE lending_position_snapshots ADD COLUMN borrow_amount NUMERIC NOT NULL DEFAULT 0;
ALTER TABLE lending_position_snapshots ADD COLUMN borrow_amount_usd NUMERIC NOT NULL DEFAULT 0;

-- Drop created_at column (redundant with timestamp)
ALTER TABLE lending_position_snapshots DROP COLUMN created_at;

-- Update indexes if any exist
DROP INDEX IF EXISTS idx_lending_deposits_snapshots_address_market_timestamp;
CREATE INDEX idx_lending_position_snapshots_address_market_timestamp
    ON lending_position_snapshots(address, market_id, timestamp DESC);

CREATE INDEX idx_lending_position_snapshots_timestamp
    ON lending_position_snapshots(timestamp DESC);
