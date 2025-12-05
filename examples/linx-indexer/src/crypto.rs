/// Alephium cryptographic utilities for signing and verifying messages
///
/// This module provides functions for:
/// - Generating Alephium addresses from private keys
/// - Signing messages with private keys
/// - Verifying signatures against Alephium addresses
use crate::AddressType;
use anyhow;

/// Generate an Alephium address from a private key
///
/// # Arguments
/// * `private_key_bytes` - The 32-byte private key
/// * `address_type` - The address type (default: P2PKH = 0x00)
///
/// # Returns
/// * Base58-encoded Alephium address
pub fn address_from_private_key(
    private_key_bytes: &[u8],
    address_type: AddressType,
) -> anyhow::Result<String> {
    use blake2::{Blake2b, Digest};
    use secp256k1::{PublicKey, Secp256k1, SecretKey};

    if private_key_bytes.len() != 32 {
        return Err(anyhow::anyhow!("Private key must be 32 bytes"));
    }

    // Create secret key
    let secret_key = SecretKey::from_slice(private_key_bytes)
        .map_err(|e| anyhow::anyhow!("Invalid private key: {}", e))?;

    // Get public key
    let secp = Secp256k1::new();
    let public_key = PublicKey::from_secret_key(&secp, &secret_key);
    let pubkey_bytes = public_key.serialize();

    // Hash the public key with Blake2b-256
    let mut hasher = Blake2b::<blake2::digest::consts::U32>::new();
    hasher.update(pubkey_bytes);
    let hash = hasher.finalize();

    // Create address: [type_byte][blake2b_hash]
    // No checksum - Alephium uses just type byte + hash
    let mut address_bytes = Vec::with_capacity(1 + hash.len());
    address_bytes.push(address_type as u8);
    address_bytes.extend_from_slice(&hash);

    Ok(bs58::encode(address_bytes).into_string())
}

/// Sign a message with a private key (Alephium format)
///
/// # Arguments
/// * `message` - The message to sign
/// * `private_key_bytes` - The 32-byte private key
///
/// # Returns
/// * Hex-encoded signature (64-byte compact format)
pub fn sign_message(message: &str, private_key_bytes: &[u8]) -> anyhow::Result<String> {
    use secp256k1::{Message, Secp256k1, SecretKey};
    use sha2::{Digest, Sha256};

    if private_key_bytes.len() != 32 {
        return Err(anyhow::anyhow!("Private key must be 32 bytes"));
    }

    // Hash the message using SHA256
    let mut hasher = Sha256::new();
    hasher.update(message.as_bytes());
    let message_hash = hasher.finalize();

    // Convert to secp256k1 Message
    let secp_message = Message::from_digest_slice(&message_hash)
        .map_err(|e| anyhow::anyhow!("Invalid message hash: {}", e))?;

    // Create secret key
    let secret_key = SecretKey::from_slice(private_key_bytes)
        .map_err(|e| anyhow::anyhow!("Invalid private key: {}", e))?;

    // Sign the message
    let secp = Secp256k1::signing_only();
    let signature = secp.sign_ecdsa(&secp_message, &secret_key);

    // Return as hex (compact 64-byte format)
    Ok(hex::encode(signature.serialize_compact()))
}

/// Verify that a signature was created by the given public key
///
/// # Arguments
/// * `public_key_hex` - The public key in hex format (66 chars for compressed, 130 for uncompressed)
/// * `message` - The message that was signed
/// * `signature_hex` - The signature in hex format (compact 64-byte format)
///
/// # Returns
/// * `Ok(true)` if the signature is valid
/// * `Ok(false)` if the signature is invalid
/// * `Err` if there's a formatting error with the inputs
pub fn verify_signature(
    public_key_hex: &str,
    message: &str,
    signature_hex: &str,
) -> anyhow::Result<bool> {
    use secp256k1::ecdsa::Signature;
    use secp256k1::{Message, PublicKey, Secp256k1};
    use sha2::{Digest, Sha256};

    // Hash the message using SHA256
    let mut hasher = Sha256::new();
    hasher.update(message.as_bytes());
    let message_hash = hasher.finalize();

    // Convert to secp256k1 Message
    let secp_message = Message::from_digest_slice(&message_hash)
        .map_err(|e| anyhow::anyhow!("Invalid message hash: {}", e))?;

    // Decode the signature from hex
    let signature_bytes =
        hex::decode(signature_hex).map_err(|e| anyhow::anyhow!("Invalid signature hex: {}", e))?;

    let signature = Signature::from_compact(&signature_bytes)
        .map_err(|e| anyhow::anyhow!("Invalid signature format: {}", e))?;

    // Decode public key from hex
    let pubkey_bytes = hex::decode(public_key_hex)
        .map_err(|e| anyhow::anyhow!("Invalid public key hex: {}", e))?;

    let public_key = PublicKey::from_slice(&pubkey_bytes)
        .map_err(|e| anyhow::anyhow!("Invalid public key: {}", e))?;

    // Verify the signature
    let secp = Secp256k1::verification_only();
    Ok(secp.verify_ecdsa(&secp_message, &signature, &public_key).is_ok())
}

