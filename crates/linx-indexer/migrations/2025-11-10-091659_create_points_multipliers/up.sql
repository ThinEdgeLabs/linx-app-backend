CREATE TABLE points_multipliers (
    id SERIAL PRIMARY KEY,
    multiplier_type TEXT NOT NULL,
    threshold_value NUMERIC NOT NULL,
    multiplier NUMERIC NOT NULL,
    is_active BOOLEAN DEFAULT true NOT NULL,
    created_at TIMESTAMP DEFAULT NOW() NOT NULL
);

CREATE INDEX idx_points_multipliers_type ON points_multipliers(multiplier_type);
CREATE INDEX idx_points_multipliers_active ON points_multipliers(is_active);

COMMENT ON TABLE points_multipliers IS 'Multipliers for earning extra points based on various criteria';
COMMENT ON COLUMN points_multipliers.multiplier_type IS 'Type of multiplier: volume, token_holdings, time_based, etc.';
COMMENT ON COLUMN points_multipliers.threshold_value IS 'Minimum value to activate this multiplier (e.g., USD volume, token amount)';
COMMENT ON COLUMN points_multipliers.multiplier IS 'Multiplier to apply (e.g., 0.1 = 10% bonus, 0.5 = 50% bonus)';
