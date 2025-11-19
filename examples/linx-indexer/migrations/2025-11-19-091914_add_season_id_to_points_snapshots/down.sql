-- Drop new indexes
DROP INDEX IF EXISTS idx_points_snapshots_season_address_date;
DROP INDEX IF EXISTS idx_points_snapshots_season_date_points;

-- Drop new unique constraint
ALTER TABLE points_snapshots DROP CONSTRAINT IF EXISTS points_snapshots_address_date_season_key;

-- Restore old unique constraint
ALTER TABLE points_snapshots
ADD CONSTRAINT points_snapshots_address_snapshot_date_key
UNIQUE(address, snapshot_date);

-- Restore old indexes
CREATE INDEX idx_points_snapshots_address_date ON points_snapshots(address, snapshot_date DESC);
CREATE INDEX idx_points_snapshots_date_points ON points_snapshots(snapshot_date, total_points DESC);

-- Remove season_id column
ALTER TABLE points_snapshots DROP COLUMN IF EXISTS season_id;
