CREATE INDEX idx_lending_position_snapshots_history_cover
    ON lending_position_snapshots(address, timestamp DESC)
    INCLUDE (supply_amount_usd, borrow_amount_usd);
