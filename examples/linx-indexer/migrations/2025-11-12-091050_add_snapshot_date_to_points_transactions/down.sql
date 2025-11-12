-- Remove snapshot_date column from points_transactions table
DROP INDEX IF EXISTS idx_points_transactions_snapshot_date;
ALTER TABLE points_transactions
DROP COLUMN snapshot_date;
