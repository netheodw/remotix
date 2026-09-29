//! Shared fixtures for the crate's own unit tests and for the integration
//! tests under `tests/`, which link this crate as an ordinary dependency
//! and so cannot see anything gated behind `#[cfg(test)]`.

use std::sync::Arc;

use crate::db;
use crate::domain::{AuthSecretInput, NewSshProfileInput, SshProfile};
use crate::repositories::ProfileRepository;
use crate::vault::VaultManager;

pub struct VaultFixture {
    _dir: tempfile::TempDir,
    pub pool: db::Pool,
    pub vault: Arc<VaultManager>,
    pub db_path: std::path::PathBuf,
}

impl VaultFixture {
    /// Flushes WAL contents into the main database file so a byte-level
    /// scan of `db_path` reflects everything written so far.
    pub fn checkpoint(&self) {
        let conn = self.pool.get().expect("pool");
        let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
    }
}

pub fn open_temp_vault(master_password: &str) -> VaultFixture {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("test.db");
    let pool = db::init_pool(&db_path).expect("init_pool");
    let vault = VaultManager::new();
    vault.setup(&pool, master_password).expect("vault setup");
    VaultFixture { _dir: dir, pool, vault: Arc::new(vault), db_path }
}

pub fn create_password_profile(
    fixture: &VaultFixture,
    name: &str,
    hostname: &str,
    username: &str,
    password: &str,
) -> SshProfile {
    let conn = fixture.pool.get().expect("pool");
    ProfileRepository::create(
        &conn,
        &fixture.vault,
        NewSshProfileInput {
            terminal_group_id: None,
            name: name.to_string(),
            hostname: hostname.to_string(),
            port: 22,
            username: username.to_string(),
            terminal_type: None,
            secret: AuthSecretInput::Password { password: password.to_string() },
        },
    )
    .expect("create profile")
}
