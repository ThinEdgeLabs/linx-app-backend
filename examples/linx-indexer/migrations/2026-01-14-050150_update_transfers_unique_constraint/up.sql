-- Add unique constraint to account_transactions to ensure idempotency
-- This prevents duplicate account_transaction records when blocks are reprocessed
CREATE UNIQUE INDEX IF NOT EXISTS unique_account_transactions ON account_transactions (tx_id, address, tx_type);

-- Drop the old unique index that prevented duplicate transfers
DROP INDEX IF EXISTS unique_transfers;

-- Create new unique index that includes account_transaction_id
-- This allows the same transfer to exist twice (once for sender, once for receiver)
-- while still preventing duplicate inserts from the same account_transaction
CREATE UNIQUE INDEX unique_transfers ON transfers (tx_id, token_id, from_address, to_address, amount, account_transaction_id);
