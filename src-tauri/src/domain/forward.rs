use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ForwardType {
    Local,
    Remote,
    Dynamic,
}

impl ForwardType {
    pub fn as_db_str(&self) -> &'static str {
        match self {
            ForwardType::Local => "local",
            ForwardType::Remote => "remote",
            ForwardType::Dynamic => "dynamic",
        }
    }

    pub fn from_db_str(s: &str) -> Option<Self> {
        match s {
            "local" => Some(ForwardType::Local),
            "remote" => Some(ForwardType::Remote),
            "dynamic" => Some(ForwardType::Dynamic),
            _ => None,
        }
    }
}

/// A local forward whose target is an absolute path (and no port) goes to a
/// Unix domain socket on the server, like `ssh -L 23750:/var/run/docker.sock`.
pub fn is_socket_target(forward_type: ForwardType, target_hostname: Option<&str>) -> bool {
    forward_type == ForwardType::Local && target_hostname.is_some_and(|h| h.trim_start().starts_with('/'))
}

#[derive(Debug, Clone, Serialize)]
pub struct PortForward {
    pub id: String,
    pub port_forward_group_id: Option<String>,
    /// `None` means the profile it was created against has since been
    /// deleted; the UI surfaces this as "Needs Profile" rather than
    /// silently rebinding to a different profile.
    pub ssh_profile_id: Option<String>,
    pub name: String,
    pub forward_type: ForwardType,
    pub bind_address: String,
    pub listen_port: u16,
    pub target_hostname: Option<String>,
    pub target_port: Option<u16>,
    pub start_on_launch: bool,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
    /// Free text, set with `forwards_set_notes`.
    pub notes: String,
    /// Name mapped to the bind address in the hosts file, e.g.
    /// `ifg-portainer.local`. Set with `forwards_set_local_hostname`.
    pub local_hostname: Option<String>,
}

impl PortForward {
    /// The remote Unix socket path, when this is a local forward to a socket.
    pub fn target_socket_path(&self) -> Option<&str> {
        let host = self.target_hostname.as_deref()?;
        is_socket_target(self.forward_type, Some(host)).then(|| host.trim())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewPortForwardInput {
    pub port_forward_group_id: Option<String>,
    pub ssh_profile_id: String,
    pub name: String,
    pub forward_type: ForwardType,
    pub bind_address: String,
    pub listen_port: u16,
    pub target_hostname: Option<String>,
    pub target_port: Option<u16>,
    pub start_on_launch: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdatePortForwardInput {
    pub id: String,
    pub port_forward_group_id: Option<String>,
    pub ssh_profile_id: String,
    pub name: String,
    pub forward_type: ForwardType,
    pub bind_address: String,
    pub listen_port: u16,
    pub target_hostname: Option<String>,
    pub target_port: Option<u16>,
    pub start_on_launch: bool,
}

/// Runtime (non-persisted) state of a forward, tracked by the ssh manager
/// and pushed to the frontend as it changes.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ForwardRuntimeStatus {
    Stopped,
    Starting,
    VerifyingHost,
    Authenticating,
    Active { connection_count: u32 },
    Reconnecting { attempt: u32, next_retry_secs: u32 },
    Failed { message: String },
    NeedsProfile,
}
