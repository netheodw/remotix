use std::io::{Read, Write};
use std::sync::mpsc;
use std::time::Duration;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use serde::Serialize;

use crate::db::Pool;
use crate::repositories::ProfileRepository;
use crate::ssh::session::{self, ConnectStage};
use crate::vault::VaultManager;

const READ_BUF_SIZE: usize = 32 * 1024;
const POLL_INTERVAL: Duration = Duration::from_millis(15);
/// libssh2 only emits keepalives when asked, so without this an idle shell
/// dies behind NAT or a server-side idle timeout.
const KEEPALIVE_INTERVAL_SECS: u32 = 30;

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TerminalEvent {
    Status { state: String, message: Option<String> },
    Output { data_b64: String },
    Closed { reason: String },
}

pub enum TerminalCommand {
    Input(Vec<u8>),
    Resize { cols: u32, rows: u32 },
    Disconnect,
}

pub struct TerminalHandle {
    tx: mpsc::Sender<TerminalCommand>,
}

impl TerminalHandle {
    pub fn send_input(&self, data: Vec<u8>) -> Result<(), ()> {
        self.tx.send(TerminalCommand::Input(data)).map_err(|_| ())
    }

    pub fn resize(&self, cols: u32, rows: u32) -> Result<(), ()> {
        self.tx.send(TerminalCommand::Resize { cols, rows }).map_err(|_| ())
    }

    pub fn disconnect(&self) -> Result<(), ()> {
        self.tx.send(TerminalCommand::Disconnect).map_err(|_| ())
    }
}

fn stage_name(stage: ConnectStage) -> &'static str {
    match stage {
        ConnectStage::Connecting => "connecting",
        ConnectStage::VerifyingHost => "verifying_host",
        ConnectStage::Authenticating => "authenticating",
    }
}

/// Spawns a dedicated OS thread that owns the (blocking, non-`Sync`) ssh2
/// session for one terminal tab: connects, opens a PTY shell, then polls
/// for output and drains queued input/resize/disconnect commands until the
/// channel closes or the caller disconnects.
pub fn spawn_terminal(
    pool: Pool,
    vault: std::sync::Arc<VaultManager>,
    profile_id: String,
    initial_cols: u32,
    initial_rows: u32,
    emit: impl Fn(TerminalEvent) + Send + 'static,
) -> TerminalHandle {
    let (tx, rx) = mpsc::channel::<TerminalCommand>();
    std::thread::spawn(move || {
        run_terminal(pool, vault, profile_id, initial_cols, initial_rows, rx, emit);
    });
    TerminalHandle { tx }
}

fn run_terminal(
    pool: Pool,
    vault: std::sync::Arc<VaultManager>,
    profile_id: String,
    cols: u32,
    rows: u32,
    rx: mpsc::Receiver<TerminalCommand>,
    emit: impl Fn(TerminalEvent),
) {
    let conn = match pool.get() {
        Ok(c) => c,
        Err(e) => {
            emit(TerminalEvent::Closed { reason: format!("database unavailable: {e}") });
            return;
        }
    };

    let profile = match ProfileRepository::get(&conn, &profile_id) {
        Ok(p) => p,
        Err(e) => {
            emit(TerminalEvent::Closed { reason: e.to_string() });
            return;
        }
    };
    let auth = match ProfileRepository::load_auth_material(&conn, &vault, &profile_id) {
        Ok(a) => a,
        Err(e) => {
            emit(TerminalEvent::Closed { reason: e.to_string() });
            return;
        }
    };

    let on_stage = |stage: ConnectStage| {
        emit(TerminalEvent::Status { state: stage_name(stage).to_string(), message: None });
    };

    let session = match session::connect_verified(&conn, &profile.hostname, profile.port, &profile.username, &auth, on_stage) {
        Ok(s) => s,
        Err(e) => {
            let _ = ProfileRepository::record_connection_result(&conn, &profile_id, "failed");
            emit(TerminalEvent::Status { state: "failed".into(), message: Some(e.to_string()) });
            emit(TerminalEvent::Closed { reason: e.to_string() });
            return;
        }
    };
    let _ = ProfileRepository::record_connection_result(&conn, &profile_id, "connected");
    drop(conn);

    let mut channel = match session.channel_session() {
        Ok(c) => c,
        Err(e) => {
            emit(TerminalEvent::Closed { reason: format!("could not open channel: {e}") });
            return;
        }
    };
    if let Err(e) = channel.request_pty(&profile.terminal_type, None, Some((cols, rows, 0, 0))) {
        emit(TerminalEvent::Closed { reason: format!("could not request pty: {e}") });
        return;
    }
    if let Err(e) = channel.shell() {
        emit(TerminalEvent::Closed { reason: format!("could not start shell: {e}") });
        return;
    }
    session.set_keepalive(true, KEEPALIVE_INTERVAL_SECS);
    session.set_blocking(false);
    emit(TerminalEvent::Status { state: "connected".into(), message: None });

    let mut buf = [0u8; READ_BUF_SIZE];
    'outer: loop {
        match channel.read(&mut buf) {
            Ok(0) => {
                if channel.eof() {
                    break 'outer;
                }
            }
            Ok(n) => emit(TerminalEvent::Output { data_b64: BASE64.encode(&buf[..n]) }),
            Err(e) if would_block(&e) => {}
            Err(e) => {
                emit(TerminalEvent::Closed { reason: format!("read error: {e}") });
                break 'outer;
            }
        }

        loop {
            match rx.try_recv() {
                Ok(TerminalCommand::Input(data)) => {
                    if write_all_retrying(&mut channel, &data).is_err() {
                        emit(TerminalEvent::Closed { reason: "write error".into() });
                        break 'outer;
                    }
                }
                Ok(TerminalCommand::Resize { cols, rows }) => {
                    let _ = channel.request_pty_size(cols, rows, None, None);
                }
                Ok(TerminalCommand::Disconnect) => {
                    let _ = channel.close();
                    emit(TerminalEvent::Closed { reason: "disconnected".into() });
                    break 'outer;
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    let _ = channel.close();
                    emit(TerminalEvent::Closed { reason: "closed".into() });
                    break 'outer;
                }
            }
        }

        if channel.eof() {
            break 'outer;
        }
        // Cheap when not due: libssh2 only sends once the interval has passed.
        // Errors are ignored - a dead link surfaces as a read error or EOF.
        let _ = session.keepalive_send();
        std::thread::sleep(POLL_INTERVAL);
    }
}

fn would_block(err: &std::io::Error) -> bool {
    err.kind() == std::io::ErrorKind::WouldBlock
}

fn write_all_retrying(channel: &mut ssh2::Channel, data: &[u8]) -> std::io::Result<()> {
    let mut offset = 0;
    let mut stalls = 0;
    while offset < data.len() {
        match channel.write(&data[offset..]) {
            Ok(0) => {
                stalls += 1;
                if stalls > 500 {
                    return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "write stalled"));
                }
                std::thread::sleep(POLL_INTERVAL);
            }
            Ok(n) => {
                offset += n;
                stalls = 0;
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(POLL_INTERVAL);
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}
