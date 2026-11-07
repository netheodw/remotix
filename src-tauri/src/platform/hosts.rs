//! Keeps the local hostnames of port forwards (`ifg-portainer.local` and
//! the like) in the OS hosts file, so the forward can be opened by name.
//!
//! Remotix owns a single marked block in the hosts file and never touches a
//! line outside it. Writing the hosts file needs administrator rights: if the
//! app is not elevated, it starts itself elevated (one UAC prompt) with
//! `--apply-hosts <file>`, and that process, handled in `main` before the
//! app starts, validates the entries and rewrites only the block.

use std::net::IpAddr;
use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};

pub const BEGIN_MARKER: &str = "# BEGIN Remotix port forwards (managed by Remotix, do not edit)";
pub const END_MARKER: &str = "# END Remotix port forwards";
pub const APPLY_FLAG: &str = "--apply-hosts";
const MAX_ENTRIES: usize = 1000;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct HostsEntry {
    pub hostname: String,
    pub ip: String,
}

pub fn hosts_path() -> PathBuf {
    #[cfg(windows)]
    {
        let root = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        root.join(r"System32\drivers\etc\hosts")
    }
    #[cfg(not(windows))]
    {
        PathBuf::from("/etc/hosts")
    }
}

/// The block's lines, one `ip<TAB>hostname` per entry.
pub fn render_entries(entries: &[HostsEntry]) -> Vec<String> {
    entries.iter().map(|e| format!("{}\t{}", e.ip, e.hostname)).collect()
}

/// The non-empty lines currently inside the managed block.
pub fn managed_lines(content: &str) -> Vec<String> {
    let mut inside = false;
    let mut lines = Vec::new();
    for line in content.lines() {
        let t = line.trim();
        if t == BEGIN_MARKER {
            inside = true;
        } else if t == END_MARKER {
            inside = false;
        } else if inside && !t.is_empty() {
            lines.push(t.to_string());
        }
    }
    lines
}

/// `content` with the managed block replaced by `lines` (removed when empty).
/// Everything outside the block is kept as it was, and the file's line
/// ending (CRLF on Windows) is preserved.
pub fn replace_block(content: &str, lines: &[String]) -> String {
    let eol = if content.contains("\r\n") || (cfg!(windows) && !content.contains('\n')) { "\r\n" } else { "\n" };
    let mut kept: Vec<&str> = Vec::new();
    let mut inside = false;
    for line in content.lines() {
        let t = line.trim();
        if t == BEGIN_MARKER {
            inside = true;
        } else if t == END_MARKER {
            inside = false;
        } else if !inside {
            kept.push(line);
        }
    }
    while kept.last().is_some_and(|l| l.trim().is_empty()) {
        kept.pop();
    }
    let mut out = kept.join(eol);
    if !lines.is_empty() {
        if !out.is_empty() {
            out.push_str(eol);
            out.push_str(eol);
        }
        out.push_str(BEGIN_MARKER);
        out.push_str(eol);
        for l in lines {
            out.push_str(l);
            out.push_str(eol);
        }
        out.push_str(END_MARKER);
    }
    out.push_str(eol);
    out
}

/// Parses one `ip hostname` line, refusing anything else. The elevated
/// process only writes lines that pass this.
pub fn parse_entry_line(line: &str) -> Option<HostsEntry> {
    let mut parts = line.split_whitespace();
    let ip = parts.next()?;
    let hostname = parts.next()?;
    if parts.next().is_some() || ip.parse::<IpAddr>().is_err() || !is_valid_local_hostname(hostname) {
        return None;
    }
    Some(HostsEntry { hostname: hostname.to_string(), ip: ip.to_string() })
}

/// A DNS name (letters, digits, `-`, dots), not an IP and not `localhost`.
pub fn is_valid_local_hostname(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 253
        && name.parse::<IpAddr>().is_err()
        && !name.eq_ignore_ascii_case("localhost")
        && name.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
}

/// Makes the hosts file's managed block hold exactly `entries`. Returns
/// whether anything changed; when nothing did, no prompt is shown.
pub fn sync(entries: &[HostsEntry]) -> AppResult<bool> {
    let path = hosts_path();
    let current = read_hosts(&path)?;
    let wanted = render_entries(entries);
    if managed_lines(&current) == wanted {
        return Ok(false);
    }
    match std::fs::write(&path, replace_block(&current, &wanted)) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            write_elevated(&wanted)?;
            Ok(true)
        }
        Err(e) => Err(AppError::Internal(format!("could not write the hosts file: {e}"))),
    }
}

