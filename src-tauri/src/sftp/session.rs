//! The control side of an SFTP session: one OS thread owning a blocking
//! `ssh2::Session` plus its `Sftp`, answering browse requests over an mpsc
//! channel.
//!
//! Transfers deliberately do *not* share this session. Every call derived from
//! one `ssh2::Session` takes the same internal mutex for the whole duration of
//! a libssh2 call, so a directory listing queued behind a running transfer
//! would wait not one chunk but one whole libssh2 call - bounded only by the
//! 30s timeout on a stalled link. Transfers get their own connection (see
//! [`super::transfer`]); this is the same split WinSCP and FileZilla use.
//!
//! If the second authentication ever becomes a problem (a 2FA prompt, a server
//! limiting concurrent sessions), the transfer worker can be collapsed onto
//! this one by draining its command channel between chunks. Only the borrowed
//! `Session` changes, not the shape of the code.

use std::path::Path;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use ssh2::{ErrorCode, FileStat, Session, Sftp};
use tokio::sync::oneshot;

use super::transfer::{self, TransferCommand, TransferEvent};
use super::{guard, path, EntryKind};
use crate::db::Pool;
use crate::error::{AppError, AppResult};
use crate::repositories::ProfileRepository;
use crate::ssh::session as ssh_session;
use crate::vault::VaultManager;

/// `libssh2_sftp_readdir_ex` reports "no more files" as this session error
/// rather than an empty read. `ssh2` does not re-export `libssh2-sys`, so the
/// constant is repeated here.
pub(super) const LIBSSH2_ERROR_FILE: i32 = -16;

const OPERATION_TIMEOUT_MS: u32 = 30_000;
const KEEPALIVE_INTERVAL_SECS: u32 = 30;
/// How long a worker waits for a request before sending a keepalive. libssh2
/// only emits keepalives when asked, so without this an idle session dies
/// behind NAT.
pub(super) const IDLE_TICK: Duration = Duration::from_secs(20);

const MAX_LISTING_ENTRIES: usize = 10_000;
/// Each symlink costs an extra round trip to resolve, so stop after this many.
const MAX_SYMLINK_RESOLUTIONS: usize = 200;

#[derive(Debug, Clone, Serialize)]
pub struct RemoteEntry {
    pub name: String,
    pub path: String,
    pub kind: EntryKind,
    pub resolved_kind: Option<EntryKind>,
    pub size: u64,
    pub modified: Option<i64>,
    pub permissions: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RemoteListing {
    pub path: String,
    pub parent: Option<String>,
    pub entries: Vec<RemoteEntry>,
    pub truncated: bool,
}

pub enum ControlRequest {
    List {
        path: String,
        reply: oneshot::Sender<AppResult<RemoteListing>>,
    },
    /// Fetches one file to a local path. Only used to open a remote file in an
    /// editor, so it runs on the control session rather than the transfer queue
    /// - it must finish before the editor can be launched, and the size cap
    /// keeps it short enough not to stall browsing.
    Fetch {
        remote: String,
        local: std::path::PathBuf,
        reply: oneshot::Sender<AppResult<()>>,
    },
    Remove {
        path: String,
        reply: oneshot::Sender<AppResult<()>>,
    },
    Disconnect,
}

pub struct SftpSessionHandle {
    control: mpsc::Sender<ControlRequest>,
    transfers: mpsc::Sender<TransferCommand>,
}

impl SftpSessionHandle {
    pub fn control(&self) -> mpsc::Sender<ControlRequest> {
        self.control.clone()
    }

    pub fn transfers(&self) -> mpsc::Sender<TransferCommand> {
        self.transfers.clone()
    }

