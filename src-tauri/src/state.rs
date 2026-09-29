use std::sync::Arc;

use crate::db::Pool;
use crate::ssh::SshManager;
use crate::vault::VaultManager;

pub struct AppState {
    pub db: Pool,
    pub vault: Arc<VaultManager>,
    pub ssh: Arc<SshManager>,
}

impl AppState {
    pub fn new(db: Pool) -> Self {
        Self {
            db,
            vault: Arc::new(VaultManager::new()),
            ssh: Arc::new(SshManager::new()),
        }
    }
}
