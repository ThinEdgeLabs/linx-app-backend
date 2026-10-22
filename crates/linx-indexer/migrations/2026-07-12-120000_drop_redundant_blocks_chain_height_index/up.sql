-- idx_blocks_chain_height duplicates idx_block_chain_height (both are btree indexes on
-- blocks(chain_from, chain_to, height), created by the bento-types create-indexes migration).
-- The duplicate only adds write/maintenance overhead on every block insert and delete; the
-- canonical idx_block_chain_height still serves the gap_detector's window scan.
DROP INDEX IF EXISTS idx_blocks_chain_height;
