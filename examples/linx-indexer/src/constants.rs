use std::collections::HashMap;

use bento_types::network::{self, Network};

pub const VIRTUAL_ASSETS: u64 = 1;
pub const VIRTUAL_SHARES: u64 = 1000000;
pub const ALPH_TOKEN_ID: &str = "0000000000000000000000000000000000000000000000000000000000000000";

const TEST_BTC_TOKEN_ID: &str = "0712ee40be418ed0105b1b9c1f255a5e3fa0ef40004f400f216df05eb014c600";
const TEST_USDT_TOKEN_ID: &str = "79804da1cd63c4575675b6391d956f4745591c65a30aa058ae6bd0a07ce64b00";

pub fn dia_token_pairs(network: &Network) -> HashMap<&'static str, &'static str> {
    match network {
        network::Network::Mainnet => mainnet_dia_token_pairs(),
        network::Network::Testnet => testnet_dia_token_pairs(),
        _ => HashMap::new(),
    }
}

fn mainnet_dia_token_pairs() -> HashMap<&'static str, &'static str> {
    let mut map = HashMap::new();
    map.insert(ALPH_TOKEN_ID, "ALPHUSD");
    map.insert("383bc735a4de6722af80546ec9eeb3cff508f2f68e97da19489ce69f3e703200", "BTC/USD"); // Wrapped BTC (AlphBridge)
    map.insert("556d9582463fe44fbd108aedc9f409f69086dc78d994b88ea6c9e65f8bf98e00", "USDT/USD"); // Tether USD (Ethereum via AlphBridge)
    map.insert("7ff5e72636f640eb2c28056df3b6879e4c86933505abebf566518ad396335700", "USDT/USD"); // Tether USD (BSC via AlphBridge)
    map.insert("722954d9067c5a5ad532746a024f2a9d7a18ed9b90e27d0a3a504962160b5600", "USDC/USD"); // USD Coin (Ethereum via AlphBridge)
    map.insert("75e1e9f91468616a371fe416430819bf5386a3e6a258864c574271a404ec8900", "USDC/USD"); // USD Coin (BSC via AlphBridge)
    map.insert("19246e8c2899bc258a1156e08466e3cdd3323da756d8a543c7fc911847b96f00", "ETH/USD"); // Wrapped Ether (AlphBridge)
    map
}

fn testnet_dia_token_pairs() -> HashMap<&'static str, &'static str> {
    let mut map = HashMap::new();
    map.insert(ALPH_TOKEN_ID, "ALPH/USD");
    map.insert(TEST_BTC_TOKEN_ID, "BTC/USD");
    map.insert(TEST_USDT_TOKEN_ID, "USDT/USD");
    map
}
