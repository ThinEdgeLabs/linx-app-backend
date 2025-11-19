-- Drop index
DROP INDEX IF EXISTS idx_points_transactions_season_date;

-- Remove season_id column
ALTER TABLE points_transactions DROP COLUMN IF EXISTS season_id;
