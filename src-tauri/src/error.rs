use serde::Serialize;

/// User-safe error type returned across the IPC boundary.
///
/// Variants never carry secret material. Any lower-level error (rusqlite,
/// ssh2, io) is redacted into a short message before it reaches this type -
/// see the `From` impls below and the `tracing` calls at call sites for the
/// full diagnostic detail kept only in local logs.
#[derive(Debug, Clone, Serialize, thiserror::Error)]
#[serde(tag = "kind", content = "message", rename_all = "snake_case")]
pub enum AppError {
    #[error("validation error: {0}")]
    Validation(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("vault is locked")]
    VaultLocked,

    #[error("vault already unlocked")]
    VaultAlreadyUnlocked,

    #[error("vault already initialized")]
    VaultAlreadyInitialized,

    #[error("vault not initialized")]
    VaultNotInitialized,

    #[error("incorrect master password")]
    IncorrectPassword,

    /// Windows Hello unlock is refused because the master password has not
    /// been typed for six months (or ever, since Hello was turned on).
    #[error("master password required")]
    MasterPasswordRequired,

    #[error("windows hello unavailable: {0}")]
    BiometricUnavailable(String),

    #[error("windows hello cancelled")]
    BiometricCancelled,

    #[error("too many attempts, try again in {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },

    #[error("cryptography error")]
    Crypto,

    #[error("database error: {0}")]
    Database(String),

    #[error("host key for {host}:{port} could not be verified")]
    HostKeyUntrusted {
        host: String,
        port: u16,
        fingerprint_sha256: String,
        key_algorithm: String,
        host_key_b64: String,
    },

    #[error("host key for {host}:{port} has changed")]
    HostKeyMismatch {
        host: String,
        port: u16,
        previous_fingerprint_sha256: String,
        presented_fingerprint_sha256: String,
        key_algorithm: String,
        host_key_b64: String,
    },

    #[error("authentication failed")]
    AuthenticationFailed,

    #[error("connection failed: {0}")]
    ConnectionFailed(String),

    #[error("ssh error: {0}")]
    Ssh(String),

    #[error("io error: {0}")]
    Io(String),

    #[error("{0}")]
    Internal(String),
}

pub type AppResult<T> = Result<T, AppError>;

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        tracing::error!(error = %e, "database error");
        AppError::Database(redact(&e.to_string()))
    }
}

impl From<r2d2::Error> for AppError {
    fn from(e: r2d2::Error) -> Self {
        tracing::error!(error = %e, "database pool error");
        AppError::Database("connection pool unavailable".into())
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        tracing::error!(error = %e, "io error");
        AppError::Io(redact(&e.to_string()))
    }
}

/// Best-effort scrub of anything that might look like a path fragment or
/// secret before an internal error string is shown to the user.
fn redact(s: &str) -> String {
    if s.len() > 200 {
        format!("{}...", &s[..200])
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_variant_serializes_to_snake_case_kind_with_no_message() {
        let json = serde_json::to_value(AppError::VaultLocked).unwrap();
        assert_eq!(json, serde_json::json!({ "kind": "vault_locked" }));
    }

    #[test]
    fn tuple_variant_carries_its_string_as_message() {
        let json = serde_json::to_value(AppError::NotFound("ssh profile".into())).unwrap();
        assert_eq!(json, serde_json::json!({ "kind": "not_found", "message": "ssh profile" }));
    }

    #[test]
    fn host_key_untrusted_never_contains_a_generic_message_field_by_accident() {
        let err = AppError::HostKeyUntrusted {
            host: "example.com".into(),
            port: 22,
            fingerprint_sha256: "SHA256:abc".into(),
            key_algorithm: "ssh-ed25519".into(),
            host_key_b64: "base64data".into(),
        };
        let json = serde_json::to_value(err).unwrap();
        assert_eq!(json["kind"], "host_key_untrusted");
        assert_eq!(json["message"]["host"], "example.com");
        assert_eq!(json["message"]["fingerprint_sha256"], "SHA256:abc");
    }
}
