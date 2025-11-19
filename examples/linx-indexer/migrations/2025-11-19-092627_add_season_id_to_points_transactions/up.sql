-- Add season_id column to points_transactions
ALTER TABLE points_transactions
ADD COLUMN season_id INTEGER NOT NULL DEFAULT 1 REFERENCES points_seasons(id);

-- Remove the default after migration
ALTER TABLE points_transactions ALTER COLUMN season_id DROP DEFAULT;

-- Create index for efficient queries by season and date
CREATE INDEX idx_points_transactions_season_date
ON points_transactions(season_id, snapshot_date);

COMMENT ON COLUMN points_transactions.season_id IS 'References the season this transaction belongs to';
