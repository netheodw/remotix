//! The transfer side of an SFTP session: a worker thread with its own SSH
//! connection, a queue of jobs, and a chunk loop that stays responsive to
//! cancellation.
//!
//! The connection is opened lazily on the first enqueue, so someone who only
//! browses never pays for a second authentication.

use std::collections::{HashSet, VecDeque};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use ssh2::{ErrorCode, Sftp};

use super::session::{self, SftpConn, IDLE_TICK, LIBSSH2_ERROR_FILE};
use super::walk::{self, Entry};
use super::{guard, path, EntryKind};
use crate::db::Pool;
use crate::error::{AppError, AppResult};
use crate::vault::VaultManager;

const CHUNK: usize = 32 * 1024;
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);
const PROGRESS_BYTES: u64 = 1 << 20;
/// Above this many files, per-file completion events would flood the IPC
/// channel; the aggregate `Progress` already carries the file counter.
const MAX_FILE_DONE_EVENTS: usize = 500;

type Emit<'a> = &'a dyn Fn(TransferEvent);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Upload,
    Download,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictPolicy {
    Overwrite,
    Skip,
    KeepBoth,
}

pub struct TransferJob {
    pub item_id: String,
    pub direction: Direction,
    pub name: String,
    pub local_dir: PathBuf,
    pub remote_dir: String,
    pub conflict: ConflictPolicy,
}

pub enum TransferCommand {
    Enqueue(TransferJob),
    Cancel { item_id: String },
    CancelAll,
    Disconnect,
}

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TransferEvent {
    Queued {
        item_id: String,
        direction: Direction,
        name: String,
        source: String,
        destination: String,
    },
    Planning {
        item_id: String,
    },
    Planned {
        item_id: String,
        file_count: u32,
        total_bytes: u64,
    },
    Started {
        item_id: String,
        file_index: u32,
        file_count: u32,
        current_name: String,
    },
    Progress {
        item_id: String,
        transferred_bytes: u64,
        total_bytes: u64,
        file_index: u32,
        file_count: u32,
        bytes_per_sec: u64,
    },
    FileDone {
        item_id: String,
        file_index: u32,
        path: String,
    },
    FileFailed {
        item_id: String,
        path: String,
        message: String,
    },
    Skipped {
        item_id: String,
        path: String,
        reason: String,
    },
    Canceled {
        item_id: String,
    },
    Failed {
        item_id: String,
        message: String,
    },
    Completed {
        item_id: String,
        transferred_bytes: u64,
        file_count: u32,
        skipped: u32,
        failed: u32,
    },
    Idle,
}

pub fn spawn(
    pool: Pool,
    vault: Arc<VaultManager>,
    profile_id: String,
    emit: impl Fn(TransferEvent) + Send + 'static,
) -> Sender<TransferCommand> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || run(pool, vault, profile_id, rx, &emit));
    tx
}

/// Queue plus cancellation bookkeeping, kept apart from the connection so the
/// chunk loop can borrow the `Sftp` and still drain commands.
#[derive(Default)]
struct Control {
    queue: VecDeque<TransferJob>,
    cancelled: HashSet<String>,
    current: Option<String>,
    stop: bool,
}

impl Control {
    fn is_cancelled(&self, item_id: &str) -> bool {
        self.stop || self.cancelled.contains(item_id)
    }
}

enum Outcome {
    Completed {
        transferred: u64,
        files: u32,
        skipped: u32,
        failed: u32,
    },
    Canceled,
}

enum FileOutcome {
    Done,
    Skipped(String),
    Canceled,
}

