-- Remove referral_code column from user_referrals table
-- The referred_by_address is sufficient - we don't need to store the code used
ALTER TABLE user_referrals DROP COLUMN referral_code;

COMMENT ON COLUMN user_referrals.referred_by_address IS 'Address of the user who referred this user (immutable relationship)';
