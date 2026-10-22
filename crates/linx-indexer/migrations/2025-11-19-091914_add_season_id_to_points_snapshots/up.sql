-- Add season_id column to points_snapshots
ALTER TABLE points_snapshots
ADD COLUMN season_id INTEGER NOT NULL DEFAULT 1 REFERENCES points_seasons(id);

-- Remove the default after migration
ALTER TABLE points_snapshots ALTER COLUMN season_id DROP DEFAULT;

-- Drop old unique constraint
ALTER TABLE points_snapshots DROP CONSTRAINT IF EXISTS points_snapshots_address_snapshot_date_key;

-- Add new unique constraint including season_id
ALTER TABLE points_snapshots
ADD CONSTRAINT points_snapshots_address_date_season_key
UNIQUE(address, snapshot_date, season_id);

-- Drop old indexes
DROP INDEX IF EXISTS idx_points_snapshots_address_date;
DROP INDEX IF EXISTS idx_points_snapshots_date_points;

-- Create new indexes with season_id
CREATE INDEX idx_points_snapshots_season_address_date
ON points_snapshots(season_id, address, snapshot_date DESC);

CREATE INDEX idx_points_snapshots_season_date_points
ON points_snapshots(season_id, snapshot_date, total_points DESC);

COMMENT ON COLUMN points_snapshots.season_id IS 'References the season this snapshot belongs to';
