use rusqlite::{Connection, OptionalExtension, Row};

use crate::domain::{ForwardType, NewPortForwardInput, PortForward, UpdatePortForwardInput};
use crate::error::{AppError, AppResult};
use crate::util::{new_id, now_iso};
use crate::validation::{validate_bind_address, validate_display_name, validate_port};

pub struct ForwardRepository;

impl ForwardRepository {
    pub fn list(conn: &Connection) -> AppResult<Vec<PortForward>> {
        let mut stmt = conn.prepare(&format!("{} ORDER BY sort_order, name", Self::select_clause()))?;
        let rows = stmt.query_map([], Self::map_row)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn get(conn: &Connection, id: &str) -> AppResult<PortForward> {
        conn.query_row(&format!("{} WHERE id = ?1", Self::select_clause()), [id], Self::map_row)
            .optional()?
            .ok_or_else(|| AppError::NotFound("port forward".into()))
    }

    pub fn create(conn: &Connection, input: NewPortForwardInput) -> AppResult<PortForward> {
        Self::validate_input(
            conn,
            &input.name,
            &input.bind_address,
            input.listen_port,
            input.forward_type,
            &input.target_hostname,
            input.target_port,
            &input.ssh_profile_id,
            None,
        )?;

        let target_hostname = if input.forward_type == ForwardType::Dynamic { None } else { input.target_hostname };
        let target_port = if input.forward_type == ForwardType::Dynamic { None } else { input.target_port };

        let id = new_id();
        let now = now_iso();
        conn.execute(
            "INSERT INTO port_forwards (id, port_forward_group_id, ssh_profile_id, name, type, bind_address, listen_port, target_hostname, target_port, start_on_launch, sort_order, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, (SELECT COALESCE(MAX(sort_order), -1) + 1 FROM port_forwards WHERE port_forward_group_id IS ?2), ?11, ?11)",
            rusqlite::params![
                id,
                input.port_forward_group_id,
                input.ssh_profile_id,
                input.name.trim(),
                input.forward_type.as_db_str(),
                input.bind_address,
                input.listen_port,
                target_hostname,
                target_port,
                input.start_on_launch,
                now,
            ],
        )?;
        Self::get(conn, &id)
    }

    pub fn update(conn: &Connection, input: UpdatePortForwardInput) -> AppResult<PortForward> {
        Self::validate_input(
            conn,
            &input.name,
            &input.bind_address,
            input.listen_port,
            input.forward_type,
            &input.target_hostname,
            input.target_port,
            &input.ssh_profile_id,
            Some(&input.id),
        )?;

        let target_hostname = if input.forward_type == ForwardType::Dynamic { None } else { input.target_hostname };
        let target_port = if input.forward_type == ForwardType::Dynamic { None } else { input.target_port };

        conn.execute(
            "UPDATE port_forwards SET port_forward_group_id = ?1, ssh_profile_id = ?2, name = ?3, type = ?4, bind_address = ?5, listen_port = ?6, target_hostname = ?7, target_port = ?8, start_on_launch = ?9, updated_at = ?10 WHERE id = ?11",
            rusqlite::params![
                input.port_forward_group_id,
                input.ssh_profile_id,
                input.name.trim(),
                input.forward_type.as_db_str(),
                input.bind_address,
                input.listen_port,
                target_hostname,
                target_port,
                input.start_on_launch,
                now_iso(),
                input.id,
            ],
        )?;
        Self::get(conn, &input.id)
    }

    pub fn delete(conn: &Connection, id: &str) -> AppResult<()> {
        conn.execute("DELETE FROM port_forwards WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn duplicate(conn: &Connection, id: &str, new_name: &str) -> AppResult<PortForward> {
        validate_display_name(new_name)?;
        let source = Self::get(conn, id)?;
        let new_id_value = new_id();
        let now = now_iso();
        conn.execute(
            "INSERT INTO port_forwards (id, port_forward_group_id, ssh_profile_id, name, type, bind_address, listen_port, target_hostname, target_port, start_on_launch, sort_order, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, (SELECT COALESCE(MAX(sort_order), -1) + 1 FROM port_forwards WHERE port_forward_group_id IS ?2), ?10, ?10)",
            rusqlite::params![
                new_id_value,
                source.port_forward_group_id,
                source.ssh_profile_id,
                new_name.trim(),
                source.forward_type.as_db_str(),
                source.bind_address,
                source.listen_port,
                source.target_hostname,
                source.target_port,
                now,
            ],
        )?;
        Self::get(conn, &new_id_value)
    }

    pub fn move_to_group(conn: &Connection, id: &str, group_id: Option<String>) -> AppResult<PortForward> {
        if let Some(group) = &group_id {
            Self::assert_group_exists(conn, group)?;
        }
        conn.execute(
            "UPDATE port_forwards SET port_forward_group_id = ?1, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![group_id, now_iso(), id],
        )?;
        Self::get(conn, id)
    }

    fn assert_group_exists(conn: &Connection, group_id: &str) -> AppResult<()> {
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM port_forward_groups WHERE id = ?1)",
            [group_id],
            |r| r.get(0),
        )?;
        if !exists {
            return Err(AppError::NotFound("port forward group".into()));
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn validate_input(
        conn: &Connection,
        name: &str,
        bind_address: &str,
        listen_port: u16,
        forward_type: ForwardType,
        target_hostname: &Option<String>,
        target_port: Option<u16>,
        ssh_profile_id: &str,
        exclude_id: Option<&str>,
    ) -> AppResult<()> {
        validate_display_name(name)?;
        validate_bind_address(bind_address)?;
        validate_port(listen_port)?;

        let profile_exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM ssh_profiles WHERE id = ?1)",
            [ssh_profile_id],
            |r| r.get(0),
        )?;
        if !profile_exists {
            return Err(AppError::NotFound("ssh profile".into()));
        }

        if forward_type != ForwardType::Dynamic {
            let host = target_hostname.as_deref().unwrap_or("").trim();
            if host.is_empty() {
                return Err(AppError::Validation("target hostname is required".into()));
            }
            match target_port {
                Some(p) => validate_port(p)?,
                None => return Err(AppError::Validation("target port is required".into())),
            }
        }

        if forward_type == ForwardType::Local || forward_type == ForwardType::Dynamic {
            let conflict_sql = "SELECT COUNT(*) FROM port_forwards WHERE bind_address = ?1 AND listen_port = ?2 AND type IN ('local','dynamic') AND id IS NOT ?3";
            let count: i64 = conn.query_row(conflict_sql, rusqlite::params![bind_address, listen_port, exclude_id], |r| r.get(0))?;
            if count > 0 {
                return Err(AppError::Validation(format!(
                    "another forward already listens on {bind_address}:{listen_port}"
                )));
            }
        }

        Ok(())
    }

