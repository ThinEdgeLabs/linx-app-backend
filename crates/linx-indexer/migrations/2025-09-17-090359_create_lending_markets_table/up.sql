CREATE TABLE lending_markets (
  id TEXT PRIMARY KEY,
  market_contract_id TEXT UNIQUE NOT NULL,
  collateral_token TEXT NOT NULL,
  loan_token TEXT NOT NULL,
  oracle TEXT NOT NULL,
  irm TEXT NOT NULL,
  ltv NUMERIC NOT NULL,
  created_at TIMESTAMP NOT NULL
);

CREATE INDEX idx_lending_markets_created_at ON lending_markets(created_at ASC);