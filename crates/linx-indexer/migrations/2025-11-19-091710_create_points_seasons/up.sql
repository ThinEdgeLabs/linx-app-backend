-- Create points_seasons table
CREATE TABLE points_seasons (
    id SERIAL PRIMARY KEY,
    season_number INTEGER UNIQUE NOT NULL,
    start_date DATE NOT NULL,
    end_date DATE NOT NULL,
    max_tokens_distribution NUMERIC NOT NULL,
    is_active BOOLEAN DEFAULT false NOT NULL,
    created_at TIMESTAMP DEFAULT NOW() NOT NULL
);

-- Ensure only one active season at a time
CREATE UNIQUE INDEX idx_points_seasons_active ON points_seasons(is_active) WHERE is_active = true;

-- Index for season number lookups
CREATE INDEX idx_points_seasons_number ON points_seasons(season_number);

-- Add comments
COMMENT ON TABLE points_seasons IS 'Defines points seasons with token distribution caps';
COMMENT ON COLUMN points_seasons.season_number IS 'Sequential season identifier (1, 2, 3...)';
COMMENT ON COLUMN points_seasons.max_tokens_distribution IS 'Maximum tokens to distribute at end of season';
COMMENT ON INDEX idx_points_seasons_active IS 'Ensures only one season can be active at a time';

-- Insert initial season for existing data
INSERT INTO points_seasons (season_number, start_date, end_date, max_tokens_distribution, is_active)
VALUES (1, '2025-01-01', '2025-12-31', 1000000, true);
