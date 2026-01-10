use bento_core::ProcessorFactory;
use bigdecimal::BigDecimal;
use rand::RngCore;
use std::collections::HashMap;

pub mod config;
pub mod constants;
pub mod crypto;
pub mod models;
pub mod processors;
pub mod repository;
pub mod routers;
pub mod schema;
pub mod services;

/// Register all custom processor factories
pub fn get_processor_factories() -> HashMap<String, ProcessorFactory> {
    let mut processor_factories = HashMap::new();
    processor_factories
        .insert("transfers".to_string(), processors::transfer_processor::processor_factory());
    processor_factories.insert(
        "contract_calls".to_string(),
        processors::contract_call_processor::processor_factory(),
    );
    processor_factories.insert("dex".to_string(), processors::dex_processor::processor_factory());
    processor_factories
        .insert("lending".to_string(), processors::lending_processor::processor_factory());
    processor_factories
}

pub enum AddressType {
    P2PKH = 0x00,
    P2MPKH = 0x01,
    P2SH = 0x02,
    P2C = 0x03,
    P2PK = 0x04,   // Groupless Pay-to-Public-Key (Danube upgrade)
    P2HMPK = 0x05, // Groupless Pay-to-Hashed-Multi-Public-Key (Danube upgrade)
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

pub fn extract_bigdecimal_from_object(
    json: &serde_json::Value,
    key: &str,
) -> anyhow::Result<BigDecimal> {
    json.as_object()
        .and_then(|obj| obj.get(key))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Invalid object structure"))?
        .parse::<BigDecimal>()
        .map_err(|e| anyhow::anyhow!("Failed to parse BigDecimal: {}", e))
}

pub fn string_to_hex(s: &str) -> String {
    s.bytes().map(|byte| format!("{:02x}", byte)).collect()
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
