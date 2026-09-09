// tests/vault_tests.rs
use conduit::config::vault::{decrypt_with_passphrase, encrypt_with_passphrase};

#[test]
fn test_encryption_and_decryption() {
    let secret_data = b"Transmission Secret API Key and Credentials 12345!";
    let passphrase = "SuperSecureMasterPassphrase#2026";

    let envelope = encrypt_with_passphrase(secret_data, passphrase).expect("Encryption failed");
    assert_eq!(envelope.version, 1);
    assert!(!envelope.ciphertext_b64.is_empty());
    assert!(!envelope.salt_b64.is_empty());
    assert!(!envelope.nonce_b64.is_empty());

    let decrypted = decrypt_with_passphrase(&envelope, passphrase).expect("Decryption failed");
    assert_eq!(decrypted, secret_data);

    // Test wrong passphrase
    let wrong_res = decrypt_with_passphrase(&envelope, "WrongPassword");
    assert!(wrong_res.is_err());
}
