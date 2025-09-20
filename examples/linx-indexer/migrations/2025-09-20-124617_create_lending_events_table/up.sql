CREATE TABLE lending_events (
  id BIGSERIAL PRIMARY KEY,
  market_id TEXT NOT NULL,
  event_type TEXT NOT NULL,
  token_id TEXT NOT NULL,
  on_behalf TEXT NOT NULL,
  amount NUMERIC NOT NULL,
  transaction_id TEXT NOT NULL,
  event_index INTEGER NOT NULL,
  block_time TIMESTAMP NOT NULL,
  created_at TIMESTAMP NOT NULL,
  fields JSONB NOT NULL,
  UNIQUE(transaction_id, event_index)
);

CREATE INDEX idx_lending_events_market_type_time ON lending_events(market_id, event_type, block_time DESC);