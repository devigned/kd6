use std::collections::HashMap;

use chrono::{DateTime, Utc};
use kd6_core::error::OmsError;
use kd6_core::models::{
    CreateSharedSpaceRequest, JoinSpaceRequest, LeaveSpaceRequest, SharedSpace, SpaceParticipant,
};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::audit::log_audit_on_conn;
use crate::helpers::map_db_error;

use crate::helpers::{
    conflict_resolution_to_str, parse_conflict_resolution, parse_layer, parse_participant_access,
    participant_access_to_str,
};

pub(crate) fn row_to_space_participant(
    row: &sqlx::sqlite::SqliteRow,
) -> Result<SpaceParticipant, OmsError> {
    let joined_at: String = row.get("joined_at");

    Ok(SpaceParticipant {
        agent_id: row.get("agent_id"),
        access: parse_participant_access(&row.get::<String, _>("access"))?,
        joined_at: DateTime::parse_from_rfc3339(&joined_at)
            .map_err(|e| OmsError::Internal(format!("invalid participant joined_at: {e}")))?
            .with_timezone(&Utc),
    })
}

pub(crate) fn row_to_shared_space(
    row: &sqlx::sqlite::SqliteRow,
    participants: Vec<SpaceParticipant>,
) -> Result<SharedSpace, OmsError> {
    let id_str: String = row.get("id");
    let store_id_str: String = row.get("store_id");
    let scope_json: String = row.get("scope_json");
    let created_at: String = row.get("created_at");
    let updated_at: String = row.get("updated_at");

    Ok(SharedSpace {
        id: Uuid::parse_str(&id_str)
            .map_err(|e| OmsError::Internal(format!("invalid shared space id: {e}")))?,
        name: row.get("name"),
        store_id: Uuid::parse_str(&store_id_str)
            .map_err(|e| OmsError::Internal(format!("invalid shared space store id: {e}")))?,
        tenant_id: row.get("tenant_id"),
        scope: serde_json::from_str(&scope_json)
            .map_err(|e| OmsError::Internal(format!("invalid shared space scope json: {e}")))?,
        layer: parse_layer(&row.get::<String, _>("layer"))?,
        conflict_resolution: parse_conflict_resolution(
            &row.get::<String, _>("conflict_resolution"),
        )?,
        notify_on_write: row.get::<i64, _>("notify_on_write") != 0,
        notify_on_delete: row.get::<i64, _>("notify_on_delete") != 0,
        participants,
        created_at: DateTime::parse_from_rfc3339(&created_at)
            .map_err(|e| OmsError::Internal(format!("invalid shared space created_at: {e}")))?
            .with_timezone(&Utc),
        updated_at: DateTime::parse_from_rfc3339(&updated_at)
            .map_err(|e| OmsError::Internal(format!("invalid shared space updated_at: {e}")))?
            .with_timezone(&Utc),
    })
}

pub(crate) async fn get_space_participants(
    pool: &SqlitePool,
    tenant_id: &str,
    store_id: Uuid,
    space_id: Uuid,
) -> Result<Vec<SpaceParticipant>, OmsError> {
    let rows = sqlx::query(
        "SELECT sp.agent_id, sp.access, sp.joined_at
         FROM space_participants sp
         JOIN shared_spaces ss ON ss.id = sp.space_id
         WHERE sp.space_id = ? AND ss.store_id = ? AND ss.tenant_id = ?
         ORDER BY sp.joined_at ASC",
    )
    .bind(space_id.to_string())
    .bind(store_id.to_string())
    .bind(tenant_id)
    .fetch_all(pool)
    .await
    .map_err(|e| map_db_error("query space participants", e))?;

    rows.iter().map(row_to_space_participant).collect()
}

pub(crate) async fn hydrate_shared_space(
    pool: &SqlitePool,
    row: &sqlx::sqlite::SqliteRow,
) -> Result<SharedSpace, OmsError> {
    let id_str: String = row.get("id");
    let store_id_str: String = row.get("store_id");
    let tenant_id: String = row.get("tenant_id");
    let space_id = Uuid::parse_str(&id_str)
        .map_err(|e| OmsError::Internal(format!("invalid shared space id: {e}")))?;
    let store_id = Uuid::parse_str(&store_id_str)
        .map_err(|e| OmsError::Internal(format!("invalid shared space store id: {e}")))?;
    let participants = get_space_participants(pool, &tenant_id, store_id, space_id).await?;
    row_to_shared_space(row, participants)
}

