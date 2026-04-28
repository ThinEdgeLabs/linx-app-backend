ALTER TABLE market_state_snapshots
    ADD COLUMN created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    DROP COLUMN cumulative_borrow_volume_usd,
    DROP COLUMN cumulative_supply_volume_usd,
    DROP COLUMN fee,
    DROP COLUMN borrow_apy,
    DROP COLUMN total_collateral_usd,
    DROP COLUMN total_borrow_usd,
    DROP COLUMN total_supply_usd,
    DROP COLUMN total_collateral_assets;
