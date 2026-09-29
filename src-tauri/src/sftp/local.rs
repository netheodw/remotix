//! Local filesystem browsing for the left-hand pane of the file transfer
//! screen. Lives under `sftp/` because it exists only to feed that UI - the app
//! has no other need to enumerate the user's disk.
//!
//! There is deliberately no sandbox here. The user already has full filesystem
//! access through Explorer, and a local-first tool that refused to list `C:\`
//! would just be broken. The guard that matters is [`super::path::sanitize_file_name`],
//! applied to names arriving from a remote server.

use std::fs::{self, Metadata};
use std::path::Path;
use std::time::UNIX_EPOCH;

use serde::Serialize;

use super::path::sanitize_file_name;
use super::EntryKind;
use crate::error::{AppError, AppResult};

/// A directory holding more than this freezes the table without helping anyone;
/// the pane shows a notice and offers its quick filter instead.
pub const MAX_LISTING_ENTRIES: usize = 10_000;

#[derive(Debug, Clone, Serialize)]
pub struct LocalEntry {
    pub name: String,
    pub path: String,
    pub kind: EntryKind,
    pub size: u64,
    pub modified: Option<i64>,
    pub hidden: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalListing {
    pub path: String,
    pub parent: Option<String>,
    pub entries: Vec<LocalEntry>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalRoot {
    pub label: String,
    pub path: String,
    pub kind: String,
}

pub fn list_dir(dir: &Path) -> AppResult<LocalListing> {
    if !dir.is_absolute() {
        return Err(AppError::Validation("path must be absolute".into()));
    }
    let meta = fs::metadata(dir)?;
    if !meta.is_dir() {
        return Err(AppError::Validation("not a directory".into()));
    }

    let mut entries = Vec::new();
    let mut truncated = false;
    for item in fs::read_dir(dir)? {
        // One unreadable entry must not sink the whole listing.
        let Ok(item) = item else { continue };
        let Ok(meta) = item.metadata() else { continue };
        let name = item.file_name().to_string_lossy().to_string();

        if entries.len() >= MAX_LISTING_ENTRIES {
            truncated = true;
            break;
        }
        entries.push(LocalEntry {
            kind: kind_of(&meta),
            size: if meta.is_dir() { 0 } else { meta.len() },
            modified: modified_secs(&meta),
            hidden: is_hidden(&name, &meta),
            path: display_path(&dir.join(&name)),
            name,
        });
    }

    Ok(LocalListing {
        path: display_path(dir),
        parent: dir.parent().map(display_path),
        entries,
        truncated,
    })
}

pub fn make_dir(parent: &Path, name: &str) -> AppResult<String> {
    let name = sanitize_file_name(name)?;
    let target = parent.join(name);
    fs::create_dir(&target)?;
    Ok(display_path(&target))
}

/// The roots offered in the pane's location menu. On Windows this probes every
/// drive letter, which can spin up an empty optical or removable drive and
/// stall for a second - the frontend caches the result for the app session.
#[cfg(windows)]
pub fn drive_roots() -> Vec<LocalRoot> {
    (b'A'..=b'Z')
        .map(|letter| format!("{}:\\", letter as char))
        .filter(|path| fs::metadata(path).is_ok())
        .map(|path| LocalRoot {
            label: path.trim_end_matches('\\').to_string(),
            path,
            kind: "drive".to_string(),
        })
        .collect()
}

#[cfg(not(windows))]
pub fn drive_roots() -> Vec<LocalRoot> {
    vec![LocalRoot {
        label: "/".to_string(),
        path: "/".to_string(),
        kind: "drive".to_string(),
    }]
}

/// `symlink_metadata` rather than `metadata`, so a symlink is reported as one
/// instead of silently resolving to whatever it points at.
fn kind_of(meta: &Metadata) -> EntryKind {
    if meta.is_symlink() {
        EntryKind::Symlink
    } else if meta.is_dir() {
        EntryKind::Directory
    } else if meta.is_file() {
        EntryKind::File
    } else {
        EntryKind::Other
    }
}

/// Deletes a file, or a directory and everything under it.
///
/// `remove_dir_all` does not traverse symlinks - it removes the link itself -
/// so a link inside the tree cannot take the delete somewhere else.
pub fn remove_entry(path: &Path) -> AppResult<()> {
    if !path.is_absolute() {
        return Err(AppError::Validation("path must be absolute".into()));
    }
    let meta = fs::symlink_metadata(path)?;
    if meta.is_dir() {
        fs::remove_dir_all(path)?;
    } else {
        fs::remove_file(path)?;
    }
    Ok(())
}

pub fn modified_at(path: &Path) -> Option<i64> {
    modified_secs(&fs::metadata(path).ok()?)
}

/// Removes a scratch copy made for editing a remote file, along with the
/// per-edit directory holding it. Refuses anything outside the app's own
/// temp area - this is a recursive delete and must never be pointed at an
/// arbitrary path.
pub fn discard_scratch(path: &Path) -> AppResult<()> {
    let root = std::env::temp_dir().join("remotix-edit");
    let dir = path.parent().unwrap_or(path);
    if !dir.starts_with(&root) || dir == root {
        return Err(AppError::Validation("not a scratch path".into()));
    }
    if dir.exists() {
        fs::remove_dir_all(dir)?;
    }
    Ok(())
}

fn modified_secs(meta: &Metadata) -> Option<i64> {
    let secs = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_secs();
    i64::try_from(secs).ok()
}

#[cfg(windows)]
fn is_hidden(_name: &str, meta: &Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;
    meta.file_attributes() & (FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM) != 0
}

#[cfg(not(windows))]
fn is_hidden(name: &str, _meta: &Metadata) -> bool {
    name.starts_with('.')
}

/// Drops a trailing separator so the path bar and the breadcrumb agree on one
/// spelling, except at a root where the separator is part of the name (`C:\`).
fn display_path(path: &Path) -> String {
    let text = path.to_string_lossy().to_string();
    if path.parent().is_none() {
        return text;
    }
    let trimmed = text.trim_end_matches(['/', '\\']);
    if trimmed.is_empty() {
        text
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_files_and_directories_with_their_kinds_and_sizes() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("notes.txt"), b"hello").unwrap();
        fs::create_dir(dir.path().join("nested")).unwrap();

        let listing = list_dir(dir.path()).unwrap();
        assert_eq!(listing.entries.len(), 2);
        assert!(!listing.truncated);

        let file = listing.entries.iter().find(|e| e.name == "notes.txt").unwrap();
        assert_eq!(file.kind, EntryKind::File);
        assert_eq!(file.size, 5);
        assert!(file.modified.is_some());

        let nested = listing.entries.iter().find(|e| e.name == "nested").unwrap();
        assert_eq!(nested.kind, EntryKind::Directory);
        assert_eq!(nested.size, 0);
    }

    #[test]
    fn reports_the_parent_directory() {
        let dir = tempfile::tempdir().unwrap();
        let child = dir.path().join("child");
        fs::create_dir(&child).unwrap();

        let listing = list_dir(&child).unwrap();
        assert_eq!(listing.parent.as_deref(), Some(display_path(dir.path()).as_str()));
    }

    #[test]
    fn missing_path_is_a_clean_error_not_a_panic() {
        let dir = tempfile::tempdir().unwrap();
        assert!(list_dir(&dir.path().join("nope")).is_err());
    }

    #[test]
    fn a_file_is_not_a_listable_directory() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("notes.txt");
        fs::write(&file, b"hi").unwrap();
        assert!(list_dir(&file).is_err());
    }

