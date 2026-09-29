use std::collections::HashMap;
use std::sync::{mpsc, Arc, Mutex};

use crate::db::Pool;
use crate::domain::ForwardRuntimeStatus;
use crate::error::{AppError, AppResult};
use crate::sftp::session::{ControlRequest, SftpSessionHandle};
use crate::sftp::transfer::TransferCommand;
use crate::ssh::forward_runtime::{self, ForwardHandle};
use crate::ssh::terminal::{self, TerminalEvent, TerminalHandle};
use crate::vault::VaultManager;

/// Registry of every terminal session and port forward currently running.
/// Owned once as app state; every entry point (open/close a terminal,
/// start/stop a forward, lock the vault) goes through here so the set of
/// "what is actually live" always matches what the UI shows.
#[derive(Default)]
pub struct SshManager {
    terminals: Mutex<HashMap<String, TerminalHandle>>,
    forwards: Mutex<HashMap<String, ForwardHandle>>,
    sftp: Mutex<HashMap<String, SftpSessionHandle>>,
}

impl SshManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn open_terminal(
        &self,
        id: String,
        pool: Pool,
        vault: Arc<VaultManager>,
        profile_id: String,
        cols: u32,
        rows: u32,
        emit: impl Fn(TerminalEvent) + Send + 'static,
    ) {
        let handle = terminal::spawn_terminal(pool, vault, profile_id, cols, rows, emit);
        self.terminals.lock().unwrap().insert(id, handle);
    }

    pub fn terminal_input(&self, id: &str, data: Vec<u8>) {
        if let Some(h) = self.terminals.lock().unwrap().get(id) {
            let _ = h.send_input(data);
        }
    }

    pub fn terminal_resize(&self, id: &str, cols: u32, rows: u32) {
        if let Some(h) = self.terminals.lock().unwrap().get(id) {
            let _ = h.resize(cols, rows);
        }
    }

    pub fn close_terminal(&self, id: &str) {
        if let Some(h) = self.terminals.lock().unwrap().remove(id) {
            let _ = h.disconnect();
        }
    }

    pub fn start_forward(
        &self,
        id: String,
        pool: Pool,
        vault: Arc<VaultManager>,
        emit: impl Fn(ForwardRuntimeStatus) + Send + 'static,
    ) {
        let mut forwards = self.forwards.lock().unwrap();
        if forwards.contains_key(&id) {
            return;
        }
        let handle = forward_runtime::spawn_forward(pool, vault, id.clone(), emit);
        forwards.insert(id, handle);
    }

    pub fn stop_forward(&self, id: &str) {
        if let Some(h) = self.forwards.lock().unwrap().remove(id) {
            h.stop();
        }
    }

    pub fn is_forward_running(&self, id: &str) -> bool {
        self.forwards.lock().unwrap().contains_key(id)
    }

    pub fn register_sftp(&self, id: String, handle: SftpSessionHandle) {
        self.sftp.lock().unwrap().insert(id, handle);
    }

    /// Clones the sender and releases the lock, so callers can await a reply
    /// without holding a `MutexGuard` across the await.
    pub fn sftp_control(&self, id: &str) -> AppResult<mpsc::Sender<ControlRequest>> {
        self.sftp
            .lock()
            .unwrap()
            .get(id)
            .map(|h| h.control())
            .ok_or_else(|| AppError::NotFound("sftp session".into()))
    }

    pub fn sftp_transfers(&self, id: &str) -> AppResult<mpsc::Sender<TransferCommand>> {
        self.sftp
            .lock()
            .unwrap()
            .get(id)
            .map(|h| h.transfers())
            .ok_or_else(|| AppError::NotFound("sftp session".into()))
    }

    pub fn close_sftp(&self, id: &str) {
        if let Some(h) = self.sftp.lock().unwrap().remove(id) {
            h.shutdown();
        }
    }

    /// Disconnects every live terminal and stops every running forward.
    /// Called when the vault is locked or the app is exiting.
    pub fn stop_all(&self) {
        for (_, h) in self.terminals.lock().unwrap().drain() {
            let _ = h.disconnect();
        }
        for (_, h) in self.forwards.lock().unwrap().drain() {
            h.stop();
        }
        for (_, h) in self.sftp.lock().unwrap().drain() {
            h.shutdown();
        }
    }
}
