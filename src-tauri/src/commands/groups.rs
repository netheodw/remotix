use serde::Deserialize;
use tauri::State;

use crate::domain::{Group, NewGroupInput, UpdateGroupInput};
use crate::error::{AppError, AppResult};
use crate::repositories::{GroupDeleteStrategy, GroupRepository};
use crate::state::AppState;

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeleteGroupStrategyDto {
    ReassignToParent,
    DeleteSubtree,
}

impl From<DeleteGroupStrategyDto> for GroupDeleteStrategy {
    fn from(v: DeleteGroupStrategyDto) -> Self {
        match v {
            DeleteGroupStrategyDto::ReassignToParent => GroupDeleteStrategy::ReassignToParent,
            DeleteGroupStrategyDto::DeleteSubtree => GroupDeleteStrategy::DeleteSubtree,
        }
    }
}

macro_rules! group_commands {
    ($list:ident, $create:ident, $update:ident, $delete:ident, $repo:expr, $item_table:literal, $item_col:literal) => {
        #[tauri::command]
        pub async fn $list(state: State<'_, AppState>) -> AppResult<Vec<Group>> {
            let db = state.db.clone();
            tokio::task::spawn_blocking(move || {
                let conn = db.get()?;
                $repo.list(&conn)
            })
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?
        }

        #[tauri::command]
        pub async fn $create(state: State<'_, AppState>, input: NewGroupInput) -> AppResult<Group> {
            let db = state.db.clone();
            tokio::task::spawn_blocking(move || {
                let conn = db.get()?;
                $repo.create(&conn, input)
            })
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?
        }

        #[tauri::command]
        pub async fn $update(state: State<'_, AppState>, input: UpdateGroupInput) -> AppResult<Group> {
            let db = state.db.clone();
            tokio::task::spawn_blocking(move || {
                let conn = db.get()?;
                $repo.update(&conn, input)
            })
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?
        }

        #[tauri::command]
        pub async fn $delete(
            state: State<'_, AppState>,
            id: String,
            strategy: DeleteGroupStrategyDto,
        ) -> AppResult<()> {
            let db = state.db.clone();
            tokio::task::spawn_blocking(move || {
                let conn = db.get()?;
                $repo.delete(&conn, &id, strategy.into(), $item_table, $item_col)
            })
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?
        }
    };
}

group_commands!(
    terminal_groups_list,
    terminal_groups_create,
    terminal_groups_update,
    terminal_groups_delete,
    GroupRepository::terminal_groups(),
    "ssh_profiles",
    "terminal_group_id"
);

group_commands!(
    port_forward_groups_list,
    port_forward_groups_create,
    port_forward_groups_update,
    port_forward_groups_delete,
    GroupRepository::port_forward_groups(),
    "port_forwards",
    "port_forward_group_id"
);
