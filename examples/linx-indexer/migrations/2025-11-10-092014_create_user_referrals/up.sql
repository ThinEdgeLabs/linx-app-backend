CREATE TABLE user_referrals (
    id SERIAL PRIMARY KEY,
    user_address TEXT UNIQUE NOT NULL,
    referral_code TEXT NOT NULL,
    referred_by_address TEXT NOT NULL,
    created_at TIMESTAMP DEFAULT NOW() NOT NULL,
    FOREIGN KEY (referral_code) REFERENCES referral_codes(code)
);

CREATE INDEX idx_user_referrals_user ON user_referrals(user_address);
CREATE INDEX idx_user_referrals_referred_by ON user_referrals(referred_by_address);
CREATE INDEX idx_user_referrals_code ON user_referrals(referral_code);

COMMENT ON TABLE user_referrals IS 'Tracks which users used which referral codes';
COMMENT ON COLUMN user_referrals.user_address IS 'Address of the user who used a referral code';
COMMENT ON COLUMN user_referrals.referral_code IS 'The referral code that was used';
COMMENT ON COLUMN user_referrals.referred_by_address IS 'Address of the user who owns the referral code';
