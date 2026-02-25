-- Supports the all-markets user position history query which filters by
-- address and range-scans on timestamp without filtering on market_id.
CREATE INDEX idx_lending_position_snapshots_address_timestamp
    ON lending_position_snapshots(address, timestamp DESC);
