-- Add columns for multi-hop swap support
ALTER TABLE swaps ADD COLUMN hop_count INTEGER DEFAULT 1 NOT NULL;
ALTER TABLE swaps ADD COLUMN hop_sequence INTEGER;

-- Create indexes for efficient querying
CREATE INDEX idx_swaps_hop_sequence_null ON swaps (hop_sequence) WHERE hop_sequence IS NULL;
CREATE INDEX idx_swaps_tx_id_sequence ON swaps (tx_id, hop_sequence);
