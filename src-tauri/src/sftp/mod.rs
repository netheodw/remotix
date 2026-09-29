//! File browsing and transfer for the Files screen: the local filesystem in
//! [`local`], POSIX path handling and the filename sanitizer in [`path`].

pub mod local;
pub mod path;
pub mod session;
pub mod transfer;
pub mod walk;

use std::panic::{catch_unwind, AssertUnwindSafe};

use serde::Serialize;

use crate::error::{AppError, AppResult};

/// Runs one worker operation, converting a panic into an error.
///
/// `ssh2`'s Windows path conversion is `PathBuf::from(str::from_utf8(bytes).unwrap())`,
/// and SFTP filenames are arbitrary byte strings - so a single Latin-1 filename
/// in a listing panics inside the crate, on a thread that would otherwise take
/// the whole app down with it. Wrapping each request also contains any other
/// latent panic in a worker rather than losing the window mid-transfer.
pub fn guard<T>(operation: &str, body: impl FnOnce() -> AppResult<T>) -> AppResult<T> {
    catch_unwind(AssertUnwindSafe(body)).unwrap_or_else(|_| {
        tracing::error!(operation, "worker panicked");
        Err(AppError::Ssh(format!(
            "{operation} failed: the server sent a name this client cannot represent"
        )))
    })
}

/// What a directory entry is, on either side of the transfer. `Symlink` is
/// reported rather than resolved, so the UI can show it and the recursive walk
/// can refuse to descend through it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    File,
    Directory,
    Symlink,
    Other,
}