fn run(
    pool: Pool,
    vault: Arc<VaultManager>,
    profile_id: String,
    rx: Receiver<TransferCommand>,
    emit: Emit<'_>,
) {
    let mut conn: Option<SftpConn> = None;
    let mut ctl = Control::default();

    loop {
        if ctl.queue.is_empty() {
            match rx.recv_timeout(IDLE_TICK) {
                Ok(command) => apply(command, &mut ctl, emit),
                Err(RecvTimeoutError::Timeout) => {
                    if conn.as_ref().is_some_and(|c| c.session.keepalive_send().is_err()) {
                        conn = None;
                    }
                }
                Err(RecvTimeoutError::Disconnected) => break,
            }
        } else {
            drain(&rx, &mut ctl, emit);
        }
        if ctl.stop {
            break;
        }

        let Some(job) = ctl.queue.pop_front() else { continue };

        if conn.is_none() {
            match session::open_connection(&pool, &vault, &profile_id) {
                Ok(opened) => conn = Some(opened),
                Err(e) => {
                    emit(TransferEvent::Failed {
                        item_id: job.item_id,
                        message: e.to_string(),
                    });
                    continue;
                }
            }
        }

        ctl.current = Some(job.item_id.clone());
        let outcome = {
            let sftp = &conn.as_ref().expect("connected above").sftp;
            guard("the transfer", || execute(sftp, &job, &rx, &mut ctl, emit))
        };
        ctl.current = None;
        ctl.cancelled.remove(&job.item_id);

        match outcome {
            Ok(Outcome::Completed {
                transferred,
                files,
                skipped,
                failed,
            }) => emit(TransferEvent::Completed {
                item_id: job.item_id,
                transferred_bytes: transferred,
                file_count: files,
                skipped,
                failed,
            }),
            Ok(Outcome::Canceled) => emit(TransferEvent::Canceled {
                item_id: job.item_id,
            }),
            Err(e) => {
                emit(TransferEvent::Failed {
                    item_id: job.item_id,
                    message: e.to_string(),
                });
                // Self-healing: drop the connection and let the next job open a
                // fresh one rather than retrying on a session of unknown state.
                conn = None;
            }
        }

        if ctl.queue.is_empty() {
            emit(TransferEvent::Idle);
        }
    }

    for job in ctl.queue.drain(..) {
        emit(TransferEvent::Failed {
            item_id: job.item_id,
            message: "session closed".into(),
        });
    }
}

fn apply(command: TransferCommand, ctl: &mut Control, emit: Emit<'_>) {
    match command {
        TransferCommand::Enqueue(job) => {
            let (source, destination) = endpoints(&job);
            emit(TransferEvent::Queued {
                item_id: job.item_id.clone(),
                direction: job.direction,
                name: job.name.clone(),
                source,
                destination,
            });
            ctl.queue.push_back(job);
        }
        TransferCommand::Cancel { item_id } => {
            if let Some(index) = ctl.queue.iter().position(|j| j.item_id == item_id) {
                ctl.queue.remove(index);
                emit(TransferEvent::Canceled { item_id });
            } else {
                ctl.cancelled.insert(item_id);
            }
        }
        TransferCommand::CancelAll => {
            for job in ctl.queue.drain(..) {
                emit(TransferEvent::Canceled {
                    item_id: job.item_id,
                });
            }
            if let Some(current) = ctl.current.clone() {
                ctl.cancelled.insert(current);
            }
        }
        TransferCommand::Disconnect => ctl.stop = true,
    }
}

fn drain(rx: &Receiver<TransferCommand>, ctl: &mut Control, emit: Emit<'_>) {
    loop {
        match rx.try_recv() {
            Ok(command) => apply(command, ctl, emit),
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                ctl.stop = true;
                return;
            }
        }
    }
}

fn endpoints(job: &TransferJob) -> (String, String) {
    let remote = path::join(&job.remote_dir, &job.name);
    let local = job.local_dir.join(&job.name).to_string_lossy().to_string();
    match job.direction {
        Direction::Download => (remote, local),
        Direction::Upload => (local, remote),
    }
}

