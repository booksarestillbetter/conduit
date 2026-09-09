// tests/db_encryption_tests.rs
use conduit::db::Database;
use tempfile::NamedTempFile;

#[test]
fn test_sqlcipher_database_encryption_and_rekey() {
    let temp_db = NamedTempFile::new().expect("Failed to create temp db file");
    let db_path = temp_db.path().to_path_buf();
    let initial_key = "ConduitMasterKey#2026";

    // 1. Initialize encrypted database and write records
    {
        let db = Database::init(&db_path, Some(initial_key))
            .expect("Failed to init encrypted database");
        
        let user = db.create_user("admin_encrypted", "hashed_pass_123", true)
            .expect("Failed to create user");
        assert_eq!(user.username, "admin_encrypted");

        let tok = db.create_api_token("test_token", "hash_tok_abc", &["torrents:read".to_string()], None)
            .expect("Failed to create API token");
        assert_eq!(tok.name, "test_token");

        db.log_event("auth", "info", "User created in encrypted DB", None)
            .expect("Failed to log event");
    }

    // 2. Re-open with the CORRECT key -> should succeed and read back records
    {
        let db = Database::init(&db_path, Some(initial_key))
            .expect("Failed to open encrypted database with valid key");

        let user = db.get_user_by_username("admin_encrypted")
            .expect("Failed query")
            .expect("User not found");
        assert_eq!(user.username, "admin_encrypted");

        let has_users = db.has_users().expect("Failed has_users query");
        assert!(has_users);
    }

    // 3. Attempt to open with NO key or WRONG key -> must fail to read encrypted database
    {
        let wrong_db_attempt = Database::init(&db_path, Some("WrongPasswordXYZ"));
        assert!(
            wrong_db_attempt.is_err(),
            "Opening encrypted database with wrong key should fail"
        );

        let unkeyed_attempt = Database::init(&db_path, None);
        assert!(
            unkeyed_attempt.is_err(),
            "Opening encrypted database without key should fail"
        );
    }

    // 4. Test Key Rotation (PRAGMA rekey)
    let new_key = "NewRotatedSuperKey#2027";
    {
        let db = Database::init(&db_path, Some(initial_key))
            .expect("Failed to open with initial key");
        db.rekey(new_key).expect("Failed to rekey database");
    }

    // 5. Verify that old key now fails and new key succeeds
    {
        let old_key_attempt = Database::init(&db_path, Some(initial_key));
        assert!(
            old_key_attempt.is_err(),
            "Old key should fail after rekey"
        );

        let new_key_db = Database::init(&db_path, Some(new_key))
            .expect("Opening with new key should succeed");
        let user = new_key_db.get_user_by_username("admin_encrypted")
            .expect("Query failed")
            .expect("User should exist");
        assert_eq!(user.username, "admin_encrypted");
    }
}
