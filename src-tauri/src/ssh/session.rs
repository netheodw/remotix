use std::net::TcpStream;
use std::time::Duration;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use ssh2::{HostKeyType, Session};

use crate::domain::HostKeyCheck;
use crate::error::{AppError, AppResult};
use crate::repositories::KnownHostRepository;

/// Secret material needed to authenticate. Never implements `Debug`/`Display`
/// so it can't accidentally end up in a log line.
pub enum AuthMaterial {
    Password(String),
    PrivateKey {
        key_pem: String,
        passphrase: Option<String>,
    },
}

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Copy)]
pub enum ConnectStage {
    Connecting,
    VerifyingHost,
    Authenticating,
}

fn key_algorithm_name(kind: HostKeyType) -> &'static str {
    match kind {
        HostKeyType::Rsa => "ssh-rsa",
        HostKeyType::Dss => "ssh-dss",
        HostKeyType::Ecdsa256 => "ecdsa-sha2-nistp256",
        HostKeyType::Ecdsa384 => "ecdsa-sha2-nistp384",
        HostKeyType::Ecdsa521 => "ecdsa-sha2-nistp521",
        HostKeyType::Ed25519 => "ssh-ed25519",
        HostKeyType::Unknown => "unknown",
    }
}

pub fn sha256_fingerprint(key_bytes: &[u8]) -> String {
    let digest = Sha256::digest(key_bytes);
    format!("SHA256:{}", BASE64.encode(digest))
}

/// Opens a TCP connection and completes the SSH handshake, returning the
/// live session plus the host key it presented. Does not authenticate and
/// does not consult the known-hosts store - callers decide trust.
pub fn handshake(hostname: &str, port: u16) -> AppResult<(Session, String, String, Vec<u8>)> {
    use std::net::ToSocketAddrs;
    let addr = format!("{hostname}:{port}");
    let socket_addrs: Vec<_> = addr
        .to_socket_addrs()
        .map_err(|e| AppError::ConnectionFailed(format!("cannot resolve {hostname}: {e}")))?
        .collect();
    let target = socket_addrs
        .first()
        .ok_or_else(|| AppError::ConnectionFailed(format!("cannot resolve {hostname}")))?;

    let tcp = TcpStream::connect_timeout(target, CONNECT_TIMEOUT)
        .map_err(|e| AppError::ConnectionFailed(format!("could not connect to {hostname}:{port}: {e}")))?;
    tcp.set_read_timeout(Some(Duration::from_secs(30))).ok();

    let mut session = Session::new().map_err(|e| AppError::Ssh(e.to_string()))?;
    session.set_tcp_stream(tcp);
    session
        .handshake()
        .map_err(|e| AppError::ConnectionFailed(format!("SSH handshake failed: {e}")))?;

    let (key_bytes, key_type) = session
        .host_key()
        .ok_or_else(|| AppError::Ssh("server presented no host key".into()))?;
    let algorithm = key_algorithm_name(key_type).to_string();
    let fingerprint = sha256_fingerprint(key_bytes);
    let key_bytes = key_bytes.to_vec();

    Ok((session, algorithm, fingerprint, key_bytes))
}

/// Full connect flow: handshake, verify the host key against the local
/// store, and authenticate. Returns a specific `HostKeyUntrusted` /
/// `HostKeyMismatch` error (never a generic failure) when the key needs a
/// user decision, so the caller can surface the trust dialog and retry.
pub fn connect_verified(
    conn: &Connection,
    hostname: &str,
    port: u16,
    username: &str,
    auth: &AuthMaterial,
    mut on_stage: impl FnMut(ConnectStage),
) -> AppResult<Session> {
    on_stage(ConnectStage::Connecting);
    let (session, algorithm, fingerprint, key_bytes) = handshake(hostname, port)?;

    on_stage(ConnectStage::VerifyingHost);
    match KnownHostRepository::check(conn, hostname, port, &algorithm, &fingerprint)? {
        HostKeyCheck::Trusted => {}
        HostKeyCheck::FirstUse { key_algorithm, sha256_fingerprint } => {
            return Err(AppError::HostKeyUntrusted {
                host: hostname.to_string(),
                port,
                fingerprint_sha256: sha256_fingerprint,
                key_algorithm,
                host_key_b64: BASE64.encode(key_bytes),
            });
        }
        HostKeyCheck::Changed { key_algorithm, previous_sha256_fingerprint, presented_sha256_fingerprint } => {
            return Err(AppError::HostKeyMismatch {
                host: hostname.to_string(),
                port,
                previous_fingerprint_sha256: previous_sha256_fingerprint,
                presented_fingerprint_sha256: presented_sha256_fingerprint,
                key_algorithm,
                host_key_b64: BASE64.encode(key_bytes),
            });
        }
    }

    on_stage(ConnectStage::Authenticating);
    authenticate(&session, username, auth)?;
    Ok(session)
}

pub fn authenticate(session: &Session, username: &str, auth: &AuthMaterial) -> AppResult<()> {
    match auth {
        AuthMaterial::Password(password) => {
            session
                .userauth_password(username, password)
                .map_err(|_| AppError::AuthenticationFailed)?;
        }
        AuthMaterial::PrivateKey { key_pem, passphrase } => {
            session
                .userauth_pubkey_memory(username, None, key_pem, passphrase.as_deref())
                .map_err(|_| AppError::AuthenticationFailed)?;
        }
    }
    if !session.authenticated() {
        return Err(AppError::AuthenticationFailed);
    }
    Ok(())
}
