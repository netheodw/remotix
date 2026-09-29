//! Import of hosts, credentials, and port-forwarding rules from a Termius
//! JSON export. Groups, tags, proxies, and known hosts in the export are
//! ignored.
//!
//! The export holds plaintext credentials, so it is only ever read on this
//! side of the IPC boundary: `preview` returns non-secret fields for the UI
//! to show, and `apply` re-reads the file and encrypts credentials straight
//! into the vault.

use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::domain::{AuthKind, AuthSecretInput, ForwardType, NewPortForwardInput, NewSshProfileInput, SshProfile};
use crate::error::{AppError, AppResult};
use crate::repositories::{ForwardRepository, ProfileRepository};
use crate::validation::{validate_bind_address, validate_display_name, validate_hostname, validate_username};
use crate::vault::VaultManager;

const DEFAULT_SSH_PORT: u16 = 22;
const LOOPBACK: &str = "127.0.0.1";

// --- Export file shape (only the fields that are imported) ---

#[derive(Deserialize)]
struct ExportFile {
    hosts: Vec<ExportHost>,
    #[serde(default)]
    keys: Vec<ExportKey>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExportHost {
    alias: String,
    label: String,
    address: String,
    port: Option<i64>,
    username: String,
    auth: String,
    password: String,
    identity_file: String,
    key: Option<ExportHostKey>,
    forwards: Vec<ExportForward>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExportHostKey {
    label: String,
    has_passphrase: bool,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExportKey {
    label: String,
    passphrase: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExportForward {
    label: String,
    #[serde(rename = "type")]
    forward_type: String,
    bound_address: String,
    local_port: Option<i64>,
    target_host: String,
    target_port: Option<i64>,
}

// --- Preview / report DTOs (never carry secret material) ---

#[derive(Debug, Clone, Serialize)]
pub struct TermiusPreview {
    pub hosts: Vec<TermiusHostPreview>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TermiusHostPreview {
    /// Position in the export's `hosts` array; `apply` selects hosts by it.
    pub index: usize,
    pub name: String,
    pub hostname: String,
    pub port: u16,
    pub username: String,
    pub authentication_kind: Option<AuthKind>,
    /// Why the host cannot be imported, if it cannot.
    pub problem: Option<String>,
    /// Name of an existing profile with the same hostname, port, and username.
    pub existing_profile_name: Option<String>,
    pub forwards: Vec<TermiusForwardPreview>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TermiusForwardPreview {
    pub name: String,
    pub forward_type: Option<ForwardType>,
    pub bind_address: String,
    pub listen_port: u16,
    pub target_hostname: Option<String>,
    pub target_port: Option<u16>,
    /// Why the forward cannot be imported, if it cannot. Listener conflicts
    /// depend on which hosts are selected, so they are only reported by `apply`.
    pub problem: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TermiusImportReport {
    pub profiles_created: usize,
    pub forwards_created: usize,
    pub skipped: Vec<TermiusSkippedItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TermiusSkippedItem {
    pub name: String,
    pub reason: String,
}

// --- Entry points ---

pub fn preview(conn: &Connection, path: &Path) -> AppResult<TermiusPreview> {
    let export = read_export(path)?;
    let existing = ProfileRepository::list(conn)?;
    let hosts = plan(&export, export_dir(path))
        .into_iter()
        .enumerate()
        .map(|(index, host)| {
            let existing_profile_name = existing
                .iter()
                .find(|p| p.hostname.eq_ignore_ascii_case(&host.hostname) && p.port == host.port && p.username == host.username)
                .map(|p| p.name.clone());
            TermiusHostPreview {
                index,
                authentication_kind: host.secret.as_ref().ok().map(auth_kind_of),
                problem: host.secret.err(),
                name: host.name,
                hostname: host.hostname,
                port: host.port,
                username: host.username,
                existing_profile_name,
                forwards: host.forwards,
            }
        })
        .collect();
    Ok(TermiusPreview { hosts })
}

/// Creates a profile (ungrouped) for each selected host plus its port
/// forwards. Individual hosts and forwards that fail validation are skipped
/// and listed in the report rather than aborting the whole import.
pub fn apply(
    conn: &Connection,
    vault: &VaultManager,
    path: &Path,
    host_indices: &[usize],
) -> AppResult<TermiusImportReport> {
    if !vault.is_unlocked() {
        return Err(AppError::VaultLocked);
    }
    let export = read_export(path)?;
    let mut selected = host_indices.to_vec();
    selected.sort_unstable();
    selected.dedup();
    if selected.last().is_some_and(|&i| i >= export.hosts.len()) {
        return Err(AppError::Validation(
            "the export file changed since it was previewed - open it again".into(),
        ));
    }

    let mut report = TermiusImportReport { profiles_created: 0, forwards_created: 0, skipped: Vec::new() };
    let hosts = plan(&export, export_dir(path))
        .into_iter()
        .enumerate()
        .filter_map(|(i, host)| selected.binary_search(&i).is_ok().then_some(host));

    for mut host in hosts {
        let host_name = host.name.clone();
        let forwards = std::mem::take(&mut host.forwards);
        let profile = match create_profile(conn, vault, host) {
            Ok(profile) => profile,
            Err(reason) => {
                report.skipped.push(TermiusSkippedItem { name: host_name, reason });
                continue;
            }
        };
        report.profiles_created += 1;

        for forward in forwards {
            let name = format!("{} ({host_name})", forward.name);
            match create_forward(conn, &profile.id, forward) {
                Ok(()) => report.forwards_created += 1,
                Err(reason) => report.skipped.push(TermiusSkippedItem { name, reason }),
            }
        }
    }
    Ok(report)
}

// --- Planning: export -> validated, normalized import items ---

struct PlannedHost {
    name: String,
    hostname: String,
    port: u16,
    username: String,
    /// `Err` carries the reason the host cannot be imported - an invalid
    /// field or missing/unreadable credentials.
    secret: Result<AuthSecretInput, String>,
    forwards: Vec<TermiusForwardPreview>,
}

fn plan(export: &ExportFile, export_dir: &Path) -> Vec<PlannedHost> {
    export.hosts.iter().map(|host| plan_host(host, &export.keys, export_dir)).collect()
}

fn plan_host(host: &ExportHost, keys: &[ExportKey], export_dir: &Path) -> PlannedHost {
    let name = [&host.label, &host.alias, &host.address]
        .into_iter()
        .map(|s| s.trim())
        .find(|s| !s.is_empty())
        .unwrap_or_default()
        .to_string();
    let hostname = host.address.trim().to_string();
    let username = host.username.trim().to_string();
    let port = match host.port {
        None | Some(0) => Some(DEFAULT_SSH_PORT),
        Some(p) => to_port(Some(p)),
    };

    let invalid = validate_display_name(&name)
        .and_then(|_| validate_hostname(&hostname))
        .and_then(|_| validate_username(&username))
        .err()
        .map(|e| reason(&e))
        .or_else(|| port.is_none().then(|| "port must be between 1 and 65535".to_string()));
    let secret = match invalid {
        Some(problem) => Err(problem),
        None => resolve_secret(host, keys, export_dir),
    };

    PlannedHost {
        forwards: host.forwards.iter().map(|f| plan_forward(f, &name)).collect(),
        name,
        hostname,
        port: port.unwrap_or(DEFAULT_SSH_PORT),
        username,
        secret,
    }
}

/// Uses whichever credential the host was set to authenticate with in
/// Termius, falling back to the other one if that is missing or unreadable.
fn resolve_secret(host: &ExportHost, keys: &[ExportKey], export_dir: &Path) -> Result<AuthSecretInput, String> {
    let prefers_key = host.auth.eq_ignore_ascii_case("publickey");
    let has_key = host.key.is_some() || !host.identity_file.trim().is_empty();
    let key = has_key.then(|| resolve_key(host, keys, export_dir));
    let password = (!host.password.is_empty()).then(|| AuthSecretInput::Password { password: host.password.clone() });

    match (key, password) {
        (Some(Ok(key)), Some(_)) if prefers_key => Ok(key),
        (_, Some(password)) => Ok(password),
        (Some(key), None) => key,
        (None, None) => Err("the export has no password or private key for this host".into()),
    }
}

fn resolve_key(host: &ExportHost, keys: &[ExportKey], export_dir: &Path) -> Result<AuthSecretInput, String> {
    let key_label = host.key.as_ref().map(|k| k.label.as_str()).unwrap_or_default();
    let private_key = key_file_candidates(&host.identity_file, key_label, export_dir)
        .into_iter()
        .find_map(|path| std::fs::read_to_string(path).ok().filter(|c| !c.trim().is_empty()))
        .ok_or_else(|| "the private key file referenced by the export could not be read".to_string())?;

    let passphrase = keys
        .iter()
        .find(|k| !key_label.is_empty() && k.label == key_label)
        .map(|k| k.passphrase.clone())
        .unwrap_or_default();
    if !passphrase.is_empty() {
        Ok(AuthSecretInput::PrivateKeyPassphrase { private_key, passphrase })
    } else if host.key.as_ref().is_some_and(|k| k.has_passphrase) {
        Err("the private key is passphrase-protected but the export has no passphrase for it".into())
    } else {
        Ok(AuthSecretInput::PrivateKey { private_key })
    }
}

/// The path recorded at export time, then the same file name under
/// `<export dir>/keys/`, so an export folder that was moved or copied to
/// another machine still finds its keys. Only bare file names are joined
/// onto the keys directory.
fn key_file_candidates(identity_file: &str, key_label: &str, export_dir: &Path) -> Vec<PathBuf> {
    let identity_file = identity_file.trim();
    let mut candidates = Vec::new();
    if !identity_file.is_empty() {
        candidates.push(PathBuf::from(identity_file));
    }
    let keys_dir = export_dir.join("keys");
    for name in [file_name_of(identity_file), file_name_of(key_label)].into_iter().flatten() {
        candidates.push(keys_dir.join(name));
    }
    candidates
}

/// Last path component, splitting on both separators so a Windows path in
/// the export still resolves on Linux (and vice versa).
fn file_name_of(path: &str) -> Option<&str> {
    path.rsplit(|c| c == '/' || c == '\\').next().filter(|n| !n.is_empty() && *n != "." && *n != "..")
}

fn plan_forward(forward: &ExportForward, host_name: &str) -> TermiusForwardPreview {
    let forward_type = match forward.forward_type.trim().to_ascii_lowercase().as_str() {
        "local" => Some(ForwardType::Local),
        "remote" => Some(ForwardType::Remote),
        "dynamic" => Some(ForwardType::Dynamic),
        _ => None,
    };
    let is_dynamic = forward_type == Some(ForwardType::Dynamic);
    let listen_port = to_port(forward.local_port);
    let bind_address = normalize_bind_address(&forward.bound_address);
    let target_hostname = Some(forward.target_host.trim().to_string()).filter(|h| !h.is_empty() && !is_dynamic);
    let target_port = to_port(forward.target_port).filter(|_| !is_dynamic);
    let name = match forward.label.trim() {
        "" => format!("{host_name} :{}", listen_port.unwrap_or_default()),
        label => label.to_string(),
    };

    let problem = if forward_type.is_none() {
        Some(format!("unsupported forward type \"{}\"", forward.forward_type))
    } else if let Err(e) = validate_bind_address(&bind_address) {
        Some(reason(&e))
    } else if listen_port.is_none() {
        Some("listen port must be between 1 and 65535".into())
    } else if !is_dynamic && target_hostname.is_none() {
        Some("target hostname is required".into())
    } else if !is_dynamic && target_port.is_none() {
        Some("target port must be between 1 and 65535".into())
    } else {
        None
    };

    TermiusForwardPreview {
        name,
        forward_type,
        bind_address,
        listen_port: listen_port.unwrap_or_default(),
        target_hostname,
        target_port,
        problem,
    }
}

/// Termius accepts `localhost` or an empty bind address; Remotix requires a
/// literal IP, so both become loopback.
fn normalize_bind_address(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() || value.eq_ignore_ascii_case("localhost") {
        LOOPBACK.to_string()
    } else {
        value.to_string()
    }
}

// --- Apply helpers ---

fn create_profile(conn: &Connection, vault: &VaultManager, host: PlannedHost) -> Result<SshProfile, String> {
    let secret = host.secret?;
    ProfileRepository::create(
        conn,
        vault,
        NewSshProfileInput {
            terminal_group_id: None,
            name: host.name,
            hostname: host.hostname,
            port: host.port,
            username: host.username,
            terminal_type: None,
            secret,
        },
    )
    .map_err(|e| reason(&e))
}

fn create_forward(conn: &Connection, profile_id: &str, forward: TermiusForwardPreview) -> Result<(), String> {
    if let Some(problem) = forward.problem {
        return Err(problem);
    }
    let forward_type = forward.forward_type.ok_or_else(|| "unsupported forward type".to_string())?;
    ForwardRepository::create(
        conn,
        NewPortForwardInput {
            port_forward_group_id: None,
            ssh_profile_id: profile_id.to_string(),
            name: forward.name,
            forward_type,
            bind_address: forward.bind_address,
            listen_port: forward.listen_port,
            target_hostname: forward.target_hostname,
            target_port: forward.target_port,
            start_on_launch: false,
        },
    )
    .map(|_| ())
    .map_err(|e| reason(&e))
}

// --- Shared helpers ---

fn read_export(path: &Path) -> AppResult<ExportFile> {
    let contents = std::fs::read_to_string(path)
        .map_err(|_| AppError::Validation("could not read the selected file".into()))?;
    parse_export(&contents)
}

fn parse_export(contents: &str) -> AppResult<ExportFile> {
    // serde's message can quote values from the file - which include
    // passwords - so only the error position is surfaced.
    serde_json::from_str(contents).map_err(|e| {
        AppError::Validation(format!(
            "the selected file is not a Termius export (line {}, column {})",
            e.line(),
            e.column()
        ))
    })
}

fn export_dir(path: &Path) -> &Path {
    path.parent().unwrap_or_else(|| Path::new("."))
}

fn to_port(value: Option<i64>) -> Option<u16> {
    value.and_then(|p| u16::try_from(p).ok()).filter(|p| *p != 0)
}

fn auth_kind_of(secret: &AuthSecretInput) -> AuthKind {
    match secret {
        AuthSecretInput::PrivateKey { .. } => AuthKind::PrivateKey,
        AuthSecretInput::PrivateKeyPassphrase { .. } => AuthKind::PrivateKeyPassphrase,
        AuthSecretInput::Password { .. } | AuthSecretInput::Unchanged | AuthSecretInput::Clear => AuthKind::Password,
    }
}

fn reason(e: &AppError) -> String {
    match e {
        AppError::Validation(message) => message.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::*;
    use crate::testing::{create_password_profile, open_temp_vault};

    fn write_export(dir: &Path, value: Value) -> PathBuf {
        let path = dir.join("export.json");
        std::fs::write(&path, value.to_string()).unwrap();
        path
    }

    fn password_host(label: &str, address: &str, forwards: Value) -> Value {
        json!({
            "alias": label, "label": label, "address": address, "port": 22, "username": "ops",
            "auth": "password", "password": "pw-1", "identity_file": "", "key": null, "forwards": forwards
        })
    }

    fn local_forward(label: &str, bind: &str, port: u16, target_port: u16) -> Value {
        json!({
            "label": label, "type": "Local", "bound_address": bind, "local_port": port,
            "target_host": "localhost", "target_port": target_port
        })
    }

    fn plan_json(dir: &Path, value: Value) -> Vec<PlannedHost> {
        plan(&parse_export(&value.to_string()).unwrap(), dir)
    }

    #[test]
    fn plans_password_host_and_normalizes_localhost_bind() {
        let dir = tempfile::tempdir().unwrap();
        let hosts = plan_json(
            dir.path(),
            json!({ "hosts": [password_host("Alpha", "10.0.0.1", json!([local_forward("DB", "localhost", 15432, 5432)]))] }),
        );
        assert!(matches!(&hosts[0].secret, Ok(AuthSecretInput::Password { password }) if password == "pw-1"));
        let forward = &hosts[0].forwards[0];
        assert_eq!(forward.forward_type, Some(ForwardType::Local));
        assert_eq!(forward.bind_address, "127.0.0.1");
        assert_eq!(forward.target_hostname.as_deref(), Some("localhost"));
        assert!(forward.problem.is_none());
    }

    #[test]
    fn maps_remote_and_dynamic_forwards() {
        let dir = tempfile::tempdir().unwrap();
        let forwards = json!([
            { "label": "Old DB", "type": "Remote", "bound_address": "127.0.0.1", "local_port": 5432,
              "target_host": "localhost", "target_port": 9010 },
            { "label": "SOCKS", "type": "Dynamic", "bound_address": "", "local_port": 1080,
              "target_host": "", "target_port": 0 }
        ]);
        let hosts = plan_json(dir.path(), json!({ "hosts": [password_host("Old", "old.example.com", forwards)] }));
        let remote = &hosts[0].forwards[0];
        assert_eq!(remote.forward_type, Some(ForwardType::Remote));
        assert_eq!((remote.listen_port, remote.target_port), (5432, Some(9010)));
        assert!(remote.problem.is_none());
        let dynamic = &hosts[0].forwards[1];
        assert_eq!(dynamic.forward_type, Some(ForwardType::Dynamic));
        assert_eq!((dynamic.target_hostname.as_deref(), dynamic.target_port), (None, None));
        assert_eq!(dynamic.bind_address, "127.0.0.1");
        assert!(dynamic.problem.is_none());
    }

    #[test]
    fn finds_key_next_to_the_export_and_attaches_its_passphrase() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("keys")).unwrap();
        std::fs::write(dir.path().join("keys").join("deploy.pem"), "-----BEGIN KEY-----abc-----END KEY-----").unwrap();
        let host = json!({
            "label": "Jump", "address": "10.0.0.9", "port": 22, "username": "deploy", "auth": "publicKey",
            "password": "", "identity_file": "Z:\\moved-away\\keys\\deploy.pem",
            "key": { "label": "deploy.pem", "has_passphrase": true }, "forwards": []
        });
        let hosts = plan_json(
            dir.path(),
            json!({ "hosts": [host], "keys": [{ "label": "deploy.pem", "passphrase": "hunter2" }] }),
        );
        match &hosts[0].secret {
            Ok(AuthSecretInput::PrivateKeyPassphrase { private_key, passphrase }) => {
                assert!(private_key.contains("BEGIN KEY"));
                assert_eq!(passphrase, "hunter2");
            }
            _ => panic!("expected private key + passphrase"),
        }
    }

    #[test]
    fn flags_hosts_without_usable_credentials_or_with_invalid_fields() {
        let dir = tempfile::tempdir().unwrap();
        let mut no_credentials = password_host("NoCreds", "10.0.0.1", json!([]));
        no_credentials["password"] = json!("");
        let bad_address = password_host("BadAddress", "has a space", json!([]));
        let mut missing_key_with_password = password_host("KeyFallback", "10.0.0.2", json!([]));
        missing_key_with_password["auth"] = json!("publicKey");
        missing_key_with_password["identity_file"] = json!("Z:\\nowhere\\missing.pem");

        let hosts = plan_json(dir.path(), json!({ "hosts": [no_credentials, bad_address, missing_key_with_password] }));
        assert!(hosts[0].secret.as_ref().is_err_and(|e| e.contains("no password or private key")));
        assert!(hosts[1].secret.is_err());
        assert!(matches!(hosts[2].secret, Ok(AuthSecretInput::Password { .. })));
    }

    #[test]
    fn parse_errors_do_not_echo_file_contents() {
        // Export structs deliberately don't implement Debug (they hold passwords), so no `unwrap_err`.
        let Err(err) = parse_export(r#"{"hosts": [{"password": "leaky-secret", "port": "not-a-number"}]}"#) else {
            panic!("expected a parse error");
        };
        let text = format!("{err:?}");
        assert!(!text.contains("leaky-secret") && !text.contains("not-a-number"));
    }

    #[test]
    fn preview_flags_existing_profiles_and_carries_no_secrets() {
        let fixture = open_temp_vault("correct-password-1");
        create_password_profile(&fixture, "Existing", "10.0.0.1", "ops", "other");
        let dir = tempfile::tempdir().unwrap();
        let path = write_export(dir.path(), json!({ "hosts": [password_host("Alpha", "10.0.0.1", json!([]))] }));

        let conn = fixture.pool.get().unwrap();
        let result = preview(&conn, &path).unwrap();
        assert_eq!(result.hosts[0].existing_profile_name.as_deref(), Some("Existing"));
        assert_eq!(result.hosts[0].authentication_kind, Some(AuthKind::Password));
        assert!(!serde_json::to_string(&result).unwrap().contains("pw-1"));
    }

    #[test]
    fn apply_imports_selected_hosts_and_skips_conflicting_listeners() {
        let fixture = open_temp_vault("correct-password-1");
        let dir = tempfile::tempdir().unwrap();
        let path = write_export(
            dir.path(),
            json!({ "hosts": [
                password_host("Glyfada", "10.0.0.1", json!([local_forward("Logs", "127.0.0.1", 10000, 5555)])),
                password_host("Santorini", "10.0.0.2", json!([
                    local_forward("Redis", "localhost", 10000, 6379),
                    local_forward("DB", "127.0.0.1", 10002, 5432)
                ])),
                password_host("Unselected", "10.0.0.3", json!([]))
            ]}),
        );

        let conn = fixture.pool.get().unwrap();
        let report = apply(&conn, &fixture.vault, &path, &[1, 0, 1]).unwrap();
        assert_eq!((report.profiles_created, report.forwards_created), (2, 2));
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(report.skipped[0].name, "Redis (Santorini)");
        assert!(report.skipped[0].reason.contains("127.0.0.1:10000"));

        let profiles = ProfileRepository::list(&conn).unwrap();
        assert_eq!(profiles.len(), 2);
        let santorini = profiles.iter().find(|p| p.name == "Santorini").unwrap();
        let material = ProfileRepository::load_auth_material(&conn, &fixture.vault, &santorini.id).unwrap();
        assert!(matches!(material, crate::ssh::session::AuthMaterial::Password(p) if p == "pw-1"));
    }

    #[test]
    fn apply_rejects_out_of_range_indices_and_a_locked_vault() {
        let fixture = open_temp_vault("correct-password-1");
        let dir = tempfile::tempdir().unwrap();
        let path = write_export(dir.path(), json!({ "hosts": [password_host("Alpha", "10.0.0.1", json!([]))] }));
        let conn = fixture.pool.get().unwrap();

        assert!(matches!(apply(&conn, &fixture.vault, &path, &[3]), Err(AppError::Validation(_))));
        fixture.vault.lock();
        assert!(matches!(apply(&conn, &fixture.vault, &path, &[0]), Err(AppError::VaultLocked)));
    }
}
