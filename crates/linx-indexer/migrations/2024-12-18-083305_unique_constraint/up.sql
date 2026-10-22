-- Your SQL goes here
ALTER TABLE events 
ADD CONSTRAINT unique_tx_event UNIQUE (tx_id, event_index);