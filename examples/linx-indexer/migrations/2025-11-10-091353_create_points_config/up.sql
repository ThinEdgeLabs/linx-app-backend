CREATE TABLE points_config (
    id SERIAL PRIMARY KEY,
    action_type TEXT NOT NULL UNIQUE,
    points_per_usd NUMERIC,
    points_per_usd_per_day NUMERIC,
    is_active BOOLEAN DEFAULT true NOT NULL,
    created_at TIMESTAMP DEFAULT NOW() NOT NULL,
    updated_at TIMESTAMP DEFAULT NOW() NOT NULL
);

COMMENT ON TABLE points_config IS 'Configuration for point earning rules per action type';
COMMENT ON COLUMN points_config.action_type IS 'Type of action: swap, supply, borrow, etc.';
COMMENT ON COLUMN points_config.points_per_usd IS 'Points earned per USD for instant actions (swap, borrow)';
COMMENT ON COLUMN points_config.points_per_usd_per_day IS 'Points earned per USD per day for time-based actions (supply)';
