-- This file should undo anything in `up.sql`
ALTER TABLE blocks DROP COLUMN main_chain;

ALTER TABLE transactions DROP COLUMN main_chain;
ALTER TABLE transactions DROP COLUMN block_hash;