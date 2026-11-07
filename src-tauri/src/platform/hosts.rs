//! Local hostnames of port forwards (`ifg-portainer.localhost`,
//! `ifg-portainer.local`).
//!
//! Remotix only *reads* the OS hosts file, to tell the user whether a name
//! is already mapped. It never writes it and never elevates itself: in 0.2.0
//! it did (a hidden PowerShell relaunching the exe as administrator to edit
//! the hosts file), and Bitdefender treated that as malware, killed the app
//! and quarantined it. Names ending in `.localhost` need no hosts entry at
//! all in browsers; for any other name the UI shows the line to add by hand.

use std::net::IpAddr;
use std::path::PathBuf;

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

/// The address `hostname` is mapped to in hosts-file `content`, if any.
pub fn lookup(content: &str, hostname: &str) -> Option<String> {
    content.lines().find_map(|line| {
        let line = line.split('#').next().unwrap_or("");
        let mut parts = line.split_whitespace();
        let ip = parts.next()?;
        ip.parse::<IpAddr>().ok()?;
        parts.any(|h| h.eq_ignore_ascii_case(hostname)).then(|| ip.to_string())
    })
}

/// Reads the hosts file (no rights needed) and looks `hostname` up.
pub fn lookup_in_hosts_file(hostname: &str) -> Option<String> {
    let content = std::fs::read_to_string(hosts_path()).ok()?;
    lookup(&content, hostname)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_mapped_names_and_skips_comments() {
        let hosts = "# 127.0.0.9 commented.local\r\n127.0.0.5\tifg-portainer.local other.local # note\r\n::1 v6.local\r\n";
        assert_eq!(lookup(hosts, "IFG-Portainer.local").as_deref(), Some("127.0.0.5"));
        assert_eq!(lookup(hosts, "other.local").as_deref(), Some("127.0.0.5"));
        assert_eq!(lookup(hosts, "v6.local").as_deref(), Some("::1"));
        assert_eq!(lookup(hosts, "commented.local"), None);
        assert_eq!(lookup(hosts, "missing.local"), None);
    }

    #[test]
    fn validates_names() {
        assert!(is_valid_local_hostname("ifg-portainer.local"));
        assert!(is_valid_local_hostname("ifg-portainer.localhost"));
        assert!(!is_valid_local_hostname("localhost"));
        assert!(!is_valid_local_hostname("bad name.local"));
        assert!(!is_valid_local_hostname("bad_name.local"));
        assert!(!is_valid_local_hostname("10.0.0.1"));
        assert!(!is_valid_local_hostname("-x.local"));
    }
}
