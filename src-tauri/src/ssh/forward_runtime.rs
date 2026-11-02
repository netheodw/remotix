use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::time::Duration;

use ssh2::{Channel, Session};

use crate::db::Pool;
use crate::domain::{ForwardRuntimeStatus, ForwardType};
use crate::error::AppError;
use crate::repositories::{ForwardRepository, ProfileRepository};
use crate::ssh::session::{self, ConnectStage};
use crate::ssh::socks5;
use crate::vault::VaultManager;

const TICK: Duration = Duration::from_millis(20);
const MAX_BUFFERED_BYTES: usize = 4 * 1024 * 1024;
const MAX_BACKOFF_SECS: u64 = 60;

pub enum ForwardControl {
    Stop,
}

pub struct ForwardHandle {
    tx: mpsc::Sender<ForwardControl>,
}

impl ForwardHandle {
    pub fn stop(&self) {
        let _ = self.tx.send(ForwardControl::Stop);
    }
}

struct Pair {
    stream: TcpStream,
    channel: Channel,
    to_channel: Vec<u8>,
    to_stream: Vec<u8>,
}

enum BridgeExit {
    Stopped,
    SessionError(String),
}

pub fn spawn_forward(
    pool: Pool,
    vault: std::sync::Arc<VaultManager>,
    forward_id: String,
    emit: impl Fn(ForwardRuntimeStatus) + Send + 'static,
) -> ForwardHandle {
    let (tx, rx) = mpsc::channel::<ForwardControl>();
    std::thread::spawn(move || run_forward(pool, vault, forward_id, rx, emit));
    ForwardHandle { tx }
}

fn run_forward(
    pool: Pool,
    vault: std::sync::Arc<VaultManager>,
    forward_id: String,
    rx: mpsc::Receiver<ForwardControl>,
    emit: impl Fn(ForwardRuntimeStatus),
) {
    let mut attempt: u32 = 0;

    loop {
        if matches!(rx.try_recv(), Ok(ForwardControl::Stop)) {
            emit(ForwardRuntimeStatus::Stopped);
            return;
        }

        emit(ForwardRuntimeStatus::Starting);
        let conn = match pool.get() {
            Ok(c) => c,
            Err(e) => {
                emit(ForwardRuntimeStatus::Failed { message: e.to_string() });
                return;
            }
        };
        let forward = match ForwardRepository::get(&conn, &forward_id) {
            Ok(f) => f,
            Err(e) => {
                emit(ForwardRuntimeStatus::Failed { message: e.to_string() });
                return;
            }
        };
        let Some(profile_id) = forward.ssh_profile_id.clone() else {
            emit(ForwardRuntimeStatus::NeedsProfile);
            return;
        };
        let profile = match ProfileRepository::get(&conn, &profile_id) {
            Ok(p) => p,
            Err(_) => {
                emit(ForwardRuntimeStatus::NeedsProfile);
                return;
            }
        };
        let auth = match ProfileRepository::load_auth_material(&conn, &vault, &profile_id) {
            Ok(a) => a,
            Err(e) => {
                emit(ForwardRuntimeStatus::Failed { message: e.to_string() });
                return;
            }
        };

        let on_stage = |stage: ConnectStage| {
            emit(match stage {
                ConnectStage::Connecting => ForwardRuntimeStatus::Starting,
                ConnectStage::VerifyingHost => ForwardRuntimeStatus::VerifyingHost,
                ConnectStage::Authenticating => ForwardRuntimeStatus::Authenticating,
            });
        };

        let connect_result =
            session::connect_verified(&conn, &profile.hostname, profile.port, &profile.username, &auth, on_stage);
        drop(conn);

        let session = match connect_result {
            Ok(s) => s,
            Err(e @ AppError::HostKeyUntrusted { .. })
            | Err(e @ AppError::HostKeyMismatch { .. })
            | Err(e @ AppError::AuthenticationFailed) => {
                // Never auto-retry a security decision or bad credentials.
                emit(ForwardRuntimeStatus::Failed { message: e.to_string() });
                return;
            }
            Err(e) => {
                attempt += 1;
                let backoff = backoff_secs(attempt);
                emit(ForwardRuntimeStatus::Reconnecting { attempt, next_retry_secs: backoff as u32 });
                if wait_or_stop(&rx, Duration::from_secs(backoff)) {
                    emit(ForwardRuntimeStatus::Stopped);
                    return;
                }
                tracing::warn!(error = %e, forward_id = %forward_id, "forward connect attempt failed");
                continue;
            }
        };

        attempt = 0;
        session.set_blocking(false);
        emit(ForwardRuntimeStatus::Active { connection_count: 0 });

        let exit = match forward.forward_type {
            ForwardType::Local => bridge_local(&session, &forward, &rx, &emit),
            ForwardType::Remote => bridge_remote(&session, &forward, &rx, &emit),
            ForwardType::Dynamic => bridge_dynamic(&session, &forward, &rx, &emit),
        };

        match exit {
            BridgeExit::Stopped => {
                emit(ForwardRuntimeStatus::Stopped);
                return;
            }
            BridgeExit::SessionError(message) => {
                attempt += 1;
                let backoff = backoff_secs(attempt);
                emit(ForwardRuntimeStatus::Reconnecting { attempt, next_retry_secs: backoff as u32 });
                tracing::warn!(error = %message, forward_id = %forward_id, "forward session dropped");
                if wait_or_stop(&rx, Duration::from_secs(backoff)) {
                    emit(ForwardRuntimeStatus::Stopped);
                    return;
                }
            }
        }
    }
}