    #[test]
    fn relative_paths_are_rejected_at_the_boundary() {
        assert!(list_dir(Path::new("relative/path")).is_err());
    }

    #[test]
    fn remove_entry_deletes_a_file_and_leaves_its_siblings() {
        let dir = tempfile::tempdir().unwrap();
        let doomed = dir.path().join("doomed.txt");
        let keeper = dir.path().join("keeper.txt");
        fs::write(&doomed, b"x").unwrap();
        fs::write(&keeper, b"y").unwrap();

        remove_entry(&doomed).unwrap();
        assert!(!doomed.exists());
        assert!(keeper.exists());
    }

    #[test]
    fn remove_entry_deletes_a_directory_and_its_contents() {
        let dir = tempfile::tempdir().unwrap();
        let tree = dir.path().join("tree");
        fs::create_dir(&tree).unwrap();
        fs::create_dir(tree.join("nested")).unwrap();
        fs::write(tree.join("nested").join("deep.txt"), b"x").unwrap();

        remove_entry(&tree).unwrap();
        assert!(!tree.exists());
        assert!(dir.path().exists());
    }

    #[test]
    fn remove_entry_rejects_a_relative_path_and_a_missing_one() {
        assert!(remove_entry(Path::new("relative/path")).is_err());
        let dir = tempfile::tempdir().unwrap();
        assert!(remove_entry(&dir.path().join("nope")).is_err());
    }

    #[test]
    fn make_dir_rejects_a_name_that_would_escape_the_parent() {
        let dir = tempfile::tempdir().unwrap();
        assert!(make_dir(dir.path(), "..").is_err());
        assert!(make_dir(dir.path(), "a/b").is_err());
        assert!(make_dir(dir.path(), "fine").is_ok());
        assert!(dir.path().join("fine").is_dir());
    }
}