fn execute(
    sftp: &Sftp,
    job: &TransferJob,
    rx: &Receiver<TransferCommand>,
    ctl: &mut Control,
    emit: Emit<'_>,
) -> AppResult<Outcome> {
    let remote_root = path::join(&job.remote_dir, &job.name);
    let local_root = job.local_dir.join(&job.name);

    let announce_scan = || {
        emit(TransferEvent::Planning {
            item_id: job.item_id.clone(),
        })
    };

    // Whether the selection is a directory is the source filesystem's business,
    // not the caller's - one stat answers it and gives the size for free.
    let plan = match job.direction {
        Direction::Download => {
            let stat = sftp.stat(Path::new(&remote_root)).map_err(session::map_ssh)?;
            if stat.is_dir() {
                announce_scan();
                walk::plan(job.direction, remote_root, local_root, &mut |remote, _| {
                    read_remote_dir(sftp, remote)
                })?
            } else {
                single_file(remote_root, local_root, stat.size.unwrap_or(0))
            }
        }
        Direction::Upload => {
            let meta = fs::metadata(&local_root)?;
            if meta.is_dir() {
                announce_scan();
                walk::plan(job.direction, remote_root, local_root, &mut |_, local| {
                    read_local_dir(local)
                })?
            } else {
                single_file(remote_root, local_root, meta.len())
            }
        }
    };

    let file_count = plan.files.len() as u32;
    tracing::info!(
        item = %job.item_id,
        direction = ?job.direction,
        files = file_count,
        dirs = plan.dirs.len(),
        bytes = plan.total_bytes,
        "sftp transfer planned"
    );
    emit(TransferEvent::Planned {
        item_id: job.item_id.clone(),
        file_count,
        total_bytes: plan.total_bytes,
    });

    for (target, reason) in &plan.skipped {
        emit(TransferEvent::Skipped {
            item_id: job.item_id.clone(),
            path: target.clone(),
            reason: reason.clone(),
        });
    }

    // Plan order is breadth-first, so a parent is always created before its
    // children need it.
    for dir in &plan.dirs {
        match job.direction {
            Direction::Download => fs::create_dir_all(&dir.local)?,
            Direction::Upload => ensure_remote_dir(sftp, &dir.remote)?,
        }
    }

    let mut transferred = 0u64;
    let mut skipped = plan.skipped.len() as u32;
    let mut failed = 0u32;
    let mut throttle = Throttle::new();

    for (index, task) in plan.files.iter().enumerate() {
        drain(rx, ctl, emit);
        if ctl.is_cancelled(&job.item_id) {
            return Ok(Outcome::Canceled);
        }

        let file_index = index as u32 + 1;
        emit(TransferEvent::Started {
            item_id: job.item_id.clone(),
            file_index,
            file_count,
            current_name: display_name(task, job.direction),
        });

        let result = transfer_one(
            sftp,
            job,
            task,
            rx,
            ctl,
            emit,
            &mut Progress {
                transferred: &mut transferred,
                total: plan.total_bytes,
                file_index,
                file_count,
                throttle: &mut throttle,
            },
        );

        match result {
            Ok(FileOutcome::Done) => {
                if plan.files.len() <= MAX_FILE_DONE_EVENTS {
                    emit(TransferEvent::FileDone {
                        item_id: job.item_id.clone(),
                        file_index,
                        path: display_name(task, job.direction),
                    });
                }
            }
            Ok(FileOutcome::Skipped(reason)) => {
                skipped += 1;
                emit(TransferEvent::Skipped {
                    item_id: job.item_id.clone(),
                    path: display_name(task, job.direction),
                    reason,
                });
            }
            Ok(FileOutcome::Canceled) => return Ok(Outcome::Canceled),
            Err(e) => {
                // One unreadable file should not sink the queue - but a dead
                // session should not be reported as 200 individual failures
                // either, so probe before continuing.
                tracing::warn!(
                    item = %job.item_id,
                    file = %display_name(task, job.direction),
                    error = %e,
                    "sftp file transfer failed"
                );
                if session::realpath(sftp, ".").is_err() {
                    return Err(e);
                }
                failed += 1;
                emit(TransferEvent::FileFailed {
                    item_id: job.item_id.clone(),
                    path: display_name(task, job.direction),
                    message: e.to_string(),
                });
            }
        }
    }

    Ok(Outcome::Completed {
        transferred,
        files: file_count,
        skipped,
        failed,
    })
}

fn single_file(remote: String, local: PathBuf, size: u64) -> walk::TransferPlan {
    walk::TransferPlan {
        files: vec![walk::FileTask { remote, local, size }],
        total_bytes: size,
        ..Default::default()
    }
}

