use rusqlite::{Connection, OptionalExtension, Row};

use crate::domain::{AuthKind, AuthSecretInput, NewSshProfileInput, SshProfile, UpdateSshProfileInput};
use crate::error::{AppError, AppResult};
use crate::repositories::SecretRepository;
use crate::util::{new_id, now_iso};
use crate::validation::{validate_display_name, validate_hostname, validate_port, validate_username};
use crate::vault::VaultManager;

pub struct ProfileRepository;

const DEFAULT_TERMINAL_TYPE: &str = "xterm-256color";

struct ExistingSecrets {
    authentication_kind: AuthKind,
    secret_id: Option<String>,
    passphrase_secret_id: Option<String>,
}

impl ProfileRepository {
    pub fn list(conn: &Connection) -> AppResult<Vec<SshProfile>> {
        let mut stmt = conn.prepare(&format!("{} ORDER BY sort_order, name", Self::select_clause()))?;
        let rows = stmt.query_map([], Self::map_row)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn get(conn: &Connection, id: &str) -> AppResult<SshProfile> {
        conn.query_row(
            &format!("{} WHERE id = ?1", Self::select_clause()),
            [id],
            Self::map_row,
        )
        .optional()?
        .ok_or_else(|| AppError::NotFound("ssh profile".into()))
    }

    pub fn create(conn: &Connection, vault: &VaultManager, input: NewSshProfileInput) -> AppResult<SshProfile> {
        validate_display_name(&input.name)?;
        validate_hostname(&input.hostname)?;
        validate_port(input.port)?;
        validate_username(&input.username)?;
        if let Some(group) = &input.terminal_group_id {
            Self::assert_group_exists(conn, group)?;
        }

        let (auth_kind, secret_id, passphrase_secret_id) =
            Self::store_new_secret(conn, vault, &input.secret)?;

        let id = new_id();
        let now = now_iso();
        conn.execute(
            "INSERT INTO ssh_profiles (id, terminal_group_id, name, hostname, port, username, authentication_kind, secret_id, passphrase_secret_id, terminal_type, sort_order, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, (SELECT COALESCE(MAX(sort_order), -1) + 1 FROM ssh_profiles WHERE terminal_group_id IS ?2), ?11, ?11)",
            rusqlite::params![
                id,
                input.terminal_group_id,
                input.name.trim(),
                input.hostname.trim(),
                input.port,
                input.username.trim(),
                auth_kind.as_db_str(),
                secret_id,
                passphrase_secret_id,
                input.terminal_type.unwrap_or_else(|| DEFAULT_TERMINAL_TYPE.to_string()),
                now,
            ],
        )?;
        Self::get(conn, &id)
    }

    pub fn update(conn: &Connection, vault: &VaultManager, input: UpdateSshProfileInput) -> AppResult<SshProfile> {
        validate_display_name(&input.name)?;
        validate_hostname(&input.hostname)?;
        validate_port(input.port)?;
        validate_username(&input.username)?;
        if let Some(group) = &input.terminal_group_id {
            Self::assert_group_exists(conn, group)?;
        }
        let existing = Self::existing_secrets(conn, &input.id)?;

        let (auth_kind, secret_id, passphrase_secret_id) = match &input.secret {
            AuthSecretInput::Unchanged => (
                existing.authentication_kind,
                existing.secret_id.clone(),
                existing.passphrase_secret_id.clone(),
            ),
            AuthSecretInput::Clear => {
                Self::delete_existing(conn, &existing)?;
                (AuthKind::Password, None, None)
            }
            _ => {
                let (kind, secret_id, passphrase_id) = Self::store_new_secret(conn, vault, &input.secret)?;
                Self::delete_existing(conn, &existing)?;
                (kind, secret_id, passphrase_id)
            }
        };

        conn.execute(
            "UPDATE ssh_profiles SET terminal_group_id = ?1, name = ?2, hostname = ?3, port = ?4, username = ?5,
             authentication_kind = ?6, secret_id = ?7, passphrase_secret_id = ?8, terminal_type = ?9, updated_at = ?10
             WHERE id = ?11",
            rusqlite::params![
                input.terminal_group_id,
                input.name.trim(),
                input.hostname.trim(),
                input.port,
                input.username.trim(),
                auth_kind.as_db_str(),
                secret_id,
                passphrase_secret_id,
                input.terminal_type.unwrap_or_else(|| DEFAULT_TERMINAL_TYPE.to_string()),
                now_iso(),
                input.id,
            ],
        )?;
        Self::get(conn, &input.id)
    }

    pub fn delete(conn: &Connection, id: &str) -> AppResult<()> {
        let existing = Self::existing_secrets(conn, id)?;
        crate::repositories::SudoRepository::delete_for_profile(conn, id)?;
        conn.execute("DELETE FROM ssh_profiles WHERE id = ?1", [id])?;
        if let Some(sid) = existing.secret_id {
            SecretRepository::delete(conn, &sid)?;
        }
        if let Some(pid) = existing.passphrase_secret_id {
            SecretRepository::delete(conn, &pid)?;
        }
        Ok(())
    }

    pub fn move_to_group(conn: &Connection, id: &str, group_id: Option<String>) -> AppResult<SshProfile> {
        if let Some(group) = &group_id {
            Self::assert_group_exists(conn, group)?;
        }
        conn.execute(
            "UPDATE ssh_profiles SET terminal_group_id = ?1, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![group_id, now_iso(), id],
        )?;
        Self::get(conn, id)
    }

    pub fn set_notes(conn: &Connection, id: &str, notes: &str) -> AppResult<SshProfile> {
        let notes = crate::validation::validate_notes(notes)?;
        let changed = conn.execute(
            "UPDATE ssh_profiles SET notes = ?1, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![notes, now_iso(), id],
        )?;
        if changed == 0 {
            return Err(AppError::NotFound("ssh profile".into()));
        }
        Self::get(conn, id)
    }

    pub fn record_connection_result(conn: &Connection, id: &str, status: &str) -> AppResult<()> {
        conn.execute(
            "UPDATE ssh_profiles SET last_status = ?1, last_connected_at = ?2, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![status, now_iso(), id],
        )?;
        Ok(())
    }

    /// Copies a profile's non-secret fields and re-encrypts a fresh copy of
    /// its secret material under new secret record ids (never reuses
    /// ciphertext, since AAD is bound to the record id).
    pub fn duplicate(conn: &Connection, vault: &VaultManager, id: &str, new_name: &str) -> AppResult<SshProfile> {
        validate_display_name(new_name)?;
        let source = Self::get(conn, id)?;
        let existing = Self::existing_secrets(conn, id)?;

        let secret_id = match &existing.secret_id {
            Some(sid) => {
                let plaintext = SecretRepository::reveal(conn, vault, sid)?;
                Some(SecretRepository::store(conn, vault, &plaintext)?)
            }
            None => None,
        };
        let passphrase_secret_id = match &existing.passphrase_secret_id {
            Some(pid) => {
                let plaintext = SecretRepository::reveal(conn, vault, pid)?;
                Some(SecretRepository::store(conn, vault, &plaintext)?)
            }
            None => None,
        };

        let new_id_value = new_id();
        let now = now_iso();
        conn.execute(
            "INSERT INTO ssh_profiles (id, terminal_group_id, name, hostname, port, username, authentication_kind, secret_id, passphrase_secret_id, terminal_type, sort_order, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, (SELECT COALESCE(MAX(sort_order), -1) + 1 FROM ssh_profiles WHERE terminal_group_id IS ?2), ?11, ?11)",
            rusqlite::params![
                new_id_value,
                source.terminal_group_id,
                new_name.trim(),
                source.hostname,
                source.port,
                source.username,
                source.authentication_kind.as_db_str(),
                secret_id,
                passphrase_secret_id,
                source.terminal_type,
                now,
            ],
        )?;
        Self::get(conn, &new_id_value)
    }

    /// Loads the plaintext secret material needed to open an SSH session.
    /// Callers must not persist or log the returned values.
    pub fn load_auth_material(
        conn: &Connection,
        vault: &VaultManager,
        id: &str,
    ) -> AppResult<crate::ssh::session::AuthMaterial> {
        let profile = Self::get(conn, id)?;
        let existing = Self::existing_secrets(conn, id)?;
        use crate::ssh::session::AuthMaterial;
        match profile.authentication_kind {
            AuthKind::Password => {
                let sid = existing.secret_id.ok_or_else(|| AppError::Validation("profile has no stored password".into()))?;
                let password = SecretRepository::reveal(conn, vault, &sid)?;
                Ok(AuthMaterial::Password(String::from_utf8_lossy(&password).to_string()))
            }
            AuthKind::PrivateKey => {
                let sid = existing.secret_id.ok_or_else(|| AppError::Validation("profile has no stored private key".into()))?;
                let key = SecretRepository::reveal(conn, vault, &sid)?;
                Ok(AuthMaterial::PrivateKey {
                    key_pem: String::from_utf8_lossy(&key).to_string(),
                    passphrase: None,
                })
            }
            AuthKind::PrivateKeyPassphrase => {
                let sid = existing.secret_id.ok_or_else(|| AppError::Validation("profile has no stored private key".into()))?;
                let pid = existing
                    .passphrase_secret_id
                    .ok_or_else(|| AppError::Validation("profile has no stored passphrase".into()))?;
                let key = SecretRepository::reveal(conn, vault, &sid)?;
                let passphrase = SecretRepository::reveal(conn, vault, &pid)?;
                Ok(AuthMaterial::PrivateKey {
                    key_pem: String::from_utf8_lossy(&key).to_string(),
                    passphrase: Some(String::from_utf8_lossy(&passphrase).to_string()),
                })
            }
        }
    }

    /// Returns the stored password of a password-auth profile so the user can
    /// view it in the editor. This is the one deliberate path that sends a
    /// plaintext secret to the UI; it is limited to passwords (never private
    /// keys or passphrases) and requires an unlocked vault.
    pub fn reveal_password(conn: &Connection, vault: &VaultManager, id: &str) -> AppResult<String> {
        let profile = Self::get(conn, id)?;
        if profile.authentication_kind != AuthKind::Password {
            return Err(AppError::Validation("profile does not use password authentication".into()));
        }
        let sid = Self::existing_secrets(conn, id)?
            .secret_id
            .ok_or_else(|| AppError::Validation("profile has no stored password".into()))?;
        let password = SecretRepository::reveal(conn, vault, &sid)?;
        Ok(String::from_utf8_lossy(&password).to_string())
    }

    fn store_new_secret(
        conn: &Connection,
        vault: &VaultManager,
        secret: &AuthSecretInput,
    ) -> AppResult<(AuthKind, Option<String>, Option<String>)> {
        match secret {
            AuthSecretInput::Password { password } => {
                if password.is_empty() {
                    return Err(AppError::Validation("password is required".into()));
                }
                let id = SecretRepository::store(conn, vault, password.as_bytes())?;
                Ok((AuthKind::Password, Some(id), None))
            }
            AuthSecretInput::PrivateKey { private_key } => {
                if private_key.trim().is_empty() {
                    return Err(AppError::Validation("private key is required".into()));
                }
                let id = SecretRepository::store(conn, vault, private_key.as_bytes())?;
                Ok((AuthKind::PrivateKey, Some(id), None))
            }
            AuthSecretInput::PrivateKeyPassphrase { private_key, passphrase } => {
                if private_key.trim().is_empty() {
                    return Err(AppError::Validation("private key is required".into()));
                }
                let key_id = SecretRepository::store(conn, vault, private_key.as_bytes())?;
                let pass_id = SecretRepository::store(conn, vault, passphrase.as_bytes())?;
                Ok((AuthKind::PrivateKeyPassphrase, Some(key_id), Some(pass_id)))
            }
            AuthSecretInput::Unchanged | AuthSecretInput::Clear => {
                Err(AppError::Validation("an authentication secret is required".into()))
            }
        }
    }

    fn delete_existing(conn: &Connection, existing: &ExistingSecrets) -> AppResult<()> {
        if let Some(sid) = &existing.secret_id {
            SecretRepository::delete(conn, sid)?;
        }
        if let Some(pid) = &existing.passphrase_secret_id {
            SecretRepository::delete(conn, pid)?;
        }
        Ok(())
    }

    fn existing_secrets(conn: &Connection, id: &str) -> AppResult<ExistingSecrets> {
        conn.query_row(
            "SELECT authentication_kind, secret_id, passphrase_secret_id FROM ssh_profiles WHERE id = ?1",
            [id],
            |r| {
                let kind_str: String = r.get(0)?;
                Ok(ExistingSecrets {
                    authentication_kind: AuthKind::from_db_str(&kind_str).unwrap_or(AuthKind::Password),
                    secret_id: r.get(1)?,
                    passphrase_secret_id: r.get(2)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| AppError::NotFound("ssh profile".into()))
    }

    fn assert_group_exists(conn: &Connection, group_id: &str) -> AppResult<()> {
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM terminal_groups WHERE id = ?1)",
            [group_id],
            |r| r.get(0),
        )?;
        if !exists {
            return Err(AppError::NotFound("terminal group".into()));
        }
        Ok(())
    }

    fn select_clause() -> &'static str {
        "SELECT id, terminal_group_id, name, hostname, port, username, authentication_kind, secret_id, passphrase_secret_id, terminal_type, sort_order, last_status, last_connected_at, created_at, updated_at, notes FROM ssh_profiles"
    }

    fn map_row(row: &Row) -> rusqlite::Result<SshProfile> {
        let auth_kind_str: String = row.get(6)?;
        let secret_id: Option<String> = row.get(7)?;
        let passphrase_secret_id: Option<String> = row.get(8)?;
        Ok(SshProfile {
            id: row.get(0)?,
            terminal_group_id: row.get(1)?,
            name: row.get(2)?,
            hostname: row.get(3)?,
            port: row.get::<_, i64>(4)? as u16,
            username: row.get(5)?,
            authentication_kind: AuthKind::from_db_str(&auth_kind_str).unwrap_or(AuthKind::Password),
            has_secret: secret_id.is_some(),
            has_passphrase: passphrase_secret_id.is_some(),
            terminal_type: row.get(9)?,
            sort_order: row.get(10)?,
            last_status: row.get(11)?,
            last_connected_at: row.get(12)?,
            created_at: row.get(13)?,
            updated_at: row.get(14)?,
            notes: row.get(15)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn setup() -> (tempfile::TempDir, db::Pool, VaultManager) {
        let dir = tempfile::tempdir().unwrap();
        let pool = db::init_pool(&dir.path().join("t.db")).unwrap();
        let vault = VaultManager::new();
        {
            let conn = pool.get().unwrap();
            vault.setup(&pool, "correct-password-1").unwrap();
            drop(conn);
        }
        (dir, pool, vault)
    }

    fn password_input(name: &str, password: &str) -> NewSshProfileInput {
        NewSshProfileInput {
            terminal_group_id: None,
            name: name.into(),
            hostname: "example.com".into(),
            port: 22,
            username: "root".into(),
            terminal_type: None,
            secret: AuthSecretInput::Password { password: password.into() },
        }
    }

    #[test]
    fn create_profile_with_password_hides_secret_in_read_model() {
        let (_dir, pool, vault) = setup();
        let conn = pool.get().unwrap();
        let profile = ProfileRepository::create(&conn, &vault, password_input("Prod", "s3cret")).unwrap();
        assert!(profile.has_secret);
        assert_eq!(profile.authentication_kind, AuthKind::Password);

        let material = ProfileRepository::load_auth_material(&conn, &vault, &profile.id).unwrap();
        match material {
            crate::ssh::session::AuthMaterial::Password(p) => assert_eq!(p, "s3cret"),
            _ => panic!("expected password auth material"),
        }
    }

    #[test]
    fn reveal_password_returns_stored_password_only_for_password_profiles() {
        let (_dir, pool, vault) = setup();
        let conn = pool.get().unwrap();
        let profile = ProfileRepository::create(&conn, &vault, password_input("Prod", "s3cret")).unwrap();
        assert_eq!(ProfileRepository::reveal_password(&conn, &vault, &profile.id).unwrap(), "s3cret");

        let mut key_input = password_input("Key", "unused");
        key_input.secret = AuthSecretInput::PrivateKey { private_key: "-----BEGIN KEY-----".into() };
        let key_profile = ProfileRepository::create(&conn, &vault, key_input).unwrap();
        assert!(ProfileRepository::reveal_password(&conn, &vault, &key_profile.id).is_err());
    }

    #[test]
    fn update_unchanged_keeps_existing_secret() {
        let (_dir, pool, vault) = setup();
        let conn = pool.get().unwrap();
        let profile = ProfileRepository::create(&conn, &vault, password_input("Prod", "s3cret")).unwrap();

        let updated = ProfileRepository::update(
            &conn,
            &vault,
            UpdateSshProfileInput {
                id: profile.id.clone(),
                terminal_group_id: None,
                name: "Prod Renamed".into(),
                hostname: "example.com".into(),
                port: 22,
                username: "root".into(),
                terminal_type: None,
                secret: AuthSecretInput::Unchanged,
            },
        )
        .unwrap();
        assert_eq!(updated.name, "Prod Renamed");
        let material = ProfileRepository::load_auth_material(&conn, &vault, &profile.id).unwrap();
        assert!(matches!(material, crate::ssh::session::AuthMaterial::Password(p) if p == "s3cret"));
    }

    #[test]
    fn notes_are_stored_trimmed_and_survive_an_update() {
        let (_dir, pool, vault) = setup();
        let conn = pool.get().unwrap();
        let profile = ProfileRepository::create(&conn, &vault, password_input("Prod", "s3cret")).unwrap();
        assert_eq!(profile.notes, "");
        let p = ProfileRepository::set_notes(&conn, &profile.id, "runs portainer\n\n").unwrap();
        assert_eq!(p.notes, "runs portainer");
        assert!(ProfileRepository::set_notes(&conn, &profile.id, &"x".repeat(10_001)).is_err());
        assert!(ProfileRepository::set_notes(&conn, "missing", "x").is_err());
        let updated = ProfileRepository::update(
            &conn,
            &vault,
            UpdateSshProfileInput {
                id: profile.id.clone(),
                terminal_group_id: None,
                name: "Prod 2".into(),
                hostname: "example.com".into(),
                port: 22,
                username: "root".into(),
                terminal_type: None,
                secret: AuthSecretInput::Unchanged,
            },
        )
        .unwrap();
        assert_eq!(updated.notes, "runs portainer");
    }

    #[test]
    fn delete_profile_removes_its_secret_row() {
        let (_dir, pool, vault) = setup();
        let conn = pool.get().unwrap();
        let profile = ProfileRepository::create(&conn, &vault, password_input("Prod", "s3cret")).unwrap();
        ProfileRepository::delete(&conn, &profile.id).unwrap();

        let remaining: i64 = conn.query_row("SELECT COUNT(*) FROM secrets", [], |r| r.get(0)).unwrap();
        assert_eq!(remaining, 0);
        assert!(ProfileRepository::get(&conn, &profile.id).is_err());
    }

    #[test]
    fn duplicate_profile_re_encrypts_under_a_new_secret_id() {
        let (_dir, pool, vault) = setup();
        let conn = pool.get().unwrap();
        let profile = ProfileRepository::create(&conn, &vault, password_input("Prod", "s3cret")).unwrap();
        let dup = ProfileRepository::duplicate(&conn, &vault, &profile.id, "Prod copy").unwrap();
        assert_ne!(dup.id, profile.id);

        let material = ProfileRepository::load_auth_material(&conn, &vault, &dup.id).unwrap();
        assert!(matches!(material, crate::ssh::session::AuthMaterial::Password(p) if p == "s3cret"));
    }

    #[test]
    fn rejects_invalid_hostname_and_port() {
        let (_dir, pool, vault) = setup();
        let conn = pool.get().unwrap();
        let mut input = password_input("Bad", "s3cret");
        input.hostname = "".into();
        assert!(ProfileRepository::create(&conn, &vault, input).is_err());

        let mut input2 = password_input("Bad Port", "s3cret");
        input2.port = 0;
        assert!(ProfileRepository::create(&conn, &vault, input2).is_err());
    }

    #[test]
    fn private_key_with_passphrase_round_trips() {
        let (_dir, pool, vault) = setup();
        let conn = pool.get().unwrap();
        let input = NewSshProfileInput {
            terminal_group_id: None,
            name: "Key host".into(),
            hostname: "example.com".into(),
            port: 22,
            username: "deploy".into(),
            terminal_type: None,
            secret: AuthSecretInput::PrivateKeyPassphrase {
                private_key: "-----BEGIN KEY-----abc-----END KEY-----".into(),
                passphrase: "hunter2".into(),
            },
        };
        let profile = ProfileRepository::create(&conn, &vault, input).unwrap();
        assert!(profile.has_secret && profile.has_passphrase);
        let material = ProfileRepository::load_auth_material(&conn, &vault, &profile.id).unwrap();
        match material {
            crate::ssh::session::AuthMaterial::PrivateKey { key_pem, passphrase } => {
                assert_eq!(key_pem, "-----BEGIN KEY-----abc-----END KEY-----");
                assert_eq!(passphrase.as_deref(), Some("hunter2"));
            }
            _ => panic!("expected private key auth material"),
        }
    }
}
