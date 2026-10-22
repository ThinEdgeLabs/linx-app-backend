-- Your SQL goes here
CREATE TYPE output_ref AS (
    tx_hash TEXT,
    index BIGINT
);

CREATE TYPE output AS (
    address TEXT,
    amount NUMERIC,
    token_id TEXT
);

-- Create the transactions table
CREATE TABLE transactions (
    tx_hash TEXT PRIMARY KEY,
    unsigned JSONB NOT NULL,
    script_execution_ok BOOLEAN NOT NULL,
    contract_inputs JSONB NOT NULL,  -- Array of OutputRef
    generated_outputs JSONB NOT NULL,  -- Array of Output
    input_signatures TEXT[] NOT NULL,
    script_signatures TEXT[] NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- Function to update timestamp
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$$ language 'plpgsql';

-- Trigger to automatically update the updated_at column
CREATE TRIGGER update_transactions_updated_at
    BEFORE UPDATE ON transactions
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();