struct Progress<'a> {
    transferred: &'a mut u64,
    total: u64,
    file_index: u32,
    file_count: u32,
    throttle: &'a mut Throttle,
}

fn transfer_one(
    sftp: &Sftp,
    job: &TransferJob,
    task: &walk::FileTask,
    rx: &Receiver<TransferCommand>,
    ctl: &mut Control,
    emit: Emit<'_>,
    progress: &mut Progress<'_>,
) -> AppResult<FileOutcome> {
    match job.direction {
        Direction::Download => download(sftp, job, task, rx, ctl, emit, progress),
        Direction::Upload => upload(sftp, job, task, rx, ctl, emit, progress),
    }
}

fn download(
    sftp: &Sftp,
    job: &TransferJob,
    task: &walk::FileTask,
    rx: &Receiver<TransferCommand>,
    ctl: &mut Control,
    emit: Emit<'_>,
    progress: &mut Progress<'_>,
) -> AppResult<FileOutcome> {
    let mut destination = task.local.clone();
    if destination.exists() {
        match job.conflict {
            ConflictPolicy::Skip => return Ok(FileOutcome::Skipped("already exists".into())),
            ConflictPolicy::KeepBoth => destination = unique_local_path(&destination),
            ConflictPolicy::Overwrite => {}
        }
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut source = sftp.open(Path::new(&task.remote)).map_err(session::map_ssh)?;
    let mut sink = fs::File::create(&destination)?;
    let mut buffer = vec![0u8; CHUNK];

    loop {
        let read = match source.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                drop(sink);
                let _ = fs::remove_file(&destination);
                return Err(e.into());
            }
        };
        if let Err(e) = sink.write_all(&buffer[..read]) {
            drop(sink);
            let _ = fs::remove_file(&destination);
            return Err(e.into());
        }

        *progress.transferred += read as u64;
        drain(rx, ctl, emit);
        if ctl.is_cancelled(&job.item_id) {
            drop(sink);
            let _ = fs::remove_file(&destination);
            return Ok(FileOutcome::Canceled);
        }
        report(emit, job, progress, false);
    }

    report(emit, job, progress, true);
    Ok(FileOutcome::Done)
}

fn upload(
    sftp: &Sftp,
    job: &TransferJob,
    task: &walk::FileTask,
    rx: &Receiver<TransferCommand>,
    ctl: &mut Control,
    emit: Emit<'_>,
    progress: &mut Progress<'_>,
) -> AppResult<FileOutcome> {
    let mut destination = task.remote.clone();
    if sftp.stat(Path::new(&destination)).is_ok() {
        match job.conflict {
            ConflictPolicy::Skip => return Ok(FileOutcome::Skipped("already exists".into())),
            ConflictPolicy::KeepBoth => destination = unique_remote_path(sftp, &destination),
            // `Sftp::create` is WRITE|TRUNCATE, which is the overwrite we want.
            ConflictPolicy::Overwrite => {}
        }
    }

    let mut source = fs::File::open(&task.local)?;
    let mut sink = sftp.create(Path::new(&destination)).map_err(session::map_ssh)?;
    let mut buffer = vec![0u8; CHUNK];

    loop {
        let read = match source.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                drop(sink);
                let _ = sftp.unlink(Path::new(&destination));
                return Err(e.into());
            }
        };

        // libssh2 caps a single SFTP packet well below our chunk size and
        // returns short writes, so loop on the count rather than `write_all`.
        let mut offset = 0;
        while offset < read {
            match sink.write(&buffer[offset..read]) {
                Ok(0) => {
                    drop(sink);
                    let _ = sftp.unlink(Path::new(&destination));
                    return Err(AppError::Ssh("upload stalled".into()));
                }
                Ok(n) => offset += n,
                Err(e) => {
                    drop(sink);
                    let _ = sftp.unlink(Path::new(&destination));
                    return Err(e.into());
                }
            }
        }

        *progress.transferred += read as u64;
        drain(rx, ctl, emit);
        if ctl.is_cancelled(&job.item_id) {
            drop(sink);
            let _ = sftp.unlink(Path::new(&destination));
            return Ok(FileOutcome::Canceled);
        }
        report(emit, job, progress, false);
    }

    // libssh2 buffers, and a rejected write (quota, a full disk, a permission
    // the server only enforces on close) surfaces here rather than from
    // `write`. Letting `Drop` swallow it would report a success that left no
    // file behind.
    if let Err(e) = sink.close() {
        let _ = sftp.unlink(Path::new(&destination));
        return Err(session::map_ssh(e));
    }

    tracing::info!(
        item = %job.item_id,
        destination = %destination,
        bytes = task.size,
        "sftp upload wrote file"
    );
    report(emit, job, progress, true);
    Ok(FileOutcome::Done)
}

