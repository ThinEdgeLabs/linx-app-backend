CREATE TABLE swaps (
    id BIGSERIAL PRIMARY KEY,
    account_transaction_id BIGINT NOT NULL REFERENCES account_transactions(id) ON DELETE CASCADE,
    token_in TEXT NOT NULL,
    token_out TEXT NOT NULL,
    amount_in NUMERIC NOT NULL,
    amount_out NUMERIC NOT NULL,
    pool_address TEXT NOT NULL,
    tx_id TEXT NOT NULL
);

CREATE UNIQUE INDEX unique_swaps ON swaps (tx_id, token_in, token_out, pool_address);