fn backoff_secs(attempt: u32) -> u64 {
    2u64.saturating_pow(attempt.min(6)).min(MAX_BACKOFF_SECS)
}

/// Sleeps up to `d`, returning early (with `true`) if a stop was requested.
fn wait_or_stop(rx: &mpsc::Receiver<ForwardControl>, d: Duration) -> bool {
    let step = Duration::from_millis(100);
    let mut waited = Duration::ZERO;
    while waited < d {
        if matches!(rx.try_recv(), Ok(ForwardControl::Stop)) {
            return true;
        }
        std::thread::sleep(step);
        waited += step;
    }
    false
}

fn is_stop_requested(rx: &mpsc::Receiver<ForwardControl>) -> bool {
    matches!(rx.try_recv(), Ok(ForwardControl::Stop))
}

fn bridge_local(
    session: &Session,
    forward: &crate::domain::PortForward,
    rx: &mpsc::Receiver<ForwardControl>,
    emit: &impl Fn(ForwardRuntimeStatus),
) -> BridgeExit {
    let listener = match TcpListener::bind((forward.bind_address.as_str(), forward.listen_port)) {
        Ok(l) => l,
        Err(e) => return BridgeExit::SessionError(format!("could not bind {}:{}: {e}", forward.bind_address, forward.listen_port)),
    };
    if listener.set_nonblocking(true).is_err() {
        return BridgeExit::SessionError("could not configure listener".into());
    }

    let socket_path = forward.target_socket_path().map(str::to_string);
    let target_host = forward.target_hostname.clone().unwrap_or_default();
    let target_port = forward.target_port.unwrap_or(0);

    let mut pairs: Vec<Pair> = Vec::new();
    loop {
        if is_stop_requested(rx) {
            return BridgeExit::Stopped;
        }

        match listener.accept() {
            Ok((stream, _addr)) => {
                let _ = stream.set_nonblocking(true);
                let opened = match &socket_path {
                    Some(path) => open_direct_streamlocal(session, path),
                    None => open_direct_tcpip(session, &target_host, target_port),
                };
                match opened {
                    Ok(channel) => pairs.push(Pair { stream, channel, to_channel: Vec::new(), to_stream: Vec::new() }),
                    Err(e) => tracing::warn!(error = %e, "local forward channel open failed"),
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => return BridgeExit::SessionError(e.to_string()),
        }

        pairs.retain_mut(|p| service_pair(p));
        emit(ForwardRuntimeStatus::Active { connection_count: pairs.len() as u32 });
        std::thread::sleep(TICK);
    }
}

fn bridge_remote(
    session: &Session,
    forward: &crate::domain::PortForward,
    rx: &mpsc::Receiver<ForwardControl>,
    emit: &impl Fn(ForwardRuntimeStatus),
) -> BridgeExit {
    let (mut listener, _bound_port) = match session.channel_forward_listen(forward.listen_port, Some(&forward.bind_address), None) {
        Ok(v) => v,
        Err(e) => return BridgeExit::SessionError(format!("remote listen failed: {e}")),
    };

    let target_host = forward.target_hostname.clone().unwrap_or_else(|| "127.0.0.1".to_string());
    let target_port = forward.target_port.unwrap_or(0);

    let mut pairs: Vec<Pair> = Vec::new();
    loop {
        if is_stop_requested(rx) {
            return BridgeExit::Stopped;
        }

        match listener.accept() {
            Ok(channel) => match TcpStream::connect((target_host.as_str(), target_port)) {
                Ok(stream) => {
                    let _ = stream.set_nonblocking(true);
                    pairs.push(Pair { stream, channel, to_channel: Vec::new(), to_stream: Vec::new() });
                }
                Err(e) => tracing::warn!(error = %e, "local target connect failed for remote forward"),
            },
            Err(e) if ssh_would_block(&e) => {}
            Err(e) => return BridgeExit::SessionError(e.to_string()),
        }

        pairs.retain_mut(|p| service_pair(p));
        emit(ForwardRuntimeStatus::Active { connection_count: pairs.len() as u32 });
        std::thread::sleep(TICK);
    }
}

fn bridge_dynamic(
    session: &Session,
    forward: &crate::domain::PortForward,
    rx: &mpsc::Receiver<ForwardControl>,
    emit: &impl Fn(ForwardRuntimeStatus),
) -> BridgeExit {
    let listener = match TcpListener::bind((forward.bind_address.as_str(), forward.listen_port)) {
        Ok(l) => l,
        Err(e) => return BridgeExit::SessionError(format!("could not bind {}:{}: {e}", forward.bind_address, forward.listen_port)),
    };
    if listener.set_nonblocking(true).is_err() {
        return BridgeExit::SessionError("could not configure listener".into());
    }

    // Blocking SOCKS5 handshakes happen on short-lived helper threads so a
    // slow/stalled client can't block servicing of already-open tunnels;
    // completed handshakes are handed back for the main loop to pair with
    // a direct-tcpip channel (channel opens must stay on this thread).
    let (handshake_tx, handshake_rx) = mpsc::channel::<socks5::HandshakeResult>();

    let mut pairs: Vec<Pair> = Vec::new();
    loop {
        if is_stop_requested(rx) {
            return BridgeExit::Stopped;
        }

        match listener.accept() {
            Ok((stream, _addr)) => {
                let tx = handshake_tx.clone();
                std::thread::spawn(move || socks5::negotiate(stream, tx));
            }
            Err(e) if would_block(&e) => {}
            Err(e) => return BridgeExit::SessionError(e.to_string()),
        }

        while let Ok(result) = handshake_rx.try_recv() {
            match result {
                socks5::HandshakeResult::Ready { stream, host, port } => {
                    let _ = stream.set_nonblocking(true);
                    match open_direct_tcpip(session, &host, port) {
                        Ok(channel) => pairs.push(Pair { stream, channel, to_channel: Vec::new(), to_stream: Vec::new() }),
                        Err(e) => tracing::warn!(error = %e, "direct-tcpip channel open failed for socks client"),
                    }
                }
                socks5::HandshakeResult::Failed => {}
            }
        }

        pairs.retain_mut(|p| service_pair(p));
        emit(ForwardRuntimeStatus::Active { connection_count: pairs.len() as u32 });
        std::thread::sleep(TICK);
    }
}

fn would_block(e: &std::io::Error) -> bool {
    e.kind() == std::io::ErrorKind::WouldBlock
}

/// LIBSSH2_ERROR_EAGAIN: the non-blocking-mode equivalent of `WouldBlock`
/// for calls that return `ssh2::Error` directly rather than `io::Error`
/// (channel/listener setup calls, as opposed to `Read`/`Write`).
fn ssh_would_block(e: &ssh2::Error) -> bool {
    matches!(e.code(), ssh2::ErrorCode::Session(-37))
}

/// Opens a direct-tcpip channel, retrying briefly on EAGAIN since the
/// session is non-blocking here. Channel setup must happen on this
/// forward's single session-owning thread.
fn open_direct_tcpip(session: &Session, host: &str, port: u16) -> Result<Channel, ssh2::Error> {
    let mut attempts = 0;
    loop {
        match session.channel_direct_tcpip(host, port, None) {
            Ok(channel) => return Ok(channel),
            Err(e) if ssh_would_block(&e) && attempts < 200 => {
                attempts += 1;
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(e) => return Err(e),
        }
    }
}

/// Opens a direct-streamlocal@openssh.com channel to a Unix socket on the
/// server (`ssh -L port:/path.sock`), with the same EAGAIN retry as
/// `open_direct_tcpip`.
fn open_direct_streamlocal(session: &Session, socket_path: &str) -> Result<Channel, ssh2::Error> {
    let mut attempts = 0;
    loop {
        match session.channel_direct_streamlocal(socket_path, None) {
            Ok(channel) => return Ok(channel),
            Err(e) if ssh_would_block(&e) && attempts < 200 => {
                attempts += 1;
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(e) => return Err(e),
        }
    }
}

/// Services one (local TCP, ssh channel) pair for a single tick. Returns
/// `false` when the pair should be dropped (either side closed or errored).
fn service_pair(pair: &mut Pair) -> bool {
    if !pair.to_channel.is_empty() {
        match pair.channel.write(&pair.to_channel) {
            Ok(n) => {
                pair.to_channel.drain(0..n);
            }
            Err(e) if would_block(&e) => {}
            Err(_) => return false,
        }
    }
    if pair.to_channel.len() < MAX_BUFFERED_BYTES {
        let mut buf = [0u8; 16 * 1024];
        match pair.stream.read(&mut buf) {
            Ok(0) => return false,
            Ok(n) => pair.to_channel.extend_from_slice(&buf[..n]),
            Err(e) if would_block(&e) => {}
            Err(_) => return false,
        }
    }

    if !pair.to_stream.is_empty() {
        match pair.stream.write(&pair.to_stream) {
            Ok(n) => {
                pair.to_stream.drain(0..n);
            }
            Err(e) if would_block(&e) => {}
            Err(_) => return false,
        }
    }
    if pair.to_stream.len() < MAX_BUFFERED_BYTES {
        let mut buf = [0u8; 16 * 1024];
        match pair.channel.read(&mut buf) {
            Ok(0) => {
                if pair.channel.eof() {
                    return false;
                }
            }
            Ok(n) => pair.to_stream.extend_from_slice(&buf[..n]),
            Err(e) if would_block(&e) => {}
            Err(_) => return false,
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_and_is_bounded() {
        assert_eq!(backoff_secs(1), 2);
        assert_eq!(backoff_secs(2), 4);
        assert_eq!(backoff_secs(6), 64.min(MAX_BACKOFF_SECS));
        assert_eq!(backoff_secs(20), MAX_BACKOFF_SECS);
    }

    #[test]
    fn wait_or_stop_returns_true_when_stop_requested_immediately() {
        let (tx, rx) = mpsc::channel();
        tx.send(ForwardControl::Stop).unwrap();
        assert!(wait_or_stop(&rx, Duration::from_secs(5)));
    }

    #[test]
    fn wait_or_stop_returns_false_after_elapsing_with_no_stop() {
        let (_tx, rx) = mpsc::channel::<ForwardControl>();
        assert!(!wait_or_stop(&rx, Duration::from_millis(150)));
    }
}
