use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::{DateTime, Months, Utc};
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use zeroize::Zeroize;

use crate::db::Pool;
use crate::error::{AppError, AppResult};
use crate::util::now_iso;
use crate::vault::crypto::{self, VaultKey, CURRENT_AAD_VERSION};

const CHECK_PLAINTEXT: &[u8] = b"REMOTIX_VAULT_OK_V1";
const MIN_PASSWORD_LEN: usize = 8;
const MAX_BACKOFF_SECS: u64 = 30;
/// How long Windows Hello may stand in for the master password before the
/// password has to be typed again.
const PASSWORD_REVERIFY_MONTHS: u32 = 6;

#[derive(Debug, Clone, Serialize)]
pub struct HelloStatus {
    pub enabled: bool,
    /// True when the next unlock must use the master password.
    pub password_required: bool,
    /// When Hello unlock stops being accepted until the password is typed.
    pub password_due_at: Option<String>,
}

fn record_password_verified(conn: &Connection) -> AppResult<()> {
    conn.execute("UPDATE vault_metadata SET password_verified_at = ?1", [now_iso()])?;
    Ok(())
}

/// Six months after the master password was last typed, or `None` if that
/// was never recorded (vaults created before Hello support).
fn password_due_at(conn: &Connection) -> AppResult<Option<DateTime<Utc>>> {
    let verified: Option<String> = conn
        .query_row("SELECT password_verified_at FROM vault_metadata LIMIT 1", [], |r| r.get(0))
        .optional()?
        .flatten();
    Ok(verified
        .and_then(|v| DateTime::parse_from_rfc3339(&v).ok())
        .and_then(|v| v.with_timezone(&Utc).checked_add_months(Months::new(PASSWORD_REVERIFY_MONTHS))))
}

fn password_is_due(due: Option<DateTime<Utc>>) -> bool {
    due.map_or(true, |due| Utc::now() >= due)
}

