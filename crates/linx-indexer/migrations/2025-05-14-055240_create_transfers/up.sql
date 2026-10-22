CREATE TABLE account_transactions (
    id BIGSERIAL PRIMARY KEY,
    address TEXT NOT NULL,
    tx_type TEXT NOT NULL,
    from_group SMALLINT NOT NULL,
    to_group SMALLINT NOT NULL,
    block_height BIGINT NOT NULL,
    tx_id TEXT NOT NULL,
    timestamp TIMESTAMP NOT NULL
);

CREATE INDEX idx_transactions_from_address ON account_transactions(address);

CREATE TABLE transfers (
    id BIGSERIAL PRIMARY KEY,
    account_transaction_id BIGINT NOT NULL REFERENCES account_transactions(id) ON DELETE CASCADE,
    token_id TEXT NOT NULL,
    from_address TEXT NOT NULL,
    to_address TEXT NOT NULL,
    amount NUMERIC NOT NULL,
    tx_id TEXT NOT NULL
);

CREATE UNIQUE INDEX unique_transfers ON transfers (tx_id, token_id, from_address, to_address, amount);

CREATE TABLE contract_calls (
    id BIGSERIAL PRIMARY KEY,
    account_transaction_id BIGINT NOT NULL REFERENCES account_transactions(id) ON DELETE CASCADE,
    contract_address TEXT NOT NULL,
    tx_id TEXT NOT NULL
);

CREATE UNIQUE INDEX unique_contract_calls ON contract_calls (tx_id, contract_address);