-- Recreate the gap-detection index (duplicate of idx_block_chain_height).
CREATE INDEX IF NOT EXISTS idx_blocks_chain_height ON blocks(chain_from, chain_to, height);