    fn select_clause() -> &'static str {
        "SELECT id, port_forward_group_id, ssh_profile_id, name, type, bind_address, listen_port, target_hostname, target_port, start_on_launch, sort_order, created_at, updated_at FROM port_forwards"
    }

    fn map_row(row: &Row) -> rusqlite::Result<PortForward> {
        let type_str: String = row.get(4)?;
        Ok(PortForward {
            id: row.get(0)?,
            port_forward_group_id: row.get(1)?,
            ssh_profile_id: row.get(2)?,
            name: row.get(3)?,
            forward_type: ForwardType::from_db_str(&type_str).unwrap_or(ForwardType::Local),
            bind_address: row.get(5)?,
            listen_port: row.get::<_, i64>(6)? as u16,
            target_hostname: row.get(7)?,
            target_port: row.get::<_, Option<i64>>(8)?.map(|p| p as u16),
            start_on_launch: row.get(9)?,
            sort_order: row.get(10)?,
            created_at: row.get(11)?,
            updated_at: row.get(12)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::domain::{AuthSecretInput, NewGroupInput, NewSshProfileInput};
    use crate::repositories::{GroupRepository, ProfileRepository};
    use crate::vault::VaultManager;

    fn setup() -> (tempfile::TempDir, db::Pool, String) {
        let dir = tempfile::tempdir().unwrap();
        let pool = db::init_pool(&dir.path().join("t.db")).unwrap();
        let vault = VaultManager::new();
        let conn = pool.get().unwrap();
        vault.setup(&pool, "correct-password-1").unwrap();
        let profile = ProfileRepository::create(
            &conn,
            &vault,
            NewSshProfileInput {
                terminal_group_id: None,
                name: "Bastion".into(),
                hostname: "bastion.example.com".into(),
                port: 22,
                username: "root".into(),
                terminal_type: None,
                secret: AuthSecretInput::Password { password: "s3cret".into() },
            },
        )
        .unwrap();
        (dir, pool, profile.id)
    }

    fn local_input(profile_id: &str, listen_port: u16) -> NewPortForwardInput {
        NewPortForwardInput {
            port_forward_group_id: None,
            ssh_profile_id: profile_id.to_string(),
            name: "DB tunnel".into(),
            forward_type: ForwardType::Local,
            bind_address: "127.0.0.1".into(),
            listen_port,
            target_hostname: Some("database.internal".into()),
            target_port: Some(5432),
            start_on_launch: false,
        }
    }

    #[test]
    fn create_local_forward_succeeds() {
        let (_dir, pool, profile_id) = setup();
        let conn = pool.get().unwrap();
        let f = ForwardRepository::create(&conn, local_input(&profile_id, 15432)).unwrap();
        assert_eq!(f.forward_type, ForwardType::Local);
        assert_eq!(f.target_port, Some(5432));
    }

    #[test]
    fn duplicate_local_listener_is_rejected() {
        let (_dir, pool, profile_id) = setup();
        let conn = pool.get().unwrap();
        ForwardRepository::create(&conn, local_input(&profile_id, 15432)).unwrap();
        let err = ForwardRepository::create(&conn, local_input(&profile_id, 15432));
        assert!(err.is_err());
    }

    #[test]
    fn dynamic_forward_ignores_target_fields() {
        let (_dir, pool, profile_id) = setup();
        let conn = pool.get().unwrap();
        let mut input = local_input(&profile_id, 1080);
        input.forward_type = ForwardType::Dynamic;
        let f = ForwardRepository::create(&conn, input).unwrap();
        assert_eq!(f.target_hostname, None);
        assert_eq!(f.target_port, None);
    }

    #[test]
    fn remote_forward_requires_target() {
        let (_dir, pool, profile_id) = setup();
        let conn = pool.get().unwrap();
        let mut input = local_input(&profile_id, 8080);
        input.forward_type = ForwardType::Remote;
        input.target_hostname = None;
        input.target_port = None;
        assert!(ForwardRepository::create(&conn, input).is_err());
    }

    #[test]
    fn unknown_profile_is_rejected() {
        let (_dir, pool, _profile_id) = setup();
        let conn = pool.get().unwrap();
        let input = local_input("does-not-exist", 15432);
        assert!(ForwardRepository::create(&conn, input).is_err());
    }

    #[test]
    fn move_to_group_sets_and_clears_group() {
        let (_dir, pool, profile_id) = setup();
        let conn = pool.get().unwrap();
        let group = GroupRepository::port_forward_groups()
            .create(&conn, NewGroupInput { parent_id: None, name: "Databases".into() })
            .unwrap();
        let f = ForwardRepository::create(&conn, local_input(&profile_id, 15432)).unwrap();

        let moved = ForwardRepository::move_to_group(&conn, &f.id, Some(group.id.clone())).unwrap();
        assert_eq!(moved.port_forward_group_id, Some(group.id));

        let ungrouped = ForwardRepository::move_to_group(&conn, &f.id, None).unwrap();
        assert_eq!(ungrouped.port_forward_group_id, None);
    }

    #[test]
    fn move_to_terminal_group_is_rejected() {
        let (_dir, pool, profile_id) = setup();
        let conn = pool.get().unwrap();
        let terminal_group = GroupRepository::terminal_groups()
            .create(&conn, NewGroupInput { parent_id: None, name: "Servers".into() })
            .unwrap();
        let f = ForwardRepository::create(&conn, local_input(&profile_id, 15432)).unwrap();
        assert!(ForwardRepository::move_to_group(&conn, &f.id, Some(terminal_group.id)).is_err());
        assert!(ForwardRepository::move_to_group(&conn, &f.id, Some("does-not-exist".into())).is_err());
    }

    #[test]
    fn out_of_range_port_is_rejected() {
        let (_dir, pool, profile_id) = setup();
        let conn = pool.get().unwrap();
        let mut input = local_input(&profile_id, 15432);
        input.listen_port = 0;
        assert!(ForwardRepository::create(&conn, input).is_err());
    }
}
