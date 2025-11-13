-- Drop existing single-column indexes that will be replaced by composite indexes
DROP INDEX IF EXISTS idx_points_snapshots_address;
DROP INDEX IF EXISTS idx_points_snapshots_date;
DROP INDEX IF EXISTS idx_points_snapshots_total_points;

-- Create composite index for queries filtering by address and ordering by snapshot_date
-- Covers: get_user_snapshots, get_latest_snapshot
CREATE INDEX idx_points_snapshots_address_date ON points_snapshots(address, snapshot_date DESC);

-- Create composite index for leaderboard queries (filter by date, order by points)
-- Covers: get_leaderboard
CREATE INDEX idx_points_snapshots_date_points ON points_snapshots(snapshot_date, total_points DESC);

-- Note: The UNIQUE constraint on (address, snapshot_date) already provides efficient lookups for get_snapshot