    pub fn shutdown(&self) {
        let _ = self.control.send(ControlRequest::Disconnect);
        let _ = self.transfers.send(TransferCommand::Disconnect);
    }
}

pub struct Connected {
    pub handle: SftpSessionHandle,
    pub home: String,
}

/// Field order matters: `sftp` must drop before `session`.
pub(super) struct SftpConn {
    pub sftp: Sftp,
    pub session: Session,
}

/// Opens a fresh authenticated SFTP connection for `profile_id`. Used once for
/// the control session and again, lazily, the first time a transfer is queued.
pub(super) fn open_connection(
    pool: &Pool,
    vault: &Arc<VaultManager>,
    profile_id: &str,
) -> AppResult<SftpConn> {
    let conn = pool.get()?;
    let profile = ProfileRepository::get(&conn, profile_id)?;
    let auth = ProfileRepository::load_auth_material(&conn, vault, profile_id)?;

    let session = ssh_session::connect_verified(
        &conn,
        &profile.hostname,
        profile.port,
        &profile.username,
        &auth,
        |_| {},
    )?;

    // Blocking mode on purpose - the opposite of the terminal and forward
    // workers. Non-blocking makes `readdir` busy-spin at 100% CPU on EAGAIN and
    // turns every metadata call into a hand-rolled retry, and `set_timeout`
    // only has an effect while blocking.
    session.set_timeout(OPERATION_TIMEOUT_MS);
    session.set_keepalive(true, KEEPALIVE_INTERVAL_SECS);
    let sftp = session.sftp().map_err(map_ssh)?;

    Ok(SftpConn { sftp, session })
}

/// Connects, authenticates and resolves the remote home *before* handing back a
/// handle, so a first-use host key or a bad credential surfaces as an error the
/// caller can act on rather than disappearing into an event stream.
pub fn connect(
    pool: Pool,
    vault: Arc<VaultManager>,
    profile_id: String,
    emit: impl Fn(TransferEvent) + Send + 'static,
) -> AppResult<Connected> {
    let conn = match open_connection(&pool, &vault, &profile_id) {
        Ok(conn) => conn,
        Err(e) => {
            if let Ok(db) = pool.get() {
                let _ = ProfileRepository::record_connection_result(&db, &profile_id, "failed");
            }
            return Err(e);
        }
    };

    let home = guard("resolving the home directory", || realpath(&conn.sftp, "."))?;
    if let Ok(db) = pool.get() {
        let _ = ProfileRepository::record_connection_result(&db, &profile_id, "connected");
    }

    let (control_tx, control_rx) = mpsc::channel();
    std::thread::spawn(move || run_control(conn, control_rx));

    let transfers = transfer::spawn(pool, vault, profile_id, emit);

    Ok(Connected {
        handle: SftpSessionHandle {
            control: control_tx,
            transfers,
        },
        home,
    })
}

fn run_control(conn: SftpConn, rx: mpsc::Receiver<ControlRequest>) {
    loop {
        match rx.recv_timeout(IDLE_TICK) {
            Ok(ControlRequest::List { path, reply }) => {
                let result = guard("listing the directory", || list_dir(&conn.sftp, &path));
                let _ = reply.send(result);
            }
            Ok(ControlRequest::Fetch { remote, local, reply }) => {
                let result = guard("opening the file", || fetch_file(&conn.sftp, &remote, &local));
                let _ = reply.send(result);
            }
            Ok(ControlRequest::Remove { path, reply }) => {
                let result = guard("deleting", || remove(&conn.sftp, &path));
                let _ = reply.send(result);
            }
            Ok(ControlRequest::Disconnect) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {
                if conn.session.keepalive_send().is_err() {
                    break;
                }
            }
        }
    }
    let _ = conn.session.disconnect(None, "closed by user", None);
}

fn list_dir(sftp: &Sftp, path: &str) -> AppResult<RemoteListing> {
    let canonical = realpath(sftp, path)?;
    let mut dir = sftp.opendir(Path::new(&canonical)).map_err(map_ssh)?;

    let mut entries = Vec::new();
    let mut truncated = false;
    let mut resolutions = 0usize;

    loop {
        let (name, stat) = match dir.readdir() {
            Ok(item) => item,
            Err(e) if e.code() == ErrorCode::Session(LIBSSH2_ERROR_FILE) => break,
            Err(e) => return Err(map_ssh(e)),
        };

        // `File::readdir` yields the bare filename and includes `.` and `..`.
        // The `Sftp::readdir` convenience wrapper would join these with
        // `Path::join`, which on Windows produces `/home/user\file.txt` - hence
        // the manual walk and `path::join`.
        let name = name.to_string_lossy().to_string();
        if name == "." || name == ".." {
            continue;
        }
        if entries.len() >= MAX_LISTING_ENTRIES {
            truncated = true;
            break;
        }

        let full = path::join(&canonical, &name);
        let kind = kind_of(&stat);
        let resolved_kind = if kind == EntryKind::Symlink && resolutions < MAX_SYMLINK_RESOLUTIONS {
            resolutions += 1;
            sftp.stat(Path::new(&full)).ok().map(|s| kind_of(&s))
        } else {
            None
        };

        entries.push(RemoteEntry {
            name,
            path: full,
            kind,
            resolved_kind,
            size: stat.size.unwrap_or(0),
            modified: stat.mtime.and_then(|t| i64::try_from(t).ok()),
            permissions: stat.perm,
        });
    }

    Ok(RemoteListing {
        parent: path::parent_of(&canonical),
        path: canonical,
        entries,
        truncated,
    })
}

/// Guards a recursive delete against a server that answers `readdir` with a
/// tree that never bottoms out.
const MAX_DELETE_DEPTH: usize = 64;

/// Deletes a file, or a directory and everything under it.
///
/// Depth-first, because SFTP `rmdir` requires the directory to already be
/// empty. Symlinks are unlinked rather than followed - deleting through a link
/// would destroy files outside the tree the user pointed at.
fn remove(sftp: &Sftp, target: &str) -> AppResult<()> {
    let stat = sftp.lstat(Path::new(target)).map_err(map_ssh)?;
    if kind_of(&stat) != EntryKind::Directory {
        return sftp.unlink(Path::new(target)).map_err(map_ssh);
    }
    remove_dir(sftp, target, 0)
}

fn remove_dir(sftp: &Sftp, dir: &str, depth: usize) -> AppResult<()> {
    if depth >= MAX_DELETE_DEPTH {
        return Err(AppError::Validation(format!(
            "directory nests deeper than {MAX_DELETE_DEPTH} levels"
        )));
    }

    let mut handle = sftp.opendir(Path::new(dir)).map_err(map_ssh)?;
    let mut children = Vec::new();
    loop {
        let (name, stat) = match handle.readdir() {
            Ok(item) => item,
            Err(e) if e.code() == ErrorCode::Session(LIBSSH2_ERROR_FILE) => break,
            Err(e) => return Err(map_ssh(e)),
        };
        let name = name.to_string_lossy().to_string();
        if name == "." || name == ".." {
            continue;
        }
        // A server answering with `../x` must not steer a delete out of the
        // directory the user chose.
        path::check_remote_segment(&name)?;
        children.push((path::join(dir, &name), kind_of(&stat)));
    }
    drop(handle);

    for (child, kind) in children {
        match kind {
            EntryKind::Directory => remove_dir(sftp, &child, depth + 1)?,
            _ => sftp.unlink(Path::new(&child)).map_err(map_ssh)?,
        }
    }
    sftp.rmdir(Path::new(dir)).map_err(map_ssh)
}

/// Editing is for source and config files; the cap stops someone
/// double-clicking a database dump and freezing the browse session.
const MAX_EDIT_BYTES: u64 = 64 * 1024 * 1024;

fn fetch_file(sftp: &Sftp, remote: &str, local: &Path) -> AppResult<()> {
    let stat = sftp.stat(Path::new(remote)).map_err(map_ssh)?;
    if !stat.is_file() {
        return Err(AppError::Validation("only files can be opened".into()));
    }
    if stat.size.unwrap_or(0) > MAX_EDIT_BYTES {
        return Err(AppError::Validation(
            "file is too large to open for editing".into(),
        ));
    }

    if let Some(parent) = local.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut source = sftp.open(Path::new(remote)).map_err(map_ssh)?;
    let mut sink = std::fs::File::create(local)?;
    std::io::copy(&mut source, &mut sink)?;
    Ok(())
}

pub(super) fn realpath(sftp: &Sftp, path: &str) -> AppResult<String> {
    let resolved = sftp.realpath(Path::new(path)).map_err(map_ssh)?;
    // `path2bytes` already rewrote separators on the way out; normalise the way
    // back so a Windows client never shows a remote path with a backslash.
    Ok(resolved.to_string_lossy().replace('\\', "/"))
}

pub(super) fn kind_of(stat: &FileStat) -> EntryKind {
    let file_type = stat.file_type();
    if file_type.is_symlink() {
        EntryKind::Symlink
    } else if file_type.is_dir() {
        EntryKind::Directory
    } else if file_type.is_file() {
        EntryKind::File
    } else {
        EntryKind::Other
    }
}

pub(super) fn map_ssh(e: ssh2::Error) -> AppError {
    AppError::Ssh(e.message().to_string())
}
