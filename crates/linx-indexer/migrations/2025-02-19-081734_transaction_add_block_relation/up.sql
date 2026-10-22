-- First drop the existing column
ALTER TABLE transactions DROP COLUMN IF EXISTS block_hash;

-- Then add it back with the correct constraints
ALTER TABLE transactions 
ADD COLUMN block_hash TEXT NOT NULL REFERENCES blocks(hash) ON DELETE CASCADE;