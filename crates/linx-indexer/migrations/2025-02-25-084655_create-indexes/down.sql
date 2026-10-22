-- This file should undo anything in `up.sql`
DROP INDEX idx_event_contract_address IF EXISTS;
DROP INDEX idx_event_transaction_id IF EXISTS;

DROP INDEX idx_block_hash IF EXISTS;
DROP INDEX idx_block_chain_from IF EXISTS;
DROP INDEX idx_block_chain_to IF EXISTS;
DROP INDEX idx_block_height IF EXISTS;

DROP INDEX idx_transaction_block_height IF EXISTS;

DROP INDEX idx_processor_status_name IF EXISTS;

