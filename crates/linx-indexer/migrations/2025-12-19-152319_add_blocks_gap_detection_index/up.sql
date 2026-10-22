-- Add index to optimize gap detection queries
-- This index helps with the window function query that partitions by chain and orders by height
CREATE INDEX IF NOT EXISTS idx_blocks_chain_height ON blocks(chain_from, chain_to, height);