fn read_hosts(path: &Path) -> AppResult<String> {
    match std::fs::read_to_string(path) {
        Ok(s) => Ok(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(AppError::Internal(format!("could not read the hosts file: {e}"))),
    }
}

#[cfg(windows)]
fn write_elevated(lines: &[String]) -> AppResult<()> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let dir = std::env::temp_dir().join("remotix-hosts");
    std::fs::create_dir_all(&dir).map_err(|e| AppError::Internal(format!("could not prepare the hosts update: {e}")))?;
    let file = dir.join(format!("{}.txt", crate::util::new_id()));
    std::fs::write(&file, lines.join("\n"))
        .map_err(|e| AppError::Internal(format!("could not prepare the hosts update: {e}")))?;
    let exe = std::env::current_exe().map_err(|e| AppError::Internal(e.to_string()))?;

    // PowerShell single-quoted strings escape ' as ''. The file path goes in
    // double quotes inside the argument string, in case it has spaces.
    let q = |p: &Path| p.display().to_string().replace('\'', "''");
    let script = format!(
        "try {{ $p = Start-Process -FilePath '{exe}' -ArgumentList '{flag} \"{file}\"' -Verb RunAs -WindowStyle Hidden -Wait -PassThru; exit $p.ExitCode }} catch {{ exit 1223 }}",
        exe = q(&exe),
        flag = APPLY_FLAG,
        file = q(&file),
    );
    let status = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .status();
    let _ = std::fs::remove_file(&file);
    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) if s.code() == Some(1223) => Err(AppError::Validation(
            "the hosts file was not updated: administrator permission was declined".into(),
        )),
        Ok(s) => Err(AppError::Internal(format!("the hosts file update failed (exit code {:?})", s.code()))),
        Err(e) => Err(AppError::Internal(format!("could not start the hosts file update: {e}"))),
    }
}

#[cfg(not(windows))]
fn write_elevated(_lines: &[String]) -> AppResult<()> {
    Err(AppError::Validation(format!(
        "no permission to write {}; add the entries yourself or run Remotix with the needed rights",
        hosts_path().display()
    )))
}

/// Entry point of the elevated helper process: `remotix --apply-hosts <file>`.
/// Returns the exit code when the flag is present, `None` otherwise.
pub fn apply_from_args(args: impl IntoIterator<Item = String>) -> Option<i32> {
    let mut args = args.into_iter();
    args.find(|a| a == APPLY_FLAG)?;
    let Some(file) = args.next() else { return Some(2) };
    Some(match apply_file(Path::new(&file)) {
        Ok(()) => 0,
        Err(_) => 1,
    })
}

fn apply_file(file: &Path) -> Result<(), String> {
    let text = std::fs::read_to_string(file).map_err(|e| e.to_string())?;
    let lines: Vec<String> = text.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect();
    if lines.len() > MAX_ENTRIES || lines.iter().any(|l| parse_entry_line(l).is_none()) {
        return Err("refusing invalid hosts entries".into());
    }
    let path = hosts_path();
    let current = read_hosts(&path).map_err(|e| e.to_string())?;
    std::fs::write(&path, replace_block(&current, &lines)).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(host: &str, ip: &str) -> HostsEntry {
        HostsEntry { hostname: host.into(), ip: ip.into() }
    }

    #[test]
    fn adds_replaces_and_removes_only_the_managed_block() {
        let original = "# Copyright\r\n127.0.0.1 my.dev\r\n";
        let lines = render_entries(&[entry("ifg-portainer.local", "127.0.0.5")]);
        let added = replace_block(original, &lines);
        assert_eq!(
            added,
            format!("# Copyright\r\n127.0.0.1 my.dev\r\n\r\n{BEGIN_MARKER}\r\n127.0.0.5\tifg-portainer.local\r\n{END_MARKER}\r\n")
        );
        assert_eq!(managed_lines(&added), lines);

        let replaced = replace_block(&added, &render_entries(&[entry("a.local", "127.0.0.6")]));
        assert!(replaced.contains("127.0.0.6\ta.local") && !replaced.contains("ifg-portainer"));
        assert!(replaced.starts_with("# Copyright\r\n127.0.0.1 my.dev\r\n"));

        assert_eq!(replace_block(&replaced, &[]), original);
    }

    #[test]
    fn keeps_lf_files_lf() {
        let out = replace_block("127.0.0.1 localhost\n", &["127.0.0.5\tx.local".into()]);
        assert!(!out.contains('\r'));
    }

    #[test]
    fn accepts_only_plain_entries() {
        assert_eq!(parse_entry_line("127.0.0.5\tifg-portainer.local"), Some(entry("ifg-portainer.local", "127.0.0.5")));
        assert!(parse_entry_line("::1 portainer.local").is_some());
        assert!(parse_entry_line("127.0.0.5 a.local b.local").is_none());
        assert!(parse_entry_line("not-an-ip a.local").is_none());
        assert!(parse_entry_line("127.0.0.5 localhost").is_none());
        assert!(parse_entry_line("127.0.0.5 bad_name.local").is_none());
        assert!(parse_entry_line("127.0.0.5 10.0.0.1").is_none());
    }

    #[test]
    fn apply_flag_is_ignored_without_it() {
        assert_eq!(apply_from_args(vec!["remotix".to_string()]), None);
        assert_eq!(apply_from_args(vec!["remotix".to_string(), APPLY_FLAG.to_string()]), Some(2));
    }
}
