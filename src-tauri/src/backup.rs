//! Export the whole database to one encrypted file, and import it on another
//! computer (or after a reinstall).
//!
//! File format (`.remotix`):
//!   MAGIC (12 bytes) | salt length (1 byte) | salt | nonce (12 bytes) | ciphertext
//! The ciphertext is AES-256-GCM over a consistent SQLite snapshot
//! (`VACUUM INTO`), under the vault key itself. The salt is the vault's own
//! salt, so the master password that unlocks the vault also opens the backup,
//! and the restored vault unlocks with that same password. Everything is
//! encrypted, including the hostnames, notes and command history that are
//! plain text inside the live database.
//!
//! Import replaces the live database in place through the SQLite backup API
//! (no restart). It first keeps a copy of the current data next to the
//! database, then migrates the restored data to this version, and drops the
//! Windows Hello wrapping, which is bound to the old machine's TPM.

use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension};
use zeroize::Zeroizing;

use crate::db::{self, Pool};
use crate::error::{AppError, AppResult};
use crate::repositories::SessionLogRepository;
use crate::vault::crypto;
use crate::vault::VaultManager;

const MAGIC: &[u8; 12] = b"REMOTIXBAK01";
const AAD: &[u8] = b"remotix-backup:v1";
const SQLITE_HEADER: &[u8; 16] = b"SQLite format 3\0";
const MAX_BACKUP_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// Writes an encrypted backup of everything to `dest`. Needs the vault unlocked.
pub fn export(pool: &Pool, vault: &VaultManager, dest: &Path) -> AppResult<()> {
    let key = vault.key()?;
    let conn = pool.get()?;
    let salt: Vec<u8> = conn.query_row("SELECT salt FROM vault_metadata LIMIT 1", [], |r| r.get(0))?;
    let dir = data_dir(&conn)?;

    let snapshot = dir.join(format!("remotix-export-{}.db", crate::util::new_id()));
    let snapshot_str = snapshot.to_string_lossy().to_string();
    conn.execute("VACUUM INTO ?1", [&snapshot_str])?;
    drop(conn);
    let bytes = std::fs::read(&snapshot).map(Zeroizing::new);
    let _ = std::fs::remove_file(&snapshot);
    let bytes = bytes.map_err(|e| AppError::Internal(format!("could not read the snapshot: {e}")))?;

    let (ciphertext, nonce) = crypto::encrypt(&key, &bytes, AAD)?;
    let mut out = Vec::with_capacity(MAGIC.len() + 1 + salt.len() + nonce.len() + ciphertext.len());
    out.extend_from_slice(MAGIC);
    out.push(u8::try_from(salt.len()).map_err(|_| AppError::Internal("unexpected salt length".into()))?);
    out.extend_from_slice(&salt);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ciphertext);
    std::fs::write(dest, out).map_err(|e| AppError::Validation(format!("could not write the backup file: {e}")))
}

/// Replaces all data with the backup at `src`, opened with `password`.
/// `before_replace` runs once the backup is known to be good and right before
/// the data is replaced (the app stops sessions and locks the vault there),
/// so a wrong password or a bad file leaves the running app untouched.
/// Returns the path of the copy kept of the data that was replaced.
pub fn import(pool: &Pool, src: &Path, password: &str, before_replace: impl FnOnce()) -> AppResult<Option<PathBuf>> {
    let meta = std::fs::metadata(src).map_err(|e| AppError::Validation(format!("could not open the backup: {e}")))?;
    if meta.len() > MAX_BACKUP_BYTES {
        return Err(AppError::Validation("this file is too large to be a Remotix backup".into()));
    }
    let file = std::fs::read(src).map_err(|e| AppError::Validation(format!("could not read the backup: {e}")))?;
    let (salt, nonce, ciphertext) = parse(&file)?;
    let key = crypto::derive_key(password, salt)?;
    let plain = Zeroizing::new(crypto::decrypt(&key, ciphertext, nonce, AAD).map_err(|_| AppError::IncorrectPassword)?);
    if plain.len() < SQLITE_HEADER.len() || &plain[..SQLITE_HEADER.len()] != SQLITE_HEADER {
        return Err(AppError::Validation("the backup does not contain a Remotix database".into()));
    }

    let mut live = pool.get()?;
    let dir = data_dir(&live)?;
    let staging = dir.join(format!("remotix-import-{}.db", crate::util::new_id()));
    std::fs::write(&staging, &*plain).map_err(|e| AppError::Internal(format!("could not stage the backup: {e}")))?;
    drop(plain);

    let result = (|| {
        let source = Connection::open(&staging)?;
        validate(&source, &key)?;
        before_replace();

        let kept = keep_current_copy(&live, &dir)?;
        {
            let backup = rusqlite::backup::Backup::new(&source, &mut live)?;
            backup.run_to_completion(256, Duration::ZERO, None)?;
        }
        drop(source);

        db::run_migrations(&mut live)?;
        // Bound to the old computer's Windows Hello key; unusable here.
        live.execute("DELETE FROM vault_hello", [])?;
        let _ = SessionLogRepository::close_dangling(&live);
        Ok::<_, AppError>(kept)
    })();

    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(PathBuf::from(format!("{}{suffix}", staging.display())));
    }
    result
}