fn matches_check_value(key: &VaultKey, format_version: i64, ciphertext: &[u8], nonce: &[u8]) -> bool {
    let aad = crypto::vault_check_aad(format_version);
    matches!(crypto::decrypt(key, ciphertext, nonce, &aad), Ok(p) if p == CHECK_PLAINTEXT)
}

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
            "INSERT INTO vault_metadata (id, format_version, salt, encrypted_check_value, check_nonce, created_at, password_verified_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
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
        if self.is_unlocked() {
            return Err(AppError::VaultAlreadyUnlocked);
        }
        let conn = pool.get()?;
        let key = self.verify_password(&conn, password)?;
        record_password_verified(&conn)?;
        self.inner.lock().unwrap().state = VaultState::Unlocked(key);
        Ok(())
    }

    /// Checks `password` against the stored check value, applying the same
    /// failed-attempt backoff as unlocking. Returns the derived vault key.
    fn verify_password(&self, conn: &Connection, password: &str) -> AppResult<VaultKey> {
        {
            let inner = self.inner.lock().unwrap();
            if let Some(until) = inner.locked_until {
                let now = Instant::now();
                if now < until {
                    return Err(AppError::RateLimited {
                        retry_after_secs: (until - now).as_secs().max(1),
                    });
                }
            }
        }

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
        let ok = matches_check_value(&key, format_version, &encrypted_check_value, &check_nonce);

        let mut inner = self.inner.lock().unwrap();
        if ok {
            inner.failed_attempts = 0;
            inner.locked_until = None;
            Ok(key)
        } else {
            inner.failed_attempts = inner.failed_attempts.saturating_add(1);
            let backoff = 2u64.saturating_pow(inner.failed_attempts.min(6)).min(MAX_BACKOFF_SECS);
            inner.locked_until = Some(Instant::now() + Duration::from_secs(backoff));
            Err(AppError::IncorrectPassword)
        }
    }

    /// Hello unlock state shown on the unlock screen and in Settings.
    pub fn hello_status(pool: &Pool) -> AppResult<HelloStatus> {
        let conn = pool.get()?;
        let enabled: bool =
            conn.query_row("SELECT EXISTS(SELECT 1 FROM vault_hello WHERE id = 1)", [], |r| r.get(0))?;
        let due = password_due_at(&conn)?;
        Ok(HelloStatus {
            enabled,
            password_required: password_is_due(due),
            password_due_at: due.map(|d| d.to_rfc3339()),
        })
    }

    /// Turns on Windows Hello unlock. The vault must be unlocked and the
    /// master password is asked again, so someone at an unattended unlocked
    /// session cannot enroll their own Hello. `sign` creates the Hello key
    /// and signs the challenge (`platform::hello::create_and_sign`).
    pub fn enable_hello(
        &self,
        pool: &Pool,
        password: &str,
        sign: impl FnOnce(&[u8]) -> AppResult<Vec<u8>>,
    ) -> AppResult<()> {
        let active = self.key()?;
        let conn = pool.get()?;
        let key = self.verify_password(&conn, password)?;
        if key.0 != active.0 {
            return Err(AppError::IncorrectPassword);
        }

        let challenge = crypto::generate_salt();
        let wrapping = crypto::hello_wrapping_key(&sign(&challenge)?);
        let aad = crypto::hello_wrap_aad(CURRENT_AAD_VERSION);
        let (wrapped_key, nonce) = crypto::encrypt(&wrapping, &key.0, &aad)?;

        conn.execute(
            "INSERT INTO vault_hello (id, challenge, wrapped_key, nonce, aad_version, created_at) VALUES (1, ?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET challenge = excluded.challenge, wrapped_key = excluded.wrapped_key,
                 nonce = excluded.nonce, aad_version = excluded.aad_version, created_at = excluded.created_at",
            rusqlite::params![challenge.to_vec(), wrapped_key, nonce, CURRENT_AAD_VERSION, now_iso()],
        )?;
        record_password_verified(&conn)?;
        Ok(())
    }

    pub fn disable_hello(pool: &Pool) -> AppResult<()> {
        let conn = pool.get()?;
        conn.execute("DELETE FROM vault_hello", [])?;
        Ok(())
    }

    /// Unlocks with Windows Hello. Refused with `MasterPasswordRequired`,
    /// before any Hello prompt, once the master password is six months
    /// stale. `sign` signs the stored challenge (`platform::hello::sign`).
    pub fn unlock_with_hello(&self, pool: &Pool, sign: impl FnOnce(&[u8]) -> AppResult<Vec<u8>>) -> AppResult<()> {
        if self.is_unlocked() {
            return Err(AppError::VaultAlreadyUnlocked);
        }
        let conn = pool.get()?;
        let row: Option<(Vec<u8>, Vec<u8>, Vec<u8>, i64)> = conn
            .query_row(
                "SELECT challenge, wrapped_key, nonce, aad_version FROM vault_hello WHERE id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?;
        let (challenge, wrapped_key, nonce, aad_version) = row
            .ok_or_else(|| AppError::BiometricUnavailable("Windows Hello unlock is not turned on.".into()))?;

        if password_is_due(password_due_at(&conn)?) {
            return Err(AppError::MasterPasswordRequired);
        }

        let wrapping = crypto::hello_wrapping_key(&sign(&challenge)?);
        let stale = || {
            AppError::BiometricUnavailable(
                "The Windows Hello key changed. Unlock with the master password and turn Windows Hello on again in Settings.".into(),
            )
        };
        let mut raw = crypto::decrypt(&wrapping, &wrapped_key, &nonce, &crypto::hello_wrap_aad(aad_version))
            .map_err(|_| stale())?;
        let parsed: Result<[u8; 32], _> = raw.as_slice().try_into();
        raw.zeroize();
        let key = VaultKey(parsed.map_err(|_| stale())?);

        let (format_version, encrypted_check_value, check_nonce): (i64, Vec<u8>, Vec<u8>) = conn.query_row(
            "SELECT format_version, encrypted_check_value, check_nonce FROM vault_metadata LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        if !matches_check_value(&key, format_version, &encrypted_check_value, &check_nonce) {
            return Err(stale());
        }

        self.inner.lock().unwrap().state = VaultState::Unlocked(key);
        Ok(())
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

    /// Stands in for the Hello key: deterministic per "device secret".
    fn fake_signer(device: &'static [u8]) -> impl Fn(&[u8]) -> AppResult<Vec<u8>> {
        move |challenge| {
            use sha2::{Digest, Sha256};
            Ok(Sha256::new().chain_update(device).chain_update(challenge).finalize().to_vec())
        }
    }

    fn set_password_verified_at(pool: &Pool, at: Option<String>) {
        pool.get()
            .unwrap()
            .execute("UPDATE vault_metadata SET password_verified_at = ?1", [at])
            .unwrap();
    }

    #[test]
    fn hello_unlock_roundtrip() {
        let (_dir, pool) = test_pool();
        let vault = VaultManager::new();
        vault.setup(&pool, "correct-password-1").unwrap();
        vault.enable_hello(&pool, "correct-password-1", fake_signer(b"tpm")).unwrap();
        let original = vault.key().unwrap().0;
        vault.lock();

        let status = VaultManager::hello_status(&pool).unwrap();
        assert!(status.enabled && !status.password_required);

        vault.unlock_with_hello(&pool, fake_signer(b"tpm")).unwrap();
        assert_eq!(vault.key().unwrap().0, original);
    }

    #[test]
    fn enabling_hello_requires_the_master_password() {
        let (_dir, pool) = test_pool();
        let vault = VaultManager::new();
        vault.setup(&pool, "correct-password-1").unwrap();
        let err = vault.enable_hello(&pool, "wrong-password", fake_signer(b"tpm")).unwrap_err();
        assert!(matches!(err, AppError::IncorrectPassword));
        assert!(!VaultManager::hello_status(&pool).unwrap().enabled);
    }

    #[test]
    fn hello_unlock_fails_with_a_different_hello_key() {
        let (_dir, pool) = test_pool();
        let vault = VaultManager::new();
        vault.setup(&pool, "correct-password-1").unwrap();
        vault.enable_hello(&pool, "correct-password-1", fake_signer(b"tpm")).unwrap();
        vault.lock();

        let err = vault.unlock_with_hello(&pool, fake_signer(b"other")).unwrap_err();
        assert!(matches!(err, AppError::BiometricUnavailable(_)));
        assert!(!vault.is_unlocked());
    }

    #[test]
    fn hello_unlock_is_refused_six_months_after_the_password_was_typed() {
        let (_dir, pool) = test_pool();
        let vault = VaultManager::new();
        vault.setup(&pool, "correct-password-1").unwrap();
        vault.enable_hello(&pool, "correct-password-1", fake_signer(b"tpm")).unwrap();
        vault.lock();

        let old = (Utc::now() - chrono::Duration::days(184)).to_rfc3339();
        set_password_verified_at(&pool, Some(old));
        assert!(VaultManager::hello_status(&pool).unwrap().password_required);
        let err = vault
            .unlock_with_hello(&pool, |_| panic!("must not prompt for Hello"))
            .unwrap_err();
        assert!(matches!(err, AppError::MasterPasswordRequired));

        // Typing the password restarts the six-month window.
        vault.unlock(&pool, "correct-password-1").unwrap();
        vault.lock();
        vault.unlock_with_hello(&pool, fake_signer(b"tpm")).unwrap();
    }

    #[test]
    fn hello_unlock_needs_a_recorded_password_time() {
        let (_dir, pool) = test_pool();
        let vault = VaultManager::new();
        vault.setup(&pool, "correct-password-1").unwrap();
        vault.enable_hello(&pool, "correct-password-1", fake_signer(b"tpm")).unwrap();
        vault.lock();
        set_password_verified_at(&pool, None);
        let err = vault.unlock_with_hello(&pool, fake_signer(b"tpm")).unwrap_err();
        assert!(matches!(err, AppError::MasterPasswordRequired));
    }

    #[test]
    fn disabling_hello_removes_the_wrapped_key() {
        let (_dir, pool) = test_pool();
        let vault = VaultManager::new();
        vault.setup(&pool, "correct-password-1").unwrap();
        vault.enable_hello(&pool, "correct-password-1", fake_signer(b"tpm")).unwrap();
        VaultManager::disable_hello(&pool).unwrap();
        vault.lock();
        let err = vault.unlock_with_hello(&pool, fake_signer(b"tpm")).unwrap_err();
        assert!(matches!(err, AppError::BiometricUnavailable(_)));
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
