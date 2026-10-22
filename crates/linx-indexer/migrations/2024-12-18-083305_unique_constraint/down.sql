-- This file should undo anything in `up.sql`
-- Start transaction
BEGIN;

-- Try to add the constraint
ALTER TABLE events 
DROP CONSTRAINT unique_tx_event
