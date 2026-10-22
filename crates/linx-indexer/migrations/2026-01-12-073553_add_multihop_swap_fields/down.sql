-- Remove indexes
DROP INDEX IF EXISTS idx_swaps_tx_id_sequence;
DROP INDEX IF EXISTS idx_swaps_hop_sequence_null;

-- Remove columns
ALTER TABLE swaps DROP COLUMN hop_sequence;
ALTER TABLE swaps DROP COLUMN hop_count;
