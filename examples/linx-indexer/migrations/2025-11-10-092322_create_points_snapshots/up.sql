CREATE TABLE points_snapshots (
    id SERIAL PRIMARY KEY,
    address TEXT NOT NULL,
    snapshot_date DATE NOT NULL,
    swap_points NUMERIC DEFAULT 0 NOT NULL,
    supply_points NUMERIC DEFAULT 0 NOT NULL,
    borrow_points NUMERIC DEFAULT 0 NOT NULL,
    base_points_total NUMERIC DEFAULT 0 NOT NULL,
    multiplier_type TEXT,
    multiplier_value NUMERIC DEFAULT 0 NOT NULL,
    multiplier_points NUMERIC DEFAULT 0 NOT NULL,
    referral_points NUMERIC DEFAULT 0 NOT NULL,
    total_points NUMERIC DEFAULT 0 NOT NULL,
    total_volume_usd NUMERIC DEFAULT 0 NOT NULL,
    created_at TIMESTAMP DEFAULT NOW() NOT NULL,
    UNIQUE(address, snapshot_date)
);

CREATE INDEX idx_points_snapshots_address ON points_snapshots(address);
CREATE INDEX idx_points_snapshots_date ON points_snapshots(snapshot_date);
CREATE INDEX idx_points_snapshots_total_points ON points_snapshots(total_points DESC);

COMMENT ON TABLE points_snapshots IS 'Aggregated points per address per snapshot period';
COMMENT ON COLUMN points_snapshots.base_points_total IS 'Sum of all action points before multipliers';
COMMENT ON COLUMN points_snapshots.multiplier_type IS 'Type of multiplier applied (volume, token_holdings, etc.)';
COMMENT ON COLUMN points_snapshots.multiplier_value IS 'The multiplier value applied';
COMMENT ON COLUMN points_snapshots.multiplier_points IS 'Bonus points from multipliers';
COMMENT ON COLUMN points_snapshots.referral_points IS 'Points earned from referrals';
COMMENT ON COLUMN points_snapshots.total_points IS 'Final total points including all bonuses';
