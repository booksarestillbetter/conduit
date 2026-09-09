use conduit::auth::{create_jwt, generate_totp_secret, get_otpauth_url, hash_password, verify_jwt, verify_password, verify_totp_code};

#[test]
fn test_password_hashing() {
    let password = "MyAdminPassword123";
    let hash = hash_password(password).expect("Hashing failed");
    assert!(verify_password(password, &hash));
    assert!(!verify_password("WrongPassword", &hash));
}

#[test]
fn test_jwt_lifecycle() {
    let secret = "test_jwt_secret_32_characters_long_key";
    let token = create_jwt("user-123", "admin", true, 1, secret, 7).expect("JWT creation failed");
    
    let claims = verify_jwt(&token, secret).expect("JWT verification failed");
    assert_eq!(claims.sub, "user-123");
    assert_eq!(claims.username, "admin");
    assert!(claims.is_admin);
    assert_eq!(claims.token_version, 1);

    let invalid = verify_jwt(&token, "wrong_secret");
    assert!(invalid.is_err());
}

#[test]
fn test_totp_two_factor_auth() {
    let secret = generate_totp_secret();
    assert_eq!(secret.len(), 32);

    let otpauth = get_otpauth_url(&secret, "testuser", "Conduit");
    assert!(otpauth.contains("secret="));
    assert!(otpauth.contains("issuer=Conduit"));

    // Verify invalid code rejected
    assert!(!verify_totp_code(&secret, "000000"));
}
