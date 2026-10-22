CREATE TABLE blocks (
    hash TEXT NOT NULL PRIMARY KEY,
    timestamp TIMESTAMP NOT NULL,
    chain_from BIGINT NOT NULL,
    chain_to BIGINT NOT NULL,
    height BIGINT NOT NULL,
    deps TEXT [],
    nonce TEXT NOT NULL,
    version TEXT NOT NULL,
    dep_state_hash TEXT NOT NULL,
    txs_hash TEXT NOT NULL,
    tx_number BIGINT NOT NULL,
    target TEXT NOT NULL,
    ghost_uncles JSONB NOT NULL DEFAULT '[]',
    CONSTRAINT height_positive CHECK (height >= 0)
);