/// Verify that a public key corresponds to an Alephium address
///
/// # Arguments
/// * `public_key_hex` - The public key in hex format
/// * `address` - The Alephium address (base58 encoded)
///
/// # Returns
/// * `Ok(true)` if the public key matches the address
/// * `Ok(false)` if they don't match
/// * `Err` if there's a formatting error
pub fn verify_public_key_for_address(public_key_hex: &str, address: &str) -> anyhow::Result<bool> {
    use blake2::{Blake2b, Digest};

    // Decode public key from hex
    let pubkey_bytes = hex::decode(public_key_hex)
        .map_err(|e| anyhow::anyhow!("Invalid public key hex: {}", e))?;

    // Hash the public key with Blake2b-256
    let mut hasher = Blake2b::<blake2::digest::consts::U32>::new();
    hasher.update(&pubkey_bytes);
    let computed_hash = hasher.finalize();

    // Decode Alephium address
    let decoded_address = bs58::decode(address)
        .into_vec()
        .map_err(|e| anyhow::anyhow!("Invalid address format: {}", e))?;

    if decoded_address.len() != 33 {
        // 1 byte type + 32 bytes hash
        return Err(anyhow::anyhow!("Invalid address length"));
    }

    let expected_hash = &decoded_address[1..]; // Skip address type byte

    // Compare hashes
    Ok(computed_hash.as_slice() == expected_hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sign_and_verify_valid_signature() {
        use secp256k1::{PublicKey, Secp256k1, SecretKey};

        // Create a test private key (32 bytes)
        let private_key = [
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
            0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c,
            0x1d, 0x1e, 0x1f, 0x20,
        ];

        // Get public key
        let secret_key = SecretKey::from_slice(&private_key).unwrap();
        let secp = Secp256k1::new();
        let public_key = PublicKey::from_secret_key(&secp, &secret_key);
        let public_key_hex = hex::encode(public_key.serialize());

        // Sign a message
        let message = "Test message for signing";
        let signature = sign_message(message, &private_key).expect("Failed to sign message");

        // Verify the signature
        let is_valid = verify_signature(&public_key_hex, message, &signature)
            .expect("Failed to verify signature");

        assert!(is_valid, "Signature should be valid");
    }

    #[test]
    fn test_verify_invalid_signature() {
        use secp256k1::{PublicKey, Secp256k1, SecretKey};

        // Create a test private key
        let private_key = [
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
            0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c,
            0x1d, 0x1e, 0x1f, 0x20,
        ];

        // Get public key
        let secret_key = SecretKey::from_slice(&private_key).unwrap();
        let secp = Secp256k1::new();
        let public_key = PublicKey::from_secret_key(&secp, &secret_key);
        let public_key_hex = hex::encode(public_key.serialize());

        // Sign a message
        let message = "Original message";
        let signature = sign_message(message, &private_key).expect("Failed to sign message");

        // Try to verify with a different message
        let different_message = "Different message";
        let is_valid = verify_signature(&public_key_hex, different_message, &signature)
            .expect("Failed to verify signature");

        assert!(!is_valid, "Signature should be invalid for different message");
    }

    #[test]
    fn test_verify_wrong_public_key() {
        use secp256k1::{PublicKey, Secp256k1, SecretKey};

        // Create two different private keys
        let private_key1 = [
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
            0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c,
            0x1d, 0x1e, 0x1f, 0x20,
        ];

        let private_key2 = [
            0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e,
            0x2f, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x3b, 0x3c,
            0x3d, 0x3e, 0x3f, 0x40,
        ];

        let secp = Secp256k1::new();
        let secret_key2 = SecretKey::from_slice(&private_key2).unwrap();
        let public_key2 = PublicKey::from_secret_key(&secp, &secret_key2);
        let public_key2_hex = hex::encode(public_key2.serialize());

        // Sign with key1
        let message = "Test message";
        let signature = sign_message(message, &private_key1).expect("Failed to sign message");

        // Try to verify with public_key2
        let is_valid = verify_signature(&public_key2_hex, message, &signature)
            .expect("Failed to verify signature");

        assert!(!is_valid, "Signature should be invalid for wrong public key");
    }

    #[test]
    fn test_verify_malformed_signature_hex() {
        use secp256k1::{PublicKey, Secp256k1, SecretKey};

        let private_key = [
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
            0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c,
            0x1d, 0x1e, 0x1f, 0x20,
        ];

        let secret_key = SecretKey::from_slice(&private_key).unwrap();
        let secp = Secp256k1::new();
        let public_key = PublicKey::from_secret_key(&secp, &secret_key);
        let public_key_hex = hex::encode(public_key.serialize());

        let message = "Test message";

        // Test with invalid hex string
        let result = verify_signature(&public_key_hex, message, "not-valid-hex");
        assert!(result.is_err(), "Should error on invalid hex");

        // Test with wrong length signature
        let result = verify_signature(&public_key_hex, message, "aabbccdd");
        assert!(result.is_err(), "Should error on wrong length signature");
    }

    #[test]
    fn test_verify_malformed_public_key() {
        let message = "Test message";
        let fake_signature = "0".repeat(128); // Valid hex but fake signature

        // Test with invalid hex
        let result = verify_signature("not-valid-hex", message, &fake_signature);
        assert!(result.is_err(), "Should error on invalid public key hex");

        // Test with wrong length public key
        let result = verify_signature("aabbccdd", message, &fake_signature);
        assert!(result.is_err(), "Should error on wrong length public key");
    }

    #[test]
    fn test_sign_with_invalid_private_key() {
        let message = "Test message";

        // Test with wrong length private key
        let short_key = [0x01, 0x02, 0x03];
        let result = sign_message(message, &short_key);
        assert!(result.is_err(), "Should error on wrong length private key");

        // Test with all zeros (invalid private key)
        let zero_key = [0u8; 32];
        let result = sign_message(message, &zero_key);
        assert!(result.is_err(), "Should error on invalid private key");
    }

    #[test]
    fn test_address_from_private_key() {
        let private_key =
            hex::decode("7babd8a9b3af814757fde3d801afcf9a94d1d9e35863c31db75e05202136e1b8")
                .unwrap();

        let address = address_from_private_key(&private_key, AddressType::P2PKH)
            .expect("Failed to generate address");
        let expected_address = "1EJCtZP3HZP5rDX5v2o32woqLTxp6GS4GoLQGpzVPQm6E";
        assert_eq!(address, expected_address, "Generated address does not match expected");
    }

    #[test]
    fn test_verify_public_key_for_address() {
        use secp256k1::{PublicKey, Secp256k1, SecretKey};

        let private_key = [
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
            0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c,
            0x1d, 0x1e, 0x1f, 0x20,
        ];

        // Generate address and public key from private key
        let address = address_from_private_key(&private_key, AddressType::P2PKH)
            .expect("Failed to generate address");

        let secret_key = SecretKey::from_slice(&private_key).unwrap();
        let secp = Secp256k1::new();
        let public_key = PublicKey::from_secret_key(&secp, &secret_key);
        let public_key_hex = hex::encode(public_key.serialize());

        // Verify they match
        let matches = verify_public_key_for_address(&public_key_hex, &address)
            .expect("Failed to verify public key");

        assert!(matches, "Public key should match address");

        // Test with wrong public key
        let private_key2 = [
            0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e,
            0x2f, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x3b, 0x3c,
            0x3d, 0x3e, 0x3f, 0x40,
        ];

        let secret_key2 = SecretKey::from_slice(&private_key2).unwrap();
        let public_key2 = PublicKey::from_secret_key(&secp, &secret_key2);
        let public_key2_hex = hex::encode(public_key2.serialize());

        let matches = verify_public_key_for_address(&public_key2_hex, &address)
            .expect("Failed to verify public key");

        assert!(!matches, "Wrong public key should not match address");
    }
}
