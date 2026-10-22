ALTER TABLE market_state_snapshots
    ADD COLUMN bad_debt_usd NUMERIC NOT NULL DEFAULT 0;