fn report(emit: Emit<'_>, job: &TransferJob, progress: &mut Progress<'_>, force: bool) {
    let transferred = *progress.transferred;
    if let Some(rate) = progress.throttle.tick(Instant::now(), transferred, force) {
        emit(TransferEvent::Progress {
            item_id: job.item_id.clone(),
            transferred_bytes: transferred,
            total_bytes: progress.total,
            file_index: progress.file_index,
            file_count: progress.file_count,
            bytes_per_sec: rate,
        });
    }
}

fn display_name(task: &walk::FileTask, direction: Direction) -> String {
    match direction {
        Direction::Download => task.remote.clone(),
        Direction::Upload => task.local.to_string_lossy().to_string(),
    }
}

fn ensure_remote_dir(sftp: &Sftp, dir: &str) -> AppResult<()> {
    // Many servers answer `mkdir` on an existing directory with a generic
    // failure, so ask first.
    if let Ok(stat) = sftp.stat(Path::new(dir)) {
        return if stat.is_dir() {
            Ok(())
        } else {
            Err(AppError::Validation(format!("{dir} exists and is not a directory")))
        };
    }
    sftp.mkdir(Path::new(dir), 0o755).map_err(session::map_ssh)
}

fn read_remote_dir(sftp: &Sftp, dir: &str) -> AppResult<Vec<Entry>> {
    let mut handle = sftp.opendir(Path::new(dir)).map_err(session::map_ssh)?;
    let mut entries = Vec::new();
    loop {
        let (name, stat) = match handle.readdir() {
            Ok(item) => item,
            Err(e) if e.code() == ErrorCode::Session(LIBSSH2_ERROR_FILE) => break,
            Err(e) => return Err(session::map_ssh(e)),
        };
        let name = name.to_string_lossy().to_string();
        if name == "." || name == ".." {
            continue;
        }
        let kind = session::kind_of(&stat);
        entries.push(Entry {
            name,
            is_dir: kind == EntryKind::Directory,
            is_symlink: kind == EntryKind::Symlink,
            size: stat.size.unwrap_or(0),
        });
    }
    Ok(entries)
}

fn read_local_dir(dir: &Path) -> AppResult<Vec<Entry>> {
    let mut entries = Vec::new();
    for item in fs::read_dir(dir)? {
        let Ok(item) = item else { continue };
        let Ok(meta) = item.metadata() else { continue };
        entries.push(Entry {
            name: item.file_name().to_string_lossy().to_string(),
            is_dir: meta.is_dir(),
            is_symlink: meta.is_symlink(),
            size: if meta.is_dir() { 0 } else { meta.len() },
        });
    }
    Ok(entries)
}

fn unique_local_path(target: &Path) -> PathBuf {
    let parent = target.parent().map(Path::to_path_buf).unwrap_or_default();
    let name = target.file_name().unwrap_or_default().to_string_lossy().to_string();
    let chosen = next_available_name(&name, |candidate| parent.join(candidate).exists());
    parent.join(chosen)
}

fn unique_remote_path(sftp: &Sftp, target: &str) -> String {
    let parent = path::parent_of(target).unwrap_or_else(|| "/".to_string());
    let name = target.rsplit('/').next().unwrap_or(target).to_string();
    let chosen = next_available_name(&name, |candidate| {
        sftp.stat(Path::new(&path::join(&parent, candidate))).is_ok()
    });
    path::join(&parent, &chosen)
}

