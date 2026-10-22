-- Down migration
-- Add back the NOT NULL constraint
ALTER TABLE transactions ALTER COLUMN block_hash SET NOT NULL;

-- Add back the foreign key constraint
ALTER TABLE transactions 
ADD CONSTRAINT transactions_block_hash_fkey 
FOREIGN KEY (block_hash) REFERENCES blocks(hash) ON DELETE CASCADE;