-- Remove points_transactions table as it's not being used
-- Points totals are already stored in points_snapshots table
DROP TABLE IF EXISTS points_transactions;
