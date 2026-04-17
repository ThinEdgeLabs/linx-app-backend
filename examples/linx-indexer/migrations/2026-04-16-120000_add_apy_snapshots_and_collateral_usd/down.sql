ALTER TABLE lending_position_snapshots
    DROP COLUMN IF EXISTS collateral_amount_usd,
    DROP COLUMN IF EXISTS collateral_amount;

DROP TABLE IF EXISTS market_apy_snapshots;
