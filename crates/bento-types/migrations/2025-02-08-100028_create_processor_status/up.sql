CREATE TABLE IF NOT EXISTS processor_status (
    processor VARCHAR(50) UNIQUE PRIMARY KEY NOT NULL,
    last_timestamp BIGINT NOT NULL
);