fn parse(file: &[u8]) -> AppResult<(&[u8], &[u8], &[u8])> {
    let not_backup = || AppError::Validation("this is not a Remotix backup file".into());
    let rest = file.strip_prefix(MAGIC.as_slice()).ok_or_else(not_backup)?;
    let (&salt_len, rest) = rest.split_first().ok_or_else(not_backup)?;
    let salt_len = salt_len as usize;
    if salt_len == 0 || rest.len() < salt_len + crypto::NONCE_LEN + 16 {
        return Err(not_backup());
    }
    let (salt, rest) = rest.split_at(salt_len);
    let (nonce, ciphertext) = rest.split_at(crypto::NONCE_LEN);
    Ok((salt, nonce, ciphertext))
}

/// The decrypted file must be a Remotix database whose vault opens with the
/// same key (it always does for a genuine backup).
fn validate(source: &Connection, key: &crypto::VaultKey) -> AppResult<()> {
    let invalid = || AppError::Validation("the backup does not contain a Remotix database".into());
    let row: Option<(i64, Vec<u8>, Vec<u8>)> = source
        .query_row(
            "SELECT format_version, encrypted_check_value, check_nonce FROM vault_metadata LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .map_err(|_| invalid())?;
    let (format_version, check, nonce) = row.ok_or_else(invalid)?;
    crypto::decrypt(key, &check, &nonce, &crypto::vault_check_aad(format_version)).map_err(|_| invalid())?;
    Ok(())
}

/// Copies the current data next to the database before it is replaced, so
/// an import by mistake can be undone by hand. Skipped when there is no vault.
fn keep_current_copy(live: &Connection, dir: &Path) -> AppResult<Option<PathBuf>> {
    let has_vault: i64 = live.query_row("SELECT COUNT(*) FROM vault_metadata", [], |r| r.get(0))?;
    if has_vault == 0 {
        return Ok(None);
    }
    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let path = dir.join(format!("remotix-before-import-{stamp}.db"));
    live.execute("VACUUM INTO ?1", [path.to_string_lossy().to_string()])?;
    Ok(Some(path))
}

fn data_dir(conn: &Connection) -> AppResult<PathBuf> {
    conn.path()
        .filter(|p| !p.is_empty())
        .and_then(|p| Path::new(p).parent().map(Path::to_path_buf))
        .ok_or_else(|| AppError::Internal("database location unknown".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repositories::{ProfileRepository, SudoRepository};
    use crate::testing::{create_password_profile, open_temp_vault};

    #[test]
    fn export_then_import_into_a_fresh_install() {
        let source = open_temp_vault("correct-password-1");
        let p = create_password_profile(&source, "web", "very-secret-host.example.com", "deploy", "login-pw");
        {
            let conn = source.pool.get().unwrap();
            SudoRepository::save(&conn, &source.vault, &p.id, b"sudo-pw").unwrap();
            ProfileRepository::set_notes(&conn, &p.id, "portainer box").unwrap();
        }
        let out = tempfile::tempdir().unwrap();
        let file = out.path().join("backup.remotix");
        export(&source.pool, &source.vault, &file).unwrap();

        // Fully encrypted: not even the hostname or notes are readable.
        let bytes = std::fs::read(&file).unwrap();
        for needle in [&b"very-secret-host"[..], b"portainer box", b"SQLite format"] {
            assert!(!bytes.windows(needle.len()).any(|w| w == needle));
        }

        // A brand-new install: no vault yet.
        let dir = tempfile::tempdir().unwrap();
        let pool = crate::db::init_pool(&dir.path().join("remotix.db")).unwrap();
        assert!(matches!(import(&pool, &file, "wrong-password", || {}), Err(AppError::IncorrectPassword)));
        let kept = import(&pool, &file, "correct-password-1", || {}).unwrap();
        assert!(kept.is_none());

        let vault = VaultManager::new();
        vault.unlock(&pool, "correct-password-1").unwrap();
        let conn = pool.get().unwrap();
        let restored = ProfileRepository::get(&conn, &p.id).unwrap();
        assert_eq!(restored.hostname, "very-secret-host.example.com");
        assert_eq!(restored.notes, "portainer box");
        assert_eq!(&SudoRepository::load(&conn, &vault, &p.id).unwrap().unwrap()[..], b"sudo-pw");
        let staged = std::fs::read_dir(dir.path())
            .unwrap()
            .filter(|e| e.as_ref().unwrap().file_name().to_string_lossy().contains("import-"))
            .count();
        assert_eq!(staged, 0);
    }

    #[test]
    fn import_over_existing_data_keeps_a_copy() {
        let source = open_temp_vault("correct-password-1");
        create_password_profile(&source, "from-backup", "a.example.com", "u", "pw");
        let out = tempfile::tempdir().unwrap();
        let file = out.path().join("backup.remotix");
        export(&source.pool, &source.vault, &file).unwrap();

        let target = open_temp_vault("other-password-2");
        create_password_profile(&target, "local-only", "b.example.com", "u", "pw");
        let kept = import(&target.pool, &file, "correct-password-1", || target.vault.lock()).unwrap().unwrap();
        assert!(kept.exists());

        let conn = target.pool.get().unwrap();
        let names: Vec<String> = ProfileRepository::list(&conn).unwrap().into_iter().map(|p| p.name).collect();
        assert_eq!(names, ["from-backup"]);
        drop(conn);
        assert!(target.vault.unlock(&target.pool, "correct-password-1").is_ok());
    }

    #[test]
    fn rejects_files_that_are_not_backups() {
        let dir = tempfile::tempdir().unwrap();
        let pool = crate::db::init_pool(&dir.path().join("remotix.db")).unwrap();
        let file = dir.path().join("x.remotix");
        std::fs::write(&file, b"hello").unwrap();
        assert!(matches!(import(&pool, &file, "pw", || {}), Err(AppError::Validation(_))));
    }
}
