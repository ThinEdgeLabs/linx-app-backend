use rand::RngCore;

pub mod constants;
pub mod models;
pub mod processors;
pub mod repository;
pub mod routers;
pub mod schema;
pub mod services;

pub enum AddressType {
    P2PKH = 0x00,
    P2MPKH = 0x01,
    P2SH = 0x02,
    P2C = 0x03,
}

pub fn hex_to_bin_unsafe(hex: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut i = 0;

    while i < hex.len() {
        let hex_pair = &hex[i..i + 2];
        let byte = u8::from_str_radix(hex_pair, 16).unwrap();
        bytes.push(byte);
        i += 2;
    }

    bytes
}

pub fn address_from_contract_id(contract_id: &str) -> String {
    let hash = hex_to_bin_unsafe(contract_id);
    let mut bytes = Vec::with_capacity(1 + hash.len());
    bytes.push(AddressType::P2C as u8);
    bytes.extend_from_slice(&hash);

    bs58::encode(bytes).into_string()
}

pub fn random_tx_id() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    bin_to_hex(&bytes)
}

pub fn bin_to_hex(bin: &[u8]) -> String {
    bin.iter().map(|byte| format!("{:02x}", byte)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_address_from_contract_id() {
        let contract_id = "a34bc3ef594c26a566658cc53f87f437cf75b54467692194e2bc9903a7cd9500";
        let address = address_from_contract_id(contract_id);
        assert_eq!(address, "25gPaHaaTYKewVuek6npKSuBgZtQ3ZCpPXouLKZpiMSib");
    }
}
