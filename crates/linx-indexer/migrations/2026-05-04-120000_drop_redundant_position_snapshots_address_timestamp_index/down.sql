CREATE INDEX idx_lending_position_snapshots_address_timestamp
    ON lending_position_snapshots(address, timestamp DESC);
