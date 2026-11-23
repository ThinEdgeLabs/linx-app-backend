-- Lending markets hidden from the API
CREATE TABLE blacklisted_markets (
    market_id TEXT PRIMARY KEY,
    reason TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

INSERT INTO blacklisted_markets (market_id, reason) VALUES
    ('9f539d49c55da710a919981cb1ac5547c6ef2acf16e685fb401570802d2a99f0',
     'Misconfigured market created to exploit users');

DELETE FROM lending_position_snapshots
WHERE market_id IN (SELECT market_id FROM blacklisted_markets);
