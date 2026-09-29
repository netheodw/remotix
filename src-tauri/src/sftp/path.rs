//! POSIX path helpers for the remote side of the file browser, plus the
//! sanitizer every filename must pass before it is joined onto a destination
//! directory. Deliberately free of `ssh2` types so it all unit tests without a
//! server.

use crate::error::{AppError, AppResult};

const MAX_NAME_BYTES: usize = 255;

/// Stems Windows refuses to create a file for, with or without an extension.
const RESERVED_STEMS: [&str; 22] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// Joins a bare filename onto a POSIX directory path.
pub fn join(dir: &str, name: &str) -> String {
    let base = dir.trim_end_matches('/');
    if base.is_empty() {
        format!("/{name}")
    } else {
        format!("{base}/{name}")
    }
}

/// The parent of a POSIX path, or `None` when already at the root.
pub fn parent_of(path: &str) -> Option<String> {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    match trimmed.rfind('/') {
        Some(0) => Some("/".to_string()),
        Some(i) => Some(trimmed[..i].to_string()),
        None => None,
    }
}

/// The minimum a name off the wire must satisfy before it is spliced into a
/// remote path we then act on. Deliberately weaker than [`sanitize_file_name`],
/// which also enforces Windows' rules - a POSIX server may legitimately hold a
/// file called `a?b`, and refusing to delete it would be wrong. What matters
/// here is only that the name cannot redirect the path somewhere else.
pub fn check_remote_segment(name: &str) -> AppResult<()> {
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\0']) {
        return Err(AppError::Validation(format!("unsafe remote name: {name:?}")));
    }
    Ok(())
}

/// Rejects any filename that could escape its destination directory or that is
/// unrepresentable on Windows.
///
/// SFTP filenames are arbitrary byte strings and libssh2 filters only the
/// literal `.` and `..`, so a hostile or compromised server can answer a
/// directory listing with `../../evil` and walk a download out of the folder
/// the user picked. Every name that comes off the wire goes through here before
/// it is joined onto a local path. User-typed folder names get the same
/// treatment: the rules are a superset of what either side allows.
pub fn sanitize_file_name(name: &str) -> AppResult<String> {
    let reject = |why: &str| Err(AppError::Validation(format!("unsafe file name: {why}")));

    if name.is_empty() {
        return reject("empty");
    }
    if name.len() > MAX_NAME_BYTES {
        return reject("too long");
    }
    if name == "." || name == ".." {
        return reject("relative path segment");
    }
    if name.contains(['/', '\\', ':', '\0']) {
        return reject("contains a path separator");
    }
    if name.contains(['<', '>', '"', '|', '?', '*']) {
        return reject("contains a reserved character");
    }
    if name.chars().any(|c| c.is_control()) {
        return reject("contains a control character");
    }
    if name.ends_with('.') || name.ends_with(' ') {
        return reject("trailing dot or space");
    }
    let stem = name.split('.').next().unwrap_or(name).to_ascii_lowercase();
    if RESERVED_STEMS.contains(&stem.as_str()) {
        return reject("reserved device name");
    }
    Ok(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_builds_posix_paths_without_doubling_separators() {
        assert_eq!(join("/", "a"), "/a");
        assert_eq!(join("/home/user", "notes.txt"), "/home/user/notes.txt");
        assert_eq!(join("/home/user/", "notes.txt"), "/home/user/notes.txt");
        assert_eq!(join("", "a"), "/a");
        assert_eq!(join("/srv", "with space #1"), "/srv/with space #1");
    }

    #[test]
    fn parent_of_stops_at_the_root() {
        assert_eq!(parent_of("/"), None);
        assert_eq!(parent_of(""), None);
        assert_eq!(parent_of("/a").as_deref(), Some("/"));
        assert_eq!(parent_of("/a/b").as_deref(), Some("/a"));
        assert_eq!(parent_of("/a/b/").as_deref(), Some("/a"));
    }

    #[test]
    fn check_remote_segment_blocks_redirection_but_allows_posix_names() {
        for hostile in ["", ".", "..", "a/b", "../etc", "with\0nul"] {
            assert!(check_remote_segment(hostile).is_err(), "accepted {hostile:?}");
        }
        // Legal on POSIX, and refusing to act on them would be wrong.
        for ok in ["a?b", "<odd>", "trailing.", "COM1", "name with spaces"] {
            assert!(check_remote_segment(ok).is_ok(), "rejected {ok:?}");
        }
    }

    #[test]
    fn sanitize_accepts_ordinary_names() {
        assert_eq!(sanitize_file_name("notes.txt").unwrap(), "notes.txt");
        assert_eq!(sanitize_file_name("normal name.tar.gz").unwrap(), "normal name.tar.gz");
        assert_eq!(sanitize_file_name(".bashrc").unwrap(), ".bashrc");
    }

    #[test]
    fn sanitize_rejects_every_way_out_of_the_destination_directory() {
        for hostile in ["", ".", "..", "a/b", "a\\b", "../../evil", "..\\evil", "C:evil"] {
            assert!(sanitize_file_name(hostile).is_err(), "accepted {hostile:?}");
        }
    }

    #[test]
    fn sanitize_rejects_names_windows_cannot_represent() {
        for bad in ["con", "COM1.txt", "nul", "trailing.", "trailing ", "<bad>", "pipe|name", "q?"] {
            assert!(sanitize_file_name(bad).is_err(), "accepted {bad:?}");
        }
        assert!(sanitize_file_name("with\0nul").is_err());
        assert!(sanitize_file_name("tab\there").is_err());
        assert!(sanitize_file_name(&"a".repeat(300)).is_err());
    }
}
