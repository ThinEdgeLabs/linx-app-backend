-- Add snapshot_date column to points_transactions table
ALTER TABLE points_transactions
ADD COLUMN snapshot_date DATE NOT NULL DEFAULT CURRENT_DATE;

-- Create index for efficient deletion by date
CREATE INDEX idx_points_transactions_snapshot_date ON points_transactions(snapshot_date);
