-- Drop old account_transactions and child tables (data will be reindexed)
DROP TABLE IF EXISTS transfers CASCADE;
DROP TABLE IF EXISTS swaps CASCADE;
DROP TABLE IF EXISTS contract_calls CASCADE;
DROP TABLE IF EXISTS account_transactions CASCADE;

-- Create new account_transactions table with JSONB details
CREATE TABLE account_transactions (
    id BIGSERIAL PRIMARY KEY,
    address TEXT NOT NULL,
    tx_type TEXT NOT NULL,
    tx_id TEXT NOT NULL,
    from_group SMALLINT NOT NULL,
    to_group SMALLINT NOT NULL,
    block_height BIGINT NOT NULL,
    timestamp TIMESTAMP NOT NULL,
    details JSONB NOT NULL,

    -- Generated column for idempotency: hash of the details
    tx_key TEXT GENERATED ALWAYS AS (
        tx_id || ':' || address || ':' || tx_type || ':' || md5(details::text)
    ) STORED
);

-- Idempotency: unique constraint on tx_key
CREATE UNIQUE INDEX unique_account_transactions ON account_transactions (tx_key);

-- Query performance: index for fetching transactions by address
CREATE INDEX idx_account_transactions_address_time ON account_transactions (address, timestamp DESC);

-- Additional index for filtering by tx_type
CREATE INDEX idx_account_transactions_type_time ON account_transactions (tx_type, timestamp DESC);

-- Index on tx_id for lookups
CREATE INDEX idx_account_transactions_tx_id ON account_transactions (tx_id);
