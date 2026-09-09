// src/auth/totp.rs
use rand::RngCore;
use sha1::{Digest, Sha1};

const BASE32_ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// Generates a random 20-byte Base32 secret string (32 characters).
pub fn generate_totp_secret() -> String {
    let mut bytes = [0u8; 20];
    rand::rng().fill_bytes(&mut bytes);
    encode_base32(&bytes)
}

/// Generates standard otpauth:// URL for scanning in Authenticator apps.
pub fn get_otpauth_url(secret: &str, username: &str, issuer: &str) -> String {
    format!(
        "otpauth://totp/{}:{}?secret={}&issuer={}&algorithm=SHA1&digits=6&period=30",
        url_encode(issuer),
        url_encode(username),
        secret,
        url_encode(issuer)
    )
}

/// Validates a 6-digit TOTP code against a Base32 secret allowing +/- 1 time-step (30s) drift.
/// Does not check for replay — prefer `verify_totp_code_at` wherever a per-user last-used step
/// is available, and pair it with `Database::consume_totp_step` to enforce single-use codes.
#[allow(dead_code)]
pub fn verify_totp_code(secret: &str, code: &str) -> bool {
    verify_totp_code_at(secret, code, 0).is_some()
}

/// Validates a 6-digit TOTP code, rejecting any time-step at or before `min_step` so a captured
/// code cannot be replayed within its ~90s validity window. Returns the matched step on success
/// so the caller can atomically record it (see `Database::consume_totp_step`).
pub fn verify_totp_code_at(secret: &str, code: &str, min_step: i64) -> Option<u64> {
    let code_clean = code.trim();
    if code_clean.len() != 6 {
        return None;
    }

    let secret_bytes = decode_base32(secret)?;

    let now_secs = chrono::Utc::now().timestamp() as u64;
    let step = now_secs / 30;

    // Check steps: current, previous (-30s), next (+30s)
    for s in [step, step.saturating_sub(1), step + 1] {
        if (s as i64) <= min_step {
            continue; // already-consumed step: reject as a replay
        }
        if let Some(expected_code) = compute_totp(&secret_bytes, s) {
            if subtle::ConstantTimeEq::ct_eq(code_clean.as_bytes(), expected_code.as_bytes()).into() {
                return Some(s);
            }
        }
    }

    None
}

/// Computes a 6-digit TOTP code for a specific 30s step counter.
pub fn compute_totp(secret_bytes: &[u8], step: u64) -> Option<String> {
    let mut step_bytes = [0u8; 8];
    step_bytes.copy_from_slice(&step.to_be_bytes());

    let hmac = hmac_sha1(secret_bytes, &step_bytes);
    if hmac.is_empty() {
        return None;
    }

    // Dynamic truncation (RFC 4226)
    let offset = (hmac[19] & 0x0f) as usize;
    let binary_code = ((hmac[offset] as u32 & 0x7f) << 24)
        | ((hmac[offset + 1] as u32 & 0xff) << 16)
        | ((hmac[offset + 2] as u32 & 0xff) << 8)
        | (hmac[offset + 3] as u32 & 0xff);

    let token = binary_code % 1_000_000;
    Some(format!("{:06}", token))
}

/// Standard HMAC-SHA1 implementation
fn hmac_sha1(key: &[u8], message: &[u8]) -> Vec<u8> {
    const BLOCK_SIZE: usize = 64;

    let mut k = [0u8; BLOCK_SIZE];
    if key.len() > BLOCK_SIZE {
        let mut hasher = Sha1::new();
        hasher.update(key);
        let hash = hasher.finalize();
        k[..hash.len()].copy_from_slice(&hash);
    } else {
        k[..key.len()].copy_from_slice(key);
    }

    let mut ipad = [0x36u8; BLOCK_SIZE];
    let mut opad = [0x5cu8; BLOCK_SIZE];

    for i in 0..BLOCK_SIZE {
        ipad[i] ^= k[i];
        opad[i] ^= k[i];
    }

    // Inner hash
    let mut inner = Sha1::new();
    inner.update(ipad);
    inner.update(message);
    let inner_hash = inner.finalize();

    // Outer hash
    let mut outer = Sha1::new();
    outer.update(opad);
    outer.update(inner_hash);
    outer.finalize().to_vec()
}

/// Standard Base32 encoder (RFC 4648 without padding)
pub fn encode_base32(data: &[u8]) -> String {
    let mut result = String::new();
    let mut buffer = 0u32;
    let mut bits_left = 0;

    for &b in data {
        buffer = (buffer << 8) | (b as u32);
        bits_left += 8;
        while bits_left >= 5 {
            bits_left -= 5;
            let idx = ((buffer >> bits_left) & 0x1F) as usize;
            result.push(BASE32_ALPHABET[idx] as char);
        }
    }

    if bits_left > 0 {
        let idx = ((buffer << (5 - bits_left)) & 0x1F) as usize;
        result.push(BASE32_ALPHABET[idx] as char);
    }

    result
}

/// Standard Base32 decoder (RFC 4648, case-insensitive, ignores spaces & dashes)
pub fn decode_base32(input: &str) -> Option<Vec<u8>> {
    let mut result = Vec::new();
    let mut buffer = 0u32;
    let mut bits_left = 0;

    for ch in input.chars() {
        if ch == ' ' || ch == '-' || ch == '=' {
            continue;
        }

        let val = match ch.to_ascii_uppercase() {
            'A'..='Z' => (ch.to_ascii_uppercase() as u8 - b'A') as u32,
            '2'..='7' => (ch as u8 - b'2' + 26) as u32,
            _ => return None,
        };

        buffer = (buffer << 5) | val;
        bits_left += 5;

        if bits_left >= 8 {
            bits_left -= 8;
            result.push(((buffer >> bits_left) & 0xFF) as u8);
        }
    }

    Some(result)
}

fn url_encode(input: &str) -> String {
    let mut encoded = String::new();
    for b in input.bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b'~' {
            encoded.push(b as char);
        } else {
            encoded.push_str(&format!("%{:02X}", b));
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base32_roundtrip() {
        let secret = generate_totp_secret();
        assert_eq!(secret.len(), 32);
        let decoded = decode_base32(&secret).expect("decode failed");
        assert_eq!(decoded.len(), 20);
        let reencoded = encode_base32(&decoded);
        assert_eq!(secret, reencoded);
    }

    #[test]
    fn test_totp_generation_and_verification() {
        let secret = generate_totp_secret();
        let decoded = decode_base32(&secret).unwrap();
        let now_secs = chrono::Utc::now().timestamp() as u64;
        let step = now_secs / 30;

        let code = compute_totp(&decoded, step).expect("compute failed");
        assert_eq!(code.len(), 6);

        // Verify valid code
        assert!(verify_totp_code(&secret, &code));

        // Verify invalid code fails
        assert!(!verify_totp_code(&secret, "000000"));
        assert!(!verify_totp_code(&secret, "invalid"));
    }

    #[test]
    fn test_otpauth_url_formatting() {
        let secret = "JBSWY3DPEHPK3PXP";
        let url = get_otpauth_url(secret, "admin@example.com", "Conduit");
        assert!(url.starts_with("otpauth://totp/Conduit:admin%40example.com"));
        assert!(url.contains("secret=JBSWY3DPEHPK3PXP"));
        assert!(url.contains("issuer=Conduit"));
    }
}