async fn get_space_participants_by_space_id(
    pool: &SqlitePool,
    tenant_id: &str,
    store_id: Uuid,
    space_ids: &[String],
) -> Result<HashMap<String, Vec<SpaceParticipant>>, OmsError> {
    if space_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let placeholders = vec!["?"; space_ids.len()].join(", ");
    let query = format!(
        "SELECT sp.space_id, sp.agent_id, sp.access, sp.joined_at
         FROM space_participants sp
         JOIN shared_spaces ss ON ss.id = sp.space_id
         WHERE ss.store_id = ? AND ss.tenant_id = ? AND sp.space_id IN ({placeholders})
         ORDER BY sp.space_id ASC, sp.joined_at ASC"
    );

    let mut query = sqlx::query(&query)
        .bind(store_id.to_string())
        .bind(tenant_id);

    for space_id in space_ids {
        query = query.bind(space_id);
    }

    let rows = query
        .fetch_all(pool)
        .await
        .map_err(|e| map_db_error("query space participants", e))?;

    let mut participants_by_space_id = HashMap::new();
    for row in &rows {
        let space_id: String = row.get("space_id");
        participants_by_space_id
            .entry(space_id)
            .or_insert_with(Vec::new)
            .push(row_to_space_participant(row)?);
    }

    Ok(participants_by_space_id)
}

pub(crate) async fn create_shared_space(
    pool: &SqlitePool,
    tenant_id: &str,
    store_id: Uuid,
    request: CreateSharedSpaceRequest,
) -> Result<SharedSpace, OmsError> {
    crate::stores::get_store(pool, tenant_id, store_id).await?;

    let id = Uuid::new_v4();
    let now = Utc::now().to_rfc3339();
    let scope = request.scope.normalize(tenant_id);
    let scope_json = serde_json::to_string(&scope)
        .map_err(|e| OmsError::Internal(format!("failed to serialize shared space scope: {e}")))?;

    let mut conn = pool
        .acquire()
        .await
        .map_err(|e| OmsError::Internal(format!("failed to acquire connection: {e}")))?;

    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *conn)
        .await
        .map_err(|e| map_db_error("begin transaction", e))?;

    if let Err(e) = sqlx::query(
        "INSERT INTO shared_spaces (id, name, store_id, tenant_id, scope_json, layer, conflict_resolution, notify_on_write, notify_on_delete, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(&request.name)
    .bind(store_id.to_string())
    .bind(tenant_id)
    .bind(scope_json)
    .bind(request.layer.to_string())
    .bind(conflict_resolution_to_str(request.conflict_resolution))
    .bind(request.notify_on_write as i64)
    .bind(request.notify_on_delete as i64)
    .bind(&now)
    .bind(&now)
    .execute(&mut *conn)
    .await
    {
        let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
        return Err(map_db_error("create shared space", e));
    }

    if let Err(e) = log_audit_on_conn(
        pool,
        &mut conn,
        tenant_id,
        store_id,
        None,
        "create_shared_space",
        None,
        Some(serde_json::json!({
            "entity": "shared_space",
            "space_id": id.to_string(),
            "name": &request.name,
        })),
    )
    .await
    {
        let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
        return Err(e);
    }

    sqlx::query("COMMIT")
        .execute(&mut *conn)
        .await
        .map_err(|e| map_db_error("commit transaction", e))?;

    get_shared_space(pool, tenant_id, store_id, id).await
}

pub(crate) async fn list_shared_spaces(
    pool: &SqlitePool,
    tenant_id: &str,
    store_id: Uuid,
) -> Result<Vec<SharedSpace>, OmsError> {
    crate::stores::get_store(pool, tenant_id, store_id).await?;

    let rows = sqlx::query(
        "SELECT * FROM shared_spaces WHERE store_id = ? AND tenant_id = ? ORDER BY created_at DESC",
    )
    .bind(store_id.to_string())
    .bind(tenant_id)
    .fetch_all(pool)
    .await
    .map_err(|e| map_db_error("list shared spaces", e))?;

    let space_ids = rows
        .iter()
        .map(|row| row.get::<String, _>("id"))
        .collect::<Vec<_>>();
    let mut participants_by_space_id =
        get_space_participants_by_space_id(pool, tenant_id, store_id, &space_ids).await?;

    let mut spaces = Vec::with_capacity(rows.len());
    for row in &rows {
        let space_id: String = row.get("id");
        let participants = participants_by_space_id
            .remove(&space_id)
            .unwrap_or_default();
        spaces.push(row_to_shared_space(row, participants)?);
    }
    Ok(spaces)
}

pub(crate) async fn get_shared_space(
    pool: &SqlitePool,
    tenant_id: &str,
    store_id: Uuid,
    space_id: Uuid,
) -> Result<SharedSpace, OmsError> {
    let row =
        sqlx::query("SELECT * FROM shared_spaces WHERE id = ? AND store_id = ? AND tenant_id = ?")
            .bind(space_id.to_string())
            .bind(store_id.to_string())
            .bind(tenant_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| map_db_error("query shared space", e))?
            .ok_or_else(|| OmsError::InvalidInput(format!("shared space not found: {space_id}")))?;

    hydrate_shared_space(pool, &row).await
}

