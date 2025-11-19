-- Change points fields from NUMERIC to INTEGER in points_snapshots
ALTER TABLE points_snapshots
    ALTER COLUMN swap_points TYPE INTEGER USING swap_points::INTEGER,
    ALTER COLUMN supply_points TYPE INTEGER USING supply_points::INTEGER,
    ALTER COLUMN borrow_points TYPE INTEGER USING borrow_points::INTEGER,
    ALTER COLUMN base_points_total TYPE INTEGER USING base_points_total::INTEGER,
    ALTER COLUMN multiplier_points TYPE INTEGER USING multiplier_points::INTEGER,
    ALTER COLUMN referral_points TYPE INTEGER USING referral_points::INTEGER,
    ALTER COLUMN total_points TYPE INTEGER USING total_points::INTEGER;

-- Change points fields from NUMERIC to INTEGER in points_transactions
ALTER TABLE points_transactions
    ALTER COLUMN points_earned TYPE INTEGER USING points_earned::INTEGER;

-- points_per_usd and points_per_usd_per_day stay as NUMERIC (can be fractional like 0.5 points per USD)
-- multiplier values stay as NUMERIC (can be fractional like 1.5x)

COMMENT ON COLUMN points_snapshots.swap_points IS 'Points earned from swaps (integer)';
COMMENT ON COLUMN points_snapshots.supply_points IS 'Points earned from supplying (integer)';
COMMENT ON COLUMN points_snapshots.borrow_points IS 'Points earned from borrowing (integer)';
COMMENT ON COLUMN points_snapshots.total_points IS 'Total accumulated points (integer)';
COMMENT ON COLUMN points_transactions.points_earned IS 'Points earned in this transaction (integer)';
