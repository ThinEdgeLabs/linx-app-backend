ALTER TABLE market_state_snapshots
    ADD COLUMN total_collateral_assets NUMERIC NOT NULL DEFAULT 0,
    ADD COLUMN total_supply_usd NUMERIC NOT NULL DEFAULT 0,
    ADD COLUMN total_borrow_usd NUMERIC NOT NULL DEFAULT 0,
    ADD COLUMN total_collateral_usd NUMERIC NOT NULL DEFAULT 0,
    ADD COLUMN borrow_apy NUMERIC NOT NULL DEFAULT 0,
    ADD COLUMN cumulative_supply_volume_usd NUMERIC NOT NULL DEFAULT 0,
    ADD COLUMN cumulative_borrow_volume_usd NUMERIC NOT NULL DEFAULT 0,
    DROP COLUMN created_at;
