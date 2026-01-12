pub const VIRTUAL_ASSETS: u64 = 1;
pub const VIRTUAL_SHARES: u64 = 1000000;
pub const ALPH_TOKEN_ID: &str = "0000000000000000000000000000000000000000000000000000000000000000";
pub const DUST_AMOUNT: &str = "1000000000000000"; // 0.001 ALPH

// Stablecoin token IDs for price calculation
pub const USDT_TOKEN_IDS: &[&str] = &[
    "556d9582463fe44fbd108aedc9f409f69086dc78d994b88ea6c9e65f8bf98e00", // USDTeth Mainnet
    "7ff5e72636f640eb2c28056df3b6879e4c86933505abebf566518ad396335700", // USDTbsc Mainnet
];

pub const USDC_TOKEN_IDS: &[&str] = &[
    "722954d9067c5a5ad532746a024f2a9d7a18ed9b90e27d0a3a504962160b5600", // USDCeth Mainnet
    "75e1e9f91468616a371fe416430819bf5386a3e6a258864c574271a404ec8900", // USDCbsc Mainnet
];

/// Check if a token is a stablecoin (USDT or USDC)
pub fn is_stablecoin(token_id: &str) -> bool {
    USDT_TOKEN_IDS.contains(&token_id) || USDC_TOKEN_IDS.contains(&token_id)
}
