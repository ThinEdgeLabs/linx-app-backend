-- Drop composite indexes
DROP INDEX IF EXISTS idx_points_snapshots_address_date;
DROP INDEX IF EXISTS idx_points_snapshots_date_points;

-- Restore original single-column indexes
CREATE INDEX idx_points_snapshots_address ON points_snapshots(address);
CREATE INDEX idx_points_snapshots_date ON points_snapshots(snapshot_date);
CREATE INDEX idx_points_snapshots_total_points ON points_snapshots(total_points DESC);
