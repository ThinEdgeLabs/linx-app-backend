-- Revert points fields from INTEGER back to NUMERIC in points_snapshots
ALTER TABLE points_snapshots
    ALTER COLUMN swap_points TYPE NUMERIC USING swap_points::NUMERIC,
    ALTER COLUMN supply_points TYPE NUMERIC USING supply_points::NUMERIC,
    ALTER COLUMN borrow_points TYPE NUMERIC USING borrow_points::NUMERIC,
    ALTER COLUMN base_points_total TYPE NUMERIC USING base_points_total::NUMERIC,
    ALTER COLUMN multiplier_points TYPE NUMERIC USING multiplier_points::NUMERIC,
    ALTER COLUMN referral_points TYPE NUMERIC USING referral_points::NUMERIC,
    ALTER COLUMN total_points TYPE NUMERIC USING total_points::NUMERIC;

-- Revert points fields from INTEGER back to NUMERIC in points_transactions
ALTER TABLE points_transactions
    ALTER COLUMN points_earned TYPE NUMERIC USING points_earned::NUMERIC;
