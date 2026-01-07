-- Remove the composite index on tx_type and timestamp
DROP INDEX IF EXISTS idx_account_transactions_tx_type_timestamp;
