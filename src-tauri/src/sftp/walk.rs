//! Turns "transfer this directory" into a flat list of directories to create
//! and files to copy, before a single byte moves. Planning up front is what
//! lets the queue show a real total instead of a spinner.
//!
//! The directory reader is injected, so the whole walk tests against an
//! in-memory tree with no SSH server involved.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use super::path::sanitize_file_name;
use super::transfer::Direction;
use crate::error::{AppError, AppResult};

/// Deep enough for any real tree, shallow enough that a symlink loop the
/// server resolved for us cannot run away.
const MAX_DEPTH: usize = 64;
const MAX_PLAN_ENTRIES: usize = 50_000;

/// One directory entry, in whichever filesystem is being walked.
pub struct Entry {
    pub name: String,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size: u64,
}

#[derive(Debug, PartialEq)]
pub struct DirTask {
    pub remote: String,
    pub local: PathBuf,
}

#[derive(Debug, PartialEq)]
pub struct FileTask {
    pub remote: String,
    pub local: PathBuf,
    pub size: u64,
}

#[derive(Default)]
pub struct TransferPlan {
    pub dirs: Vec<DirTask>,
    pub files: Vec<FileTask>,
    pub total_bytes: u64,
    pub skipped: Vec<(String, String)>,
}

/// A name arriving from the server can be anything, including `..` or `C:evil`,
/// and must never be joined onto a local path unchecked. A name arriving from
/// the local disk only needs to be safe to splice into a POSIX path.
fn check_name(direction: Direction, name: &str) -> AppResult<()> {
    match direction {
        Direction::Download => sanitize_file_name(name).map(|_| ()),
        Direction::Upload => {
            if name.contains(['/', '\\', '\0']) {
                Err(AppError::Validation("unsafe file name".into()))
            } else {
                Ok(())
            }
        }
    }
}

