-- Add composite index for efficient filtering by tx_type and timestamp
-- This index significantly improves performance for get_swaps_in_period queries
-- used by the points calculator when processing daily swap transactions
CREATE INDEX idx_account_transactions_tx_type_timestamp
ON account_transactions(tx_type, timestamp);
