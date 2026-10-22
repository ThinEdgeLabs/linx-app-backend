-- Your SQL goes here

CREATE TABLE loan_details (
    id SERIAL PRIMARY KEY,
    loan_subcontract_id VARCHAR NOT NULL,
    lending_token_id VARCHAR NOT NULL,
    collateral_token_id VARCHAR NOT NULL,
    lending_amount NUMERIC NOT NULL,
    collateral_amount NUMERIC NOT NULL,
    interest_rate NUMERIC NOT NULL,
    duration NUMERIC NOT NULL,
    lender VARCHAR NOT NULL
);

CREATE INDEX idx_loan_details_lending_token_id ON loan_details(lending_token_id);

CREATE INDEX idx_loan_details_collateral_token_id ON loan_details(collateral_token_id);

CREATE INDEX idx_loan_details_lender ON loan_details(lender);