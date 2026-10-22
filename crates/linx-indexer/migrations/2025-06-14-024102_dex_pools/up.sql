CREATE TABLE pools (
    id BIGSERIAL PRIMARY KEY,
    address TEXT UNIQUE NOT NULL,
    token_a TEXT NOT NULL,
    token_b TEXT NOT NULL,
    factory_address TEXT NOT NULL
);