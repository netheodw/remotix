use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct KnownHost {
    pub id: String,
    pub hostname: String,
    pub port: u16,
    pub key_algorithm: String,
    pub sha256_fingerprint: String,
    pub first_trusted_at: String,
    pub last_seen_at: String,
}

/// Outcome of checking a freshly-presented host key against the local store.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status")]
pub enum HostKeyCheck {
    /// Matches the stored key for this host/port.
    Trusted,
    /// No stored key yet; caller must ask the user to trust or reject.
    FirstUse {
        key_algorithm: String,
        sha256_fingerprint: String,
    },
    /// A different key is stored for this host/port; caller must block by
    /// default and require an explicit "replace" decision.
    Changed {
        key_algorithm: String,
        previous_sha256_fingerprint: String,
        presented_sha256_fingerprint: String,
    },
}
