CREATE TABLE referral_codes (
    id SERIAL PRIMARY KEY,
    code TEXT UNIQUE NOT NULL,
    owner_address TEXT NOT NULL,
    created_at TIMESTAMP DEFAULT NOW() NOT NULL
);

CREATE INDEX idx_referral_codes_owner ON referral_codes(owner_address);
CREATE INDEX idx_referral_codes_code ON referral_codes(code);

COMMENT ON TABLE referral_codes IS 'User referral codes for tracking referrals';
COMMENT ON COLUMN referral_codes.code IS 'Unique referral code';
COMMENT ON COLUMN referral_codes.owner_address IS 'Address of the user who owns this referral code';
