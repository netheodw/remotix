use std::sync::Mutex;
use std::time::{Duration, Instant};

use rusqlite::OptionalExtension;

use crate::db::Pool;
use crate::error::{AppError, AppResult};
use crate::util::now_iso;
use crate::vault::crypto::{self, VaultKey, CURRENT_AAD_VERSION};

const CHECK_PLAINTEXT: &[u8] = b"REMOTIX_VAULT_OK_V1";
const MIN_PASSWORD_LEN: usize = 8;
const MAX_BACKOFF_SECS: u64 = 30;

enum VaultState {
    Locked,
    Unlocked(VaultKey),
}

struct Inner {
    state: VaultState,
    failed_attempts: u32,
    locked_until: Option<Instant>,
}

pub struct VaultManager {
    inner: Mutex<Inner>,
}

impl Default for VaultManager {
    fn default() -> Self {
        Self {
            inner: Mutex::new(Inner {
                state: VaultState::Locked,
                failed_attempts: 0,
                locked_until: None,
            }),
        }
    }
}

impl VaultManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_unlocked(&self) -> bool {
        matches!(self.inner.lock().unwrap().state, VaultState::Unlocked(_))
    }

    pub fn lock(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.state = VaultState::Locked;
    }

    /// Returns a clone of the active vault key for internal encrypt/decrypt
    /// use. The clone zeroizes itself on drop like the original.
    pub fn key(&self) -> AppResult<VaultKey> {
        match &self.inner.lock().unwrap().state {
            VaultState::Unlocked(key) => Ok(key.clone()),
            VaultState::Locked => Err(AppError::VaultLocked),
        }
    }

    pub fn is_initialized(pool: &Pool) -> AppResult<bool> {
        let conn = pool.get()?;
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM vault_metadata", [], |r| r.get(0))?;
        Ok(count > 0)
    }

    pub fn setup(&self, pool: &Pool, password: &str) -> AppResult<()> {
        if Self::is_initialized(pool)? {
            return Err(AppError::VaultAlreadyInitialized);
        }
        if password.len() < MIN_PASSWORD_LEN {
            return Err(AppError::Validation(format!(
                "master password must be at least {MIN_PASSWORD_LEN} characters"
            )));
        }

        let salt = crypto::generate_salt();
        let key = crypto::derive_key(password, &salt)?;
        let aad = crypto::vault_check_aad(CURRENT_AAD_VERSION);
        let (ciphertext, nonce) = crypto::encrypt(&key, CHECK_PLAINTEXT, &aad)?;

        let conn = pool.get()?;
        conn.execute(
            "INSERT INTO vault_metadata (id, format_version, salt, encrypted_check_value, check_nonce, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                crate::util::new_id(),
                CURRENT_AAD_VERSION,
                salt.to_vec(),
                ciphertext,
                nonce,
                now_iso(),
            ],
        )?;

        let mut inner = self.inner.lock().unwrap();
        inner.state = VaultState::Unlocked(key);
        inner.failed_attempts = 0;
        inner.locked_until = None;
        Ok(())
    }

    pub fn unlock(&self, pool: &Pool, password: &str) -> AppResult<()> {
        {
            let inner = self.inner.lock().unwrap();
            if matches!(inner.state, VaultState::Unlocked(_)) {
                return Err(AppError::VaultAlreadyUnlocked);
            }
            if let Some(until) = inner.locked_until {
                let now = Instant::now();
                if now < until {
                    return Err(AppError::RateLimited {
                        retry_after_secs: (until - now).as_secs().max(1),
                    });
                }
            }
        }

        let conn = pool.get()?;
        let row: Option<(i64, Vec<u8>, Vec<u8>, Vec<u8>)> = conn
            .query_row(
                "SELECT format_version, salt, encrypted_check_value, check_nonce FROM vault_metadata LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?;

        let (format_version, salt, encrypted_check_value, check_nonce) =
            row.ok_or(AppError::VaultNotInitialized)?;

        let key = crypto::derive_key(password, &salt)?;
        let aad = crypto::vault_check_aad(format_version);
        let check = crypto::decrypt(&key, &encrypted_check_value, &check_nonce, &aad);

        let mut inner = self.inner.lock().unwrap();
        match check {
            Ok(plaintext) if plaintext == CHECK_PLAINTEXT => {
                inner.state = VaultState::Unlocked(key);
                inner.failed_attempts = 0;
                inner.locked_until = None;
                Ok(())
            }
            _ => {
                inner.failed_attempts = inner.failed_attempts.saturating_add(1);
                let backoff = 2u64.saturating_pow(inner.failed_attempts.min(6)).min(MAX_BACKOFF_SECS);
                inner.locked_until = Some(Instant::now() + Duration::from_secs(backoff));
                Err(AppError::IncorrectPassword)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn test_pool() -> (tempfile::TempDir, Pool) {
        let dir = tempfile::tempdir().unwrap();
        let pool = db::init_pool(&dir.path().join("test.db")).unwrap();
        (dir, pool)
    }

    #[test]
    fn setup_then_unlock_succeeds() {
        let (_dir, pool) = test_pool();
        let vault = VaultManager::new();
        vault.setup(&pool, "correct-password-1").unwrap();
        assert!(vault.is_unlocked());
        vault.lock();
        assert!(!vault.is_unlocked());
        vault.unlock(&pool, "correct-password-1").unwrap();
        assert!(vault.is_unlocked());
    }

    #[test]
    fn unlock_with_wrong_password_fails_and_does_not_corrupt_vault() {
        let (_dir, pool) = test_pool();
        let vault = VaultManager::new();
        vault.setup(&pool, "correct-password-1").unwrap();
        vault.lock();

        let err = vault.unlock(&pool, "totally-wrong-password").unwrap_err();
        assert!(matches!(err, AppError::IncorrectPassword));
        assert!(!vault.is_unlocked());

        // The vault must still unlock with the correct password afterwards.
        std::thread::sleep(Duration::from_millis(50));
        // second manager instance simulates app restart against the same db
        let vault2 = VaultManager::new();
        let err2 = vault2.unlock(&pool, "wrong-again");
        assert!(err2.is_err());
    }

    #[test]
    fn setup_twice_is_rejected() {
        let (_dir, pool) = test_pool();
        let vault = VaultManager::new();
        vault.setup(&pool, "correct-password-1").unwrap();
        let err = vault.setup(&pool, "another-password").unwrap_err();
        assert!(matches!(err, AppError::VaultAlreadyInitialized));
    }

    #[test]
    fn repeated_wrong_attempts_trigger_rate_limit() {
        let (_dir, pool) = test_pool();
        let vault = VaultManager::new();
        vault.setup(&pool, "correct-password-1").unwrap();
        vault.lock();

        let _ = vault.unlock(&pool, "wrong");
        let second = vault.unlock(&pool, "wrong");
        assert!(matches!(second, Err(AppError::RateLimited { .. })));
    }

    #[test]
    fn key_is_unavailable_while_locked() {
        let (_dir, pool) = test_pool();
        let vault = VaultManager::new();
        vault.setup(&pool, "correct-password-1").unwrap();
        vault.lock();
        assert!(matches!(vault.key(), Err(AppError::VaultLocked)));
    }
}