/// Splits at the first interior dot, so `archive.tar.gz` keeps its whole
/// compound extension and `.bashrc` is treated as a bare name.
fn split_name(name: &str) -> (&str, &str) {
    let start = usize::from(name.starts_with('.'));
    match name[start..].find('.') {
        Some(index) => name.split_at(start + index),
        None => (name, ""),
    }
}

fn next_available_name(name: &str, exists: impl Fn(&str) -> bool) -> String {
    if !exists(name) {
        return name.to_string();
    }
    let (stem, extension) = split_name(name);
    (1..10_000)
        .map(|n| format!("{stem} ({n}){extension}"))
        .find(|candidate| !exists(candidate))
        .unwrap_or_else(|| format!("{stem} ({}){extension}", crate::util::new_id()))
}

struct Throttle {
    last_at: Instant,
    last_bytes: u64,
    rate: f64,
}

impl Throttle {
    fn new() -> Self {
        Self {
            last_at: Instant::now(),
            last_bytes: 0,
            rate: 0.0,
        }
    }

    /// Returns the smoothed rate when an event is due. Emitting per 32KB chunk
    /// would put thousands of messages a second on the IPC channel; 100ms or
    /// 1MiB still animates smoothly.
    fn tick(&mut self, now: Instant, bytes: u64, force: bool) -> Option<u64> {
        let elapsed = now.duration_since(self.last_at);
        let due = force || elapsed >= PROGRESS_INTERVAL || bytes.saturating_sub(self.last_bytes) >= PROGRESS_BYTES;
        if !due {
            return None;
        }
        let seconds = elapsed.as_secs_f64();
        if seconds > 0.0 {
            let instant = (bytes.saturating_sub(self.last_bytes)) as f64 / seconds;
            self.rate = if self.rate == 0.0 {
                instant
            } else {
                self.rate * 0.7 + instant * 0.3
            };
        }
        self.last_at = now;
        self.last_bytes = bytes;
        Some(self.rate as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_name_keeps_compound_extensions_and_dotfiles_intact() {
        assert_eq!(split_name("a.txt"), ("a", ".txt"));
        assert_eq!(split_name("archive.tar.gz"), ("archive", ".tar.gz"));
        assert_eq!(split_name("README"), ("README", ""));
        assert_eq!(split_name(".bashrc"), (".bashrc", ""));
    }

    #[test]
    fn next_available_name_counts_up_past_what_exists() {
        let taken = ["report.pdf", "report (1).pdf"];
        let exists = |c: &str| taken.contains(&c);
        assert_eq!(next_available_name("report.pdf", exists), "report (2).pdf");
        assert_eq!(next_available_name("fresh.pdf", exists), "fresh.pdf");
    }

    #[test]
    fn next_available_name_handles_extensionless_and_dotfiles() {
        let exists = |c: &str| matches!(c, "notes" | ".bashrc");
        assert_eq!(next_available_name("notes", exists), "notes (1)");
        assert_eq!(next_available_name(".bashrc", exists), ".bashrc (1)");
    }

    #[test]
    fn throttle_holds_events_until_an_interval_or_a_megabyte() {
        let start = Instant::now();
        let mut throttle = Throttle {
            last_at: start,
            last_bytes: 0,
            rate: 0.0,
        };
        assert_eq!(throttle.tick(start + Duration::from_millis(10), 1024, false), None);
        assert!(throttle.tick(start + Duration::from_millis(10), PROGRESS_BYTES, false).is_some());
    }

    #[test]
    fn throttle_emits_once_the_interval_has_passed() {
        let start = Instant::now();
        let mut throttle = Throttle {
            last_at: start,
            last_bytes: 0,
            rate: 0.0,
        };
        assert!(throttle.tick(start + PROGRESS_INTERVAL, 10, false).is_some());
    }

    #[test]
    fn throttle_always_emits_when_forced() {
        let start = Instant::now();
        let mut throttle = Throttle {
            last_at: start,
            last_bytes: 0,
            rate: 0.0,
        };
        assert!(throttle.tick(start, 1, true).is_some());
    }
}
