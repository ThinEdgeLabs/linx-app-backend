CREATE TABLE points_transactions (
    id SERIAL PRIMARY KEY,
    address TEXT NOT NULL,
    action_type TEXT NOT NULL,
    transaction_id TEXT,
    amount_usd NUMERIC NOT NULL,
    points_earned NUMERIC NOT NULL,
    created_at TIMESTAMP DEFAULT NOW() NOT NULL
);

CREATE INDEX idx_points_transactions_address ON points_transactions(address);
CREATE INDEX idx_points_transactions_created_at ON points_transactions(created_at);
CREATE INDEX idx_points_transactions_action_type ON points_transactions(action_type);

COMMENT ON TABLE points_transactions IS 'Detailed audit trail of individual point-earning actions';
COMMENT ON COLUMN points_transactions.action_type IS 'Type of action: swap, supply, borrow, etc.';
COMMENT ON COLUMN points_transactions.transaction_id IS 'Blockchain transaction ID if applicable';