pub(crate) async fn join_shared_space(
    pool: &SqlitePool,
    tenant_id: &str,
    store_id: Uuid,
    space_id: Uuid,
    request: JoinSpaceRequest,
) -> Result<SharedSpace, OmsError> {
    get_shared_space(pool, tenant_id, store_id, space_id).await?;

    let access = participant_access_to_str(request.access);
    let joined_at = Utc::now().to_rfc3339();
    let mut conn = pool
        .acquire()
        .await
        .map_err(|e| OmsError::Internal(format!("failed to acquire connection: {e}")))?;

    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *conn)
        .await
        .map_err(|e| map_db_error("begin transaction", e))?;

    if let Err(e) = sqlx::query(
        "INSERT INTO space_participants (space_id, agent_id, access, joined_at)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(space_id, agent_id) DO UPDATE SET access = excluded.access, joined_at = excluded.joined_at",
    )
    .bind(space_id.to_string())
    .bind(&request.agent_id)
    .bind(access)
    .bind(&joined_at)
    .execute(&mut *conn)
    .await
    {
        let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
        return Err(map_db_error("join shared space", e));
    }

    if let Err(e) = log_audit_on_conn(
        pool,
        &mut conn,
        tenant_id,
        store_id,
        None,
        "join_shared_space",
        Some(request.agent_id.as_str()),
        Some(serde_json::json!({
            "entity": "shared_space",
            "space_id": space_id.to_string(),
            "agent_id": &request.agent_id,
            "access": access,
        })),
    )
    .await
    {
        let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
        return Err(e);
    }

    sqlx::query("COMMIT")
        .execute(&mut *conn)
        .await
        .map_err(|e| map_db_error("commit transaction", e))?;

    get_shared_space(pool, tenant_id, store_id, space_id).await
}

pub(crate) async fn leave_shared_space(
    pool: &SqlitePool,
    tenant_id: &str,
    store_id: Uuid,
    space_id: Uuid,
    request: LeaveSpaceRequest,
) -> Result<(), OmsError> {
    get_shared_space(pool, tenant_id, store_id, space_id).await?;

    let mut conn = pool
        .acquire()
        .await
        .map_err(|e| OmsError::Internal(format!("failed to acquire connection: {e}")))?;

    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *conn)
        .await
        .map_err(|e| map_db_error("begin transaction", e))?;

    if let Err(e) =
        sqlx::query("DELETE FROM space_participants WHERE space_id = ? AND agent_id = ?")
            .bind(space_id.to_string())
            .bind(&request.agent_id)
            .execute(&mut *conn)
            .await
    {
        let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
        return Err(map_db_error("leave shared space", e));
    }

    if let Err(e) = log_audit_on_conn(
        pool,
        &mut conn,
        tenant_id,
        store_id,
        None,
        "leave_shared_space",
        Some(request.agent_id.as_str()),
        Some(serde_json::json!({
            "entity": "shared_space",
            "space_id": space_id.to_string(),
            "agent_id": &request.agent_id,
        })),
    )
    .await
    {
        let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
        return Err(e);
    }

    sqlx::query("COMMIT")
        .execute(&mut *conn)
        .await
        .map_err(|e| map_db_error("commit transaction", e))?;

    Ok(())
}

pub(crate) async fn delete_shared_space(
    pool: &SqlitePool,
    tenant_id: &str,
    store_id: Uuid,
    space_id: Uuid,
) -> Result<(), OmsError> {
    let mut conn = pool
        .acquire()
        .await
        .map_err(|e| OmsError::Internal(format!("failed to acquire connection: {e}")))?;

    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *conn)
        .await
        .map_err(|e| map_db_error("begin transaction", e))?;

    let result = match sqlx::query(
        "DELETE FROM shared_spaces WHERE id = ? AND store_id = ? AND tenant_id = ?",
    )
    .bind(space_id.to_string())
    .bind(store_id.to_string())
    .bind(tenant_id)
    .execute(&mut *conn)
    .await
    {
        Ok(result) => result,
        Err(e) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            return Err(map_db_error("delete shared space", e));
        }
    };

    if result.rows_affected() == 0 {
        let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
        return Err(OmsError::InvalidInput(format!(
            "shared space not found: {space_id}"
        )));
    }

    if let Err(e) = log_audit_on_conn(
        pool,
        &mut conn,
        tenant_id,
        store_id,
        None,
        "delete_shared_space",
        None,
        Some(serde_json::json!({
            "entity": "shared_space",
            "space_id": space_id.to_string(),
        })),
    )
    .await
    {
        let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
        return Err(e);
    }

    sqlx::query("COMMIT")
        .execute(&mut *conn)
        .await
        .map_err(|e| map_db_error("commit transaction", e))?;

    Ok(())
}
