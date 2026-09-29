//! Integration test: verifies that secret plaintext never lands in the
//! on-disk database file, independent of the unit tests that check the
//! encrypt/decrypt round trip in isolation.

use remotix_lib::testing::{create_password_profile, open_temp_vault};

#[test]
fn plaintext_password_is_absent_from_the_database_file() {
    let fixture = open_temp_vault("correct-password-1");
    let secret = "super-unique-plaintext-marker-8f13ac";
    create_password_profile(&fixture, "Prod", "example.com", "root", secret);

    // Force a checkpoint so WAL contents are flushed into the main file
    // before we scan the bytes on disk.
    fixture.checkpoint();

    let db_bytes = std::fs::read(&fixture.db_path).unwrap();
    assert!(
        !contains_subslice(&db_bytes, secret.as_bytes()),
        "plaintext secret must never appear in the database file"
    );

    let wal_path = fixture.db_path.with_extension("db-wal");
    if let Ok(wal_bytes) = std::fs::read(&wal_path) {
        assert!(
            !contains_subslice(&wal_bytes, secret.as_bytes()),
            "plaintext secret must never appear in the WAL file"
        );
    }
}

#[test]
fn master_password_itself_is_never_persisted() {
    let master_password = "totally-unique-master-marker-d92e1";
    let fixture = open_temp_vault(master_password);
    create_password_profile(&fixture, "Prod", "example.com", "root", "irrelevant-secret");
    fixture.checkpoint();

    let db_bytes = std::fs::read(&fixture.db_path).unwrap();
    assert!(!contains_subslice(&db_bytes, master_password.as_bytes()));
}

fn contains_subslice(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}