/// Breadth-first over an explicit queue - no recursion, so a deep tree cannot
/// blow the worker's stack. `read_dir` is handed both sides' paths and reads
/// whichever one the direction makes the source.
pub fn plan(
    direction: Direction,
    root_remote: String,
    root_local: PathBuf,
    read_dir: &mut impl FnMut(&str, &Path) -> AppResult<Vec<Entry>>,
) -> AppResult<TransferPlan> {
    let mut plan = TransferPlan {
        dirs: vec![DirTask {
            remote: root_remote.clone(),
            local: root_local.clone(),
        }],
        ..Default::default()
    };
    let mut queue = VecDeque::from([(root_remote, root_local, 0usize)]);

    while let Some((remote, local, depth)) = queue.pop_front() {
        if depth >= MAX_DEPTH {
            return Err(AppError::Validation(format!(
                "directory tree is deeper than {MAX_DEPTH} levels"
            )));
        }

        for entry in read_dir(&remote, &local)? {
            if check_name(direction, &entry.name).is_err() {
                plan.skipped
                    .push((format!("{remote}/{}", entry.name), "unsafe file name".into()));
                continue;
            }

            let child_remote = super::path::join(&remote, &entry.name);
            let child_local = local.join(&entry.name);

            // Not followed: a link can point outside the tree, or back into it.
            if entry.is_symlink {
                plan.skipped.push((child_remote, "symbolic link".into()));
                continue;
            }

            if entry.is_dir {
                plan.dirs.push(DirTask {
                    remote: child_remote.clone(),
                    local: child_local.clone(),
                });
                queue.push_back((child_remote, child_local, depth + 1));
            } else {
                if plan.files.len() >= MAX_PLAN_ENTRIES {
                    return Err(AppError::Validation(format!(
                        "more than {MAX_PLAN_ENTRIES} files in one transfer"
                    )));
                }
                plan.total_bytes += entry.size;
                plan.files.push(FileTask {
                    remote: child_remote,
                    local: child_local,
                    size: entry.size,
                });
            }
        }
    }

    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn entry(name: &str, is_dir: bool, size: u64) -> Entry {
        Entry {
            name: name.to_string(),
            is_dir,
            is_symlink: false,
            size,
        }
    }

    /// Fake remote tree keyed by POSIX path.
    fn tree(map: Vec<(&'static str, Vec<Entry>)>) -> impl FnMut(&str, &Path) -> AppResult<Vec<Entry>> {
        let mut map: HashMap<String, Vec<Entry>> = map
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        move |remote: &str, _local: &Path| Ok(map.remove(remote).unwrap_or_default())
    }

    #[test]
    fn flattens_a_nested_tree_into_dirs_and_files() {
        let mut read = tree(vec![
            ("/src", vec![entry("a.txt", false, 10), entry("nested", true, 0)]),
            ("/src/nested", vec![entry("b.txt", false, 32)]),
        ]);
        let plan = plan(Direction::Download, "/src".into(), PathBuf::from("D:\\dst"), &mut read).unwrap();

        assert_eq!(plan.total_bytes, 42);
        assert_eq!(
            plan.dirs,
            vec![
                DirTask { remote: "/src".into(), local: PathBuf::from("D:\\dst") },
                DirTask { remote: "/src/nested".into(), local: PathBuf::from("D:\\dst\\nested") },
            ]
        );
        assert_eq!(
            plan.files,
            vec![
                FileTask { remote: "/src/a.txt".into(), local: PathBuf::from("D:\\dst\\a.txt"), size: 10 },
                FileTask { remote: "/src/nested/b.txt".into(), local: PathBuf::from("D:\\dst\\nested\\b.txt"), size: 32 },
            ]
        );
    }

    #[test]
    fn a_server_returning_a_traversal_name_is_skipped_not_joined() {
        let mut read = tree(vec![(
            "/src",
            vec![entry("..", false, 1), entry("../../evil", false, 1), entry("ok.txt", false, 5)],
        )]);
        let plan = plan(Direction::Download, "/src".into(), PathBuf::from("D:\\dst"), &mut read).unwrap();

        assert_eq!(plan.files.len(), 1);
        assert_eq!(plan.files[0].local, PathBuf::from("D:\\dst\\ok.txt"));
        assert_eq!(plan.skipped.len(), 2);
    }

    #[test]
    fn symlinks_are_skipped_with_a_reason_rather_than_followed() {
        let mut read = tree(vec![(
            "/src",
            vec![
                Entry { name: "link".into(), is_dir: true, is_symlink: true, size: 0 },
                entry("real.txt", false, 3),
            ],
        )]);
        let plan = plan(Direction::Download, "/src".into(), PathBuf::from("D:\\dst"), &mut read).unwrap();

        assert_eq!(plan.files.len(), 1);
        assert_eq!(plan.skipped, vec![("/src/link".to_string(), "symbolic link".to_string())]);
    }

    #[test]
    fn upload_allows_names_windows_would_reject_but_still_blocks_separators() {
        let mut read = tree(vec![(
            "/dst",
            vec![entry("weird?name", false, 1), entry("a/b", false, 1)],
        )]);
        let plan = plan(Direction::Upload, "/dst".into(), PathBuf::from("D:\\src"), &mut read).unwrap();

        assert_eq!(plan.files.len(), 1);
        assert_eq!(plan.files[0].remote, "/dst/weird?name");
        assert_eq!(plan.skipped.len(), 1);
    }

    #[test]
    fn refuses_a_tree_deeper_than_the_cap() {
        // Every directory contains another directory, forever.
        let mut read = |remote: &str, _local: &Path| {
            Ok(vec![entry(remote.len().to_string().as_str(), true, 0)])
        };
        let result = plan(Direction::Download, "/src".into(), PathBuf::from("D:\\dst"), &mut read);
        assert!(result.is_err());
    }

    #[test]
    fn an_empty_directory_still_plans_its_own_creation() {
        let mut read = tree(vec![("/src", vec![])]);
        let plan = plan(Direction::Download, "/src".into(), PathBuf::from("D:\\dst"), &mut read).unwrap();
        assert_eq!(plan.files.len(), 0);
        assert_eq!(plan.dirs.len(), 1);
    }
}
