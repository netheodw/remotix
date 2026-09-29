use rusqlite::Connection;
use zeroize::Zeroizing;

use crate::error::AppResult;
use crate::util::{new_id, now_iso};
use crate::vault::crypto::{self, CURRENT_AAD_VERSION};
use crate::vault::VaultManager;

pub struct SecretRepository;

impl SecretRepository {
    /// Encrypts `plaintext` and stores it as a new secret record, returning
    /// its id. Requires the vault to be unlocked.
    pub fn store(conn: &Connection, vault: &VaultManager, plaintext: &[u8]) -> AppResult<String> {
        let key = vault.key()?;
        let id = new_id();
        let aad = crypto::secret_aad(&id, CURRENT_AAD_VERSION);
        let (ciphertext, nonce) = crypto::encrypt(&key, plaintext, &aad)?;
        let now = now_iso();
        conn.execute(
            "INSERT INTO secrets (id, ciphertext, nonce, aad_version, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![id, ciphertext, nonce, CURRENT_AAD_VERSION, now, now],
        )?;
        Ok(id)
    }

    /// Decrypts a secret. The returned buffer zeroizes itself on drop.
    pub fn reveal(conn: &Connection, vault: &VaultManager, id: &str) -> AppResult<Zeroizing<Vec<u8>>> {
        let key = vault.key()?;
        let (ciphertext, nonce, aad_version): (Vec<u8>, Vec<u8>, i64) = conn.query_row(
            "SELECT ciphertext, nonce, aad_version FROM secrets WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        let aad = crypto::secret_aad(id, aad_version);
        let plaintext = crypto::decrypt(&key, &ciphertext, &nonce, &aad)?;
        Ok(Zeroizing::new(plaintext))
    }

    pub fn delete(conn: &Connection, id: &str) -> AppResult<()> {
        conn.execute("DELETE FROM secrets WHERE id = ?1", [id])?;
        Ok(())
    }
}
