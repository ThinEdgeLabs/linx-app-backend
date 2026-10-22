-- Remove the foreign key constraint
ALTER TABLE transactions DROP CONSTRAINT IF EXISTS transactions_block_hash_fkey;

-- Remove the NOT NULL constraint
ALTER TABLE transactions ALTER COLUMN block_hash DROP NOT NULL;
