use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use zeroize::Zeroizing;

use crate::error::AppResult;
use crate::repositories::SecretRepository;
use crate::util::now_iso;
use crate::vault::VaultManager;

/// What the UI may know about a profile's sudo password: never the value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SudoStatus {
    pub saved: bool,
    pub remember: bool,
}

pub struct SudoRepository;

impl SudoRepository {
    pub fn status(conn: &Connection, profile_id: &str) -> AppResult<SudoStatus> {
        let row: Option<(Option<String>, bool)> = conn
            .query_row(
                "SELECT secret_id, remember FROM sudo_passwords WHERE ssh_profile_id = ?1",
                [profile_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        Ok(match row {
            Some((secret_id, remember)) => SudoStatus { saved: secret_id.is_some(), remember },
            None => SudoStatus { saved: false, remember: true },
        })
    }

    /// The saved password, decrypted. `None` when nothing is saved or the
    /// user turned remembering off.
    pub fn load(conn: &Connection, vault: &VaultManager, profile_id: &str) -> AppResult<Option<Zeroizing<Vec<u8>>>> {
        match Self::secret_id(conn, profile_id)? {
            Some(sid) => Ok(Some(SecretRepository::reveal(conn, vault, &sid)?)),
            None => Ok(None),
        }
    }

    /// Stores `password` for the profile, replacing any earlier one. Does
    /// nothing (returns false) when the user chose not to remember it.
    pub fn save(conn: &Connection, vault: &VaultManager, profile_id: &str, password: &[u8]) -> AppResult<bool> {
        if !Self::status(conn, profile_id)?.remember {
            return Ok(false);
        }
        let new_id = SecretRepository::store(conn, vault, password)?;
        let old = Self::secret_id(conn, profile_id)?;
        conn.execute(
            "INSERT INTO sudo_passwords (ssh_profile_id, secret_id, remember, updated_at) VALUES (?1, ?2, 1, ?3)
             ON CONFLICT(ssh_profile_id) DO UPDATE SET secret_id = excluded.secret_id, updated_at = excluded.updated_at",
            rusqlite::params![profile_id, new_id, now_iso()],
        )?;
        if let Some(old) = old {
            SecretRepository::delete(conn, &old)?;
        }
        Ok(true)
    }

    /// Deletes the saved password. Remembering stays as it was, so with it
    /// on the next password typed at a sudo prompt is saved again.
    pub fn forget(conn: &Connection, profile_id: &str) -> AppResult<()> {
        if let Some(sid) = Self::secret_id(conn, profile_id)? {
            conn.execute(
                "UPDATE sudo_passwords SET secret_id = NULL, updated_at = ?1 WHERE ssh_profile_id = ?2",
                rusqlite::params![now_iso(), profile_id],
            )?;
            SecretRepository::delete(conn, &sid)?;
        }
        Ok(())
    }

    /// Turning remembering off also deletes the saved password, so sudo asks
    /// every time from then on.
    pub fn set_remember(conn: &Connection, profile_id: &str, remember: bool) -> AppResult<SudoStatus> {
        if !remember {
            Self::forget(conn, profile_id)?;
        }
        conn.execute(
            "INSERT INTO sudo_passwords (ssh_profile_id, secret_id, remember, updated_at) VALUES (?1, NULL, ?2, ?3)
             ON CONFLICT(ssh_profile_id) DO UPDATE SET remember = excluded.remember, updated_at = excluded.updated_at",
            rusqlite::params![profile_id, remember, now_iso()],
        )?;
        Self::status(conn, profile_id)
    }

    /// Called before a profile is deleted: the row itself cascades, but the
    /// secret it points at would be left behind.
    pub fn delete_for_profile(conn: &Connection, profile_id: &str) -> AppResult<()> {
        Self::forget(conn, profile_id)?;
        conn.execute("DELETE FROM sudo_passwords WHERE ssh_profile_id = ?1", [profile_id])?;
        Ok(())
    }

    fn secret_id(conn: &Connection, profile_id: &str) -> AppResult<Option<String>> {
        Ok(conn
            .query_row(
                "SELECT secret_id FROM sudo_passwords WHERE ssh_profile_id = ?1 AND remember = 1",
                [profile_id],
                |r| r.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{create_password_profile, open_temp_vault};

    #[test]
    fn defaults_to_remember_with_nothing_saved() {
        let fx = open_temp_vault("correct-password-1");
        let p = create_password_profile(&fx, "web", "example.com", "deploy", "pw");
        let conn = fx.pool.get().unwrap();
        assert_eq!(SudoRepository::status(&conn, &p.id).unwrap(), SudoStatus { saved: false, remember: true });
        assert!(SudoRepository::load(&conn, &fx.vault, &p.id).unwrap().is_none());
    }

    #[test]
    fn save_load_replace_and_forget() {
        let fx = open_temp_vault("correct-password-1");
        let p = create_password_profile(&fx, "web", "example.com", "deploy", "pw");
        let conn = fx.pool.get().unwrap();
        assert!(SudoRepository::save(&conn, &fx.vault, &p.id, b"first").unwrap());
        assert!(SudoRepository::save(&conn, &fx.vault, &p.id, b"second").unwrap());
        assert_eq!(&SudoRepository::load(&conn, &fx.vault, &p.id).unwrap().unwrap()[..], b"second");
        // The login password plus exactly one sudo password: the old one is gone.
        let secrets: i64 = conn.query_row("SELECT COUNT(*) FROM secrets", [], |r| r.get(0)).unwrap();
        assert_eq!(secrets, 2);

        SudoRepository::forget(&conn, &p.id).unwrap();
        assert_eq!(SudoRepository::status(&conn, &p.id).unwrap(), SudoStatus { saved: false, remember: true });
        let secrets: i64 = conn.query_row("SELECT COUNT(*) FROM secrets", [], |r| r.get(0)).unwrap();
        assert_eq!(secrets, 1);
    }

    #[test]
    fn remember_off_deletes_and_blocks_saving() {
        let fx = open_temp_vault("correct-password-1");
        let p = create_password_profile(&fx, "web", "example.com", "deploy", "pw");
        let conn = fx.pool.get().unwrap();
        SudoRepository::save(&conn, &fx.vault, &p.id, b"secret").unwrap();
        let status = SudoRepository::set_remember(&conn, &p.id, false).unwrap();
        assert_eq!(status, SudoStatus { saved: false, remember: false });
        assert!(!SudoRepository::save(&conn, &fx.vault, &p.id, b"secret").unwrap());
        assert!(SudoRepository::load(&conn, &fx.vault, &p.id).unwrap().is_none());

        let status = SudoRepository::set_remember(&conn, &p.id, true).unwrap();
        assert_eq!(status, SudoStatus { saved: false, remember: true });
        assert!(SudoRepository::save(&conn, &fx.vault, &p.id, b"secret").unwrap());
    }

    #[test]
    fn deleting_the_profile_removes_the_sudo_secret() {
        let fx = open_temp_vault("correct-password-1");
        let p = create_password_profile(&fx, "web", "example.com", "deploy", "pw");
        let conn = fx.pool.get().unwrap();
        SudoRepository::save(&conn, &fx.vault, &p.id, b"secret").unwrap();
        crate::repositories::ProfileRepository::delete(&conn, &p.id).unwrap();
        let secrets: i64 = conn.query_row("SELECT COUNT(*) FROM secrets", [], |r| r.get(0)).unwrap();
        assert_eq!(secrets, 0);
    }
}
