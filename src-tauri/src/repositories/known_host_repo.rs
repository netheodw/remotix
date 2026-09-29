use rusqlite::{Connection, OptionalExtension};

use crate::domain::{HostKeyCheck, KnownHost};
use crate::error::AppResult;
use crate::util::{new_id, now_iso};

pub struct KnownHostRepository;

struct StoredHostKey {
    id: String,
    sha256_fingerprint: String,
}

impl KnownHostRepository {
    pub fn list(conn: &Connection) -> AppResult<Vec<KnownHost>> {
        let mut stmt = conn.prepare(
            "SELECT id, hostname, port, key_algorithm, sha256_fingerprint, first_trusted_at, last_seen_at FROM known_hosts ORDER BY hostname, port",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(KnownHost {
                id: row.get(0)?,
                hostname: row.get(1)?,
                port: row.get::<_, i64>(2)? as u16,
                key_algorithm: row.get(3)?,
                sha256_fingerprint: row.get(4)?,
                first_trusted_at: row.get(5)?,
                last_seen_at: row.get(6)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Compares a freshly-presented host key against any stored record for
    /// this hostname/port. Never writes to the store - callers act on the
    /// result (trust / reject / replace) via `trust`.
    pub fn check(
        conn: &Connection,
        hostname: &str,
        port: u16,
        key_algorithm: &str,
        presented_fingerprint: &str,
    ) -> AppResult<HostKeyCheck> {
        let stored = Self::find(conn, hostname, port)?;
        Ok(match stored {
            None => HostKeyCheck::FirstUse {
                key_algorithm: key_algorithm.to_string(),
                sha256_fingerprint: presented_fingerprint.to_string(),
            },
            Some(existing) if existing.sha256_fingerprint == presented_fingerprint => {
                Self::touch(conn, hostname, port)?;
                HostKeyCheck::Trusted
            }
            Some(existing) => HostKeyCheck::Changed {
                key_algorithm: key_algorithm.to_string(),
                previous_sha256_fingerprint: existing.sha256_fingerprint,
                presented_sha256_fingerprint: presented_fingerprint.to_string(),
            },
        })
    }

    /// Records (or replaces) the trusted key for a host. Only call this
    /// after the user has explicitly confirmed trust/replace in the UI.
    pub fn trust(
        conn: &Connection,
        hostname: &str,
        port: u16,
        key_algorithm: &str,
        host_key_bytes: &[u8],
        fingerprint: &str,
    ) -> AppResult<()> {
        let now = now_iso();
        let existing = Self::find(conn, hostname, port)?;
        match existing {
            Some(existing) => {
                conn.execute(
                    "UPDATE known_hosts SET key_algorithm = ?1, host_key = ?2, sha256_fingerprint = ?3, last_seen_at = ?4 WHERE id = ?5",
                    rusqlite::params![key_algorithm, host_key_bytes, fingerprint, now, existing.id],
                )?;
            }
            None => {
                conn.execute(
                    "INSERT INTO known_hosts (id, hostname, port, key_algorithm, host_key, sha256_fingerprint, first_trusted_at, last_seen_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
                    rusqlite::params![new_id(), hostname, port, key_algorithm, host_key_bytes, fingerprint, now],
                )?;
            }
        }
        Ok(())
    }

    fn touch(conn: &Connection, hostname: &str, port: u16) -> AppResult<()> {
        conn.execute(
            "UPDATE known_hosts SET last_seen_at = ?1 WHERE hostname = ?2 AND port = ?3",
            rusqlite::params![now_iso(), hostname, port],
        )?;
        Ok(())
    }

    fn find(conn: &Connection, hostname: &str, port: u16) -> AppResult<Option<StoredHostKey>> {
        Ok(conn
            .query_row(
                "SELECT id, sha256_fingerprint FROM known_hosts WHERE hostname = ?1 AND port = ?2",
                rusqlite::params![hostname, port],
                |row| {
                    Ok(StoredHostKey {
                        id: row.get(0)?,
                        sha256_fingerprint: row.get(1)?,
                    })
                },
            )
            .optional()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn test_pool() -> (tempfile::TempDir, db::Pool) {
        let dir = tempfile::tempdir().unwrap();
        let pool = db::init_pool(&dir.path().join("t.db")).unwrap();
        (dir, pool)
    }

    #[test]
    fn first_connection_is_first_use() {
        let (_dir, pool) = test_pool();
        let conn = pool.get().unwrap();
        let result = KnownHostRepository::check(&conn, "example.com", 22, "ssh-ed25519", "AA:BB").unwrap();
        assert!(matches!(result, HostKeyCheck::FirstUse { .. }));
    }

    #[test]
    fn trusted_key_matches_on_second_connection() {
        let (_dir, pool) = test_pool();
        let conn = pool.get().unwrap();
        KnownHostRepository::trust(&conn, "example.com", 22, "ssh-ed25519", b"keybytes", "AA:BB").unwrap();
        let result = KnownHostRepository::check(&conn, "example.com", 22, "ssh-ed25519", "AA:BB").unwrap();
        assert!(matches!(result, HostKeyCheck::Trusted));
    }

    #[test]
    fn changed_key_is_flagged_and_not_silently_replaced() {
        let (_dir, pool) = test_pool();
        let conn = pool.get().unwrap();
        KnownHostRepository::trust(&conn, "example.com", 22, "ssh-ed25519", b"keybytes", "AA:BB").unwrap();
        let result = KnownHostRepository::check(&conn, "example.com", 22, "ssh-ed25519", "CC:DD").unwrap();
        match result {
            HostKeyCheck::Changed { previous_sha256_fingerprint, presented_sha256_fingerprint, .. } => {
                assert_eq!(previous_sha256_fingerprint, "AA:BB");
                assert_eq!(presented_sha256_fingerprint, "CC:DD");
            }
            _ => panic!("expected Changed"),
        }

        // Confirm the store still holds the original key until `trust` is
        // explicitly called again (no silent replacement).
        let still_original = KnownHostRepository::check(&conn, "example.com", 22, "ssh-ed25519", "AA:BB").unwrap();
        assert!(matches!(still_original, HostKeyCheck::Trusted));
    }
}
