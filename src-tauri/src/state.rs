use std::sync::{Arc, Mutex};

use crate::db::Pool;
use crate::ssh::SshManager;
use crate::vault::VaultManager;

pub struct AppState {
    pub db: Pool,
    pub vault: Arc<VaultManager>,
    pub ssh: Arc<SshManager>,
    /// Profile id from a desktop shortcut (`--open-profile`), opened by the
    /// UI once the vault is unlocked.
    pub pending_launch_profile: Mutex<Option<String>>,
}

impl AppState {
    pub fn new(db: Pool) -> Self {
        Self {
            db,
            vault: Arc::new(VaultManager::new()),
            ssh: Arc::new(SshManager::new()),
            pending_launch_profile: Mutex::new(None),
        }
    }
}
