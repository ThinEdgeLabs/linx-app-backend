-- Your SQL goes here
CREATE TABLE loan_actions (
    id SERIAL PRIMARY KEY,
    loan_subcontract_id VARCHAR NOT NULL,
    loan_id NUMERIC,
    by VARCHAR NOT NULL,
    timestamp TIMESTAMP NOT NULL,
    action_type SMALLINT NOT NULL
);

-- Create an index on loan_subcontract_id for fast lookups
CREATE INDEX idx_loan_actions_loan_subcontract_id ON loan_actions(loan_subcontract_id);

-- Create an index on timestamp for time-based queries
CREATE INDEX idx_loan_actions_timestamp ON loan_actions(timestamp);

-- Create an index on action_type for action-based queries
CREATE INDEX idx_loan_actions_action_type ON loan_actions(action_type);