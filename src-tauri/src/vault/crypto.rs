use aes_gcm::aead::rand_core::RngCore;
use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::{Algorithm, Argon2, Params, Version};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::error::AppError;

pub const NONCE_LEN: usize = 12;
pub const SALT_LEN: usize = 32;
pub const CURRENT_AAD_VERSION: i64 = 1;

/// Argon2id parameters. 19 MiB / 2 passes / 1 lane is the OWASP-recommended
/// interactive baseline; this runs once per unlock, not per keystroke, so we
/// bias slightly stronger (64 MiB) since it still completes in well under a
/// second on any machine capable of running a desktop SSH client.
fn argon2_params() -> Params {
    Params::new(64 * 1024, 3, 1, Some(32)).expect("valid static argon2 params")
}

#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct VaultKey(pub [u8; 32]);

pub fn generate_salt() -> [u8; SALT_LEN] {
    let mut salt = [0u8; SALT_LEN];
    OsRng.fill_bytes(&mut salt);
    salt
}

pub fn derive_key(password: &str, salt: &[u8]) -> Result<VaultKey, AppError> {
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, argon2_params());
    let mut out = [0u8; 32];
    argon2
        .hash_password_into(password.as_bytes(), salt, &mut out)
        .map_err(|e| {
            tracing::error!(error = %e, "argon2 key derivation failed");
            AppError::Crypto
        })?;
    Ok(VaultKey(out))
}

/// Encrypts `plaintext` under `key`, binding `aad` (typically the secret's
/// record id + format version) so ciphertext cannot be replayed onto a
/// different record. Returns (ciphertext, nonce).
pub fn encrypt(key: &VaultKey, plaintext: &[u8], aad: &[u8]) -> Result<(Vec<u8>, Vec<u8>), AppError> {
    let cipher = Aes256Gcm::new_from_slice(&key.0).map_err(|_| AppError::Crypto)?;
    let mut nonce_bytes = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(
            nonce,
            aes_gcm::aead::Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| AppError::Crypto)?;
    Ok((ciphertext, nonce_bytes.to_vec()))
}

pub fn decrypt(key: &VaultKey, ciphertext: &[u8], nonce: &[u8], aad: &[u8]) -> Result<Vec<u8>, AppError> {
    let cipher = Aes256Gcm::new_from_slice(&key.0).map_err(|_| AppError::Crypto)?;
    if nonce.len() != NONCE_LEN {
        return Err(AppError::Crypto);
    }
    let nonce = Nonce::from_slice(nonce);
    cipher
        .decrypt(
            nonce,
            aes_gcm::aead::Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|_| AppError::Crypto)
}

/// AAD binds ciphertext to the record it belongs to and to the encryption
/// format version, so a secret can never be swapped onto a different
/// record or silently reinterpreted under a future format change.
pub fn secret_aad(secret_id: &str, aad_version: i64) -> Vec<u8> {
    format!("secret:{secret_id}:v{aad_version}").into_bytes()
}

pub fn vault_check_aad(aad_version: i64) -> Vec<u8> {
    format!("vault-check:v{aad_version}").into_bytes()
}

pub fn hello_wrap_aad(aad_version: i64) -> Vec<u8> {
    format!("vault-hello:v{aad_version}").into_bytes()
}

/// Turns a Windows Hello signature over the stored challenge into the key
/// that wraps the vault key. The signature is deterministic (RSA PKCS#1
/// v1.5) and already high-entropy, so a domain-separated SHA-256 suffices.
pub fn hello_wrapping_key(signature: &[u8]) -> VaultKey {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(b"remotix-hello-wrap-v1");
    hasher.update(signature);
    VaultKey(hasher.finalize().into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let key = derive_key("correct horse battery staple", &generate_salt()).unwrap();
        let aad = secret_aad("secret-1", CURRENT_AAD_VERSION);
        let (ct, nonce) = encrypt(&key, b"super-secret-password", &aad).unwrap();
        let pt = decrypt(&key, &ct, &nonce, &aad).unwrap();
        assert_eq!(pt, b"super-secret-password");
    }

    #[test]
    fn decrypt_fails_with_wrong_key() {
        let key1 = derive_key("password-one", &generate_salt()).unwrap();
        let key2 = derive_key("password-two", &generate_salt()).unwrap();
        let aad = secret_aad("secret-1", CURRENT_AAD_VERSION);
        let (ct, nonce) = encrypt(&key1, b"data", &aad).unwrap();
        assert!(decrypt(&key2, &ct, &nonce, &aad).is_err());
    }

    #[test]
    fn decrypt_fails_with_tampered_aad() {
        let key = derive_key("password", &generate_salt()).unwrap();
        let aad = secret_aad("secret-1", CURRENT_AAD_VERSION);
        let (ct, nonce) = encrypt(&key, b"data", &aad).unwrap();
        let wrong_aad = secret_aad("secret-2", CURRENT_AAD_VERSION);
        assert!(decrypt(&key, &ct, &nonce, &wrong_aad).is_err());
    }

    #[test]
    fn decrypt_fails_with_tampered_ciphertext() {
        let key = derive_key("password", &generate_salt()).unwrap();
        let aad = secret_aad("secret-1", CURRENT_AAD_VERSION);
        let (mut ct, nonce) = encrypt(&key, b"data", &aad).unwrap();
        ct[0] ^= 0xFF;
        assert!(decrypt(&key, &ct, &nonce, &aad).is_err());
    }

    #[test]
    fn nonces_are_unique_across_encryptions() {
        let key = derive_key("password", &generate_salt()).unwrap();
        let aad = secret_aad("secret-1", CURRENT_AAD_VERSION);
        let mut nonces = std::collections::HashSet::new();
        for _ in 0..200 {
            let (_, nonce) = encrypt(&key, b"data", &aad).unwrap();
            assert!(nonces.insert(nonce), "nonce reused across encryptions");
        }
    }

    #[test]
    fn same_password_and_salt_derive_identical_key() {
        let salt = generate_salt();
        let k1 = derive_key("password", &salt).unwrap();
        let k2 = derive_key("password", &salt).unwrap();
        assert_eq!(k1.0, k2.0);
    }

    #[test]
    fn different_salts_derive_different_keys() {
        let k1 = derive_key("password", &generate_salt()).unwrap();
        let k2 = derive_key("password", &generate_salt()).unwrap();
        assert_ne!(k1.0, k2.0);
    }
}
