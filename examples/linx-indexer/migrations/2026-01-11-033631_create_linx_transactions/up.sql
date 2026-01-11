CREATE TABLE linx_transactions (
    id BIGSERIAL PRIMARY KEY,
    tx_id TEXT NOT NULL UNIQUE,
    user_address TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_linx_transactions_tx_id ON linx_transactions (tx_id);
CREATE INDEX idx_linx_transactions_user_address ON linx_transactions (user_address, created_at DESC);
