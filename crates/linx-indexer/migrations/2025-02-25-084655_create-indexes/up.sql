-- Your SQL goes here

-- event table
CREATE INDEX idx_event_transaction_id ON events(tx_id);
CREATE INDEX idx_event_contract_address ON events(contract_address);

-- block table
CREATE INDEX idx_block_chain_height ON blocks(chain_from, chain_to, height);
CREATE INDEX idx_block_timestamp ON blocks(timestamp);

-- transaction table
CREATE INDEX idx_transaction_block_height ON transactions(block_hash);

-- processor status table
CREATE INDEX idx_processor_status_name ON processor_status(processor);