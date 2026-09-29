use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthKind {
    Password,
    PrivateKey,
    PrivateKeyPassphrase,
}

impl AuthKind {
    pub fn as_db_str(&self) -> &'static str {
        match self {
            AuthKind::Password => "password",
            AuthKind::PrivateKey => "private_key",
            AuthKind::PrivateKeyPassphrase => "private_key_passphrase",
        }
    }

    pub fn from_db_str(s: &str) -> Option<Self> {
        match s {
            "password" => Some(AuthKind::Password),
            "private_key" => Some(AuthKind::PrivateKey),
            "private_key_passphrase" => Some(AuthKind::PrivateKeyPassphrase),
            _ => None,
        }
    }
}

/// A saved SSH connection definition. Never carries secret material -
/// `has_secret` / `has_passphrase` tell the UI a credential is stored
/// without exposing it.
#[derive(Debug, Clone, Serialize)]
pub struct SshProfile {
    pub id: String,
    pub terminal_group_id: Option<String>,
    pub name: String,
    pub hostname: String,
    pub port: u16,
    pub username: String,
    pub authentication_kind: AuthKind,
    pub has_secret: bool,
    pub has_passphrase: bool,
    pub terminal_type: String,
    pub sort_order: i64,
    pub last_status: Option<String>,
    pub last_connected_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Secret material supplied only at create/update time. Dropped from
/// memory (via `Drop`/zeroize in the vault layer) as soon as it is
/// encrypted; never echoed back to the frontend.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuthSecretInput {
    Password {
        password: String,
    },
    PrivateKey {
        private_key: String,
    },
    PrivateKeyPassphrase {
        private_key: String,
        passphrase: String,
    },
    /// Keep the existing stored secret unchanged (used on profile edit).
    Unchanged,
    /// Remove any stored secret.
    Clear,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewSshProfileInput {
    pub terminal_group_id: Option<String>,
    pub name: String,
    pub hostname: String,
    pub port: u16,
    pub username: String,
    pub terminal_type: Option<String>,
    pub secret: AuthSecretInput,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateSshProfileInput {
    pub id: String,
    pub terminal_group_id: Option<String>,
    pub name: String,
    pub hostname: String,
    pub port: u16,
    pub username: String,
    pub terminal_type: Option<String>,
    pub secret: AuthSecretInput,
}
