-- Revert to the original unique index
DROP INDEX IF EXISTS unique_transfers;

-- Recreate the original unique index without account_transaction_id
CREATE UNIQUE INDEX unique_transfers ON transfers (tx_id, token_id, from_address, to_address, amount);

-- Drop the unique constraint on account_transactions
DROP INDEX IF EXISTS unique_account_transactions;
