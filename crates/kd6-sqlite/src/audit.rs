use chrono::{DateTime, Utc};
use kd6_core::error::OmsError;
use kd6_core::models::{AuditEntry, AuditFilter, Page};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqliteConnection, SqlitePool};
use uuid::Uuid;

use crate::helpers::map_db_error;

pub(crate) fn row_to_audit(row: &sqlx::sqlite::SqliteRow) -> Result<AuditEntry, OmsError> {
    let id_str: String = row.get("id");
    let store_id_str: String = row.get("store_id");
    let memory_id_str: Option<String> = row.get("memory_id");
    let details_json: Option<String> = row.get("details_json");
    let created_at: String = row.get("created_at");

    Ok(AuditEntry {
        id: Uuid::parse_str(&id_str)
            .map_err(|e| OmsError::Internal(format!("invalid audit id: {e}")))?,
        store_id: Uuid::parse_str(&store_id_str)
            .map_err(|e| OmsError::Internal(format!("invalid audit store id: {e}")))?,
        tenant_id: row.get("tenant_id"),
        memory_id: memory_id_str
            .map(|id| {
                Uuid::parse_str(&id)
                    .map_err(|e| OmsError::Internal(format!("invalid audit memory id: {e}")))
            })
            .transpose()?,
        action: row.get("action"),
        agent_id: row.get("agent_id"),
        details: details_json
            .map(|json| serde_json::from_str(&json))
            .transpose()
            .map_err(|e| OmsError::Internal(format!("invalid audit details json: {e}")))?,
        created_at: DateTime::parse_from_rfc3339(&created_at)
            .map_err(|e| OmsError::Internal(format!("invalid audit created_at: {e}")))?
            .with_timezone(&Utc),
        redacted: row.try_get::<i32, _>("redacted").unwrap_or(0) != 0,
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn build_audit_hash(
    id: Uuid,
    store_id: Uuid,
    tenant_id: &str,
    memory_id: Option<&str>,
    action: &str,
    agent_id: Option<&str>,
    details_json: Option<&str>,
    now_str: &str,
    prev_hash: Option<&str>,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(id.to_string().as_bytes());
    hasher.update(store_id.to_string().as_bytes());
    hasher.update(tenant_id.as_bytes());
    hasher.update(memory_id.unwrap_or("").as_bytes());
    hasher.update(action.as_bytes());
    hasher.update(agent_id.unwrap_or("").as_bytes());
    hasher.update(details_json.unwrap_or("").as_bytes());
    hasher.update(now_str.as_bytes());
    hasher.update(prev_hash.unwrap_or("").as_bytes());
    format!("{:x}", hasher.finalize())
}

/// transaction management (BEGIN/COMMIT/ROLLBACK).
#[allow(clippy::too_many_arguments)]
pub(crate) async fn log_audit_on_conn(
    _pool: &SqlitePool,
    conn: &mut SqliteConnection,
    tenant_id: &str,
    store_id: Uuid,
    memory_id: Option<Uuid>,
    action: &str,
    agent_id: Option<&str>,
    details: Option<serde_json::Value>,
) -> Result<(), OmsError> {
    let id = Uuid::new_v4();
    let now_str = Utc::now().to_rfc3339();
    let details_json = details
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|e| OmsError::Internal(format!("failed to serialize audit details: {e}")))?;
    let memory_id_str = memory_id.map(|id| id.to_string());

    let prev_hash: Option<String> = sqlx::query_scalar(
        "SELECT entry_hash FROM audit_log WHERE store_id = ? AND tenant_id = ? ORDER BY rowid DESC LIMIT 1",
    )
    .bind(store_id.to_string())
    .bind(tenant_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|e| map_db_error("query prev audit hash", e))?
    .flatten();

    let entry_hash = build_audit_hash(
        id,
        store_id,
        tenant_id,
        memory_id_str.as_deref(),
        action,
        agent_id,
        details_json.as_deref(),
        &now_str,
        prev_hash.as_deref(),
    );

    sqlx::query(
        "INSERT INTO audit_log (id, store_id, tenant_id, memory_id, action, agent_id, details_json, created_at, entry_hash, prev_hash)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(store_id.to_string())
    .bind(tenant_id)
    .bind(&memory_id_str)
    .bind(action)
    .bind(agent_id)
    .bind(&details_json)
    .bind(&now_str)
    .bind(&entry_hash)
    .bind(&prev_hash)
    .execute(&mut *conn)
    .await
    .map_err(|e| map_db_error("insert audit log", e))?;

    Ok(())
}

/// Standalone audit log entry — acquires its own connection and transaction.
#[allow(dead_code)]
pub(crate) async fn log_audit(
    pool: &SqlitePool,
    tenant_id: &str,
    store_id: Uuid,
    memory_id: Option<Uuid>,
    action: &str,
    agent_id: Option<&str>,
    details: Option<serde_json::Value>,
) -> Result<(), OmsError> {
    let mut conn = pool
        .acquire()
        .await
        .map_err(|e| OmsError::Internal(format!("failed to acquire audit connection: {e}")))?;

    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *conn)
        .await
        .map_err(|e| map_db_error("begin audit transaction", e))?;

    if let Err(e) = crate::audit::log_audit_on_conn(
        pool, &mut conn, tenant_id, store_id, memory_id, action, agent_id, details,
    )
    .await
    {
        let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
        return Err(e);
    }

    sqlx::query("COMMIT")
        .execute(&mut *conn)
        .await
        .map_err(|e| map_db_error("commit audit transaction", e))?;

    Ok(())
}

pub(crate) async fn audit_log(
    pool: &SqlitePool,
    tenant_id: &str,
    store_id: Uuid,
    filter: AuditFilter,
) -> Result<Page<AuditEntry>, OmsError> {
    crate::stores::get_store(pool, tenant_id, store_id).await?;

    let limit = filter.limit.unwrap_or(50).min(1000);
    let offset = filter.offset.unwrap_or(0);
    let mut conditions = vec!["store_id = ?".to_string(), "tenant_id = ?".to_string()];
    let mut bind_values = vec![store_id.to_string(), tenant_id.to_string()];

    if let Some(memory_id) = filter.memory_id {
        conditions.push("memory_id = ?".to_string());
        bind_values.push(memory_id.to_string());
    }
    if let Some(action) = &filter.action {
        conditions.push("action = ?".to_string());
        bind_values.push(action.clone());
    }
    if let Some(agent_id) = &filter.agent_id {
        conditions.push("agent_id = ?".to_string());
        bind_values.push(agent_id.clone());
    }

    let where_clause = conditions.join(" AND ");
    let count_sql = format!("SELECT COUNT(*) AS cnt FROM audit_log WHERE {where_clause}");
    let mut count_query = sqlx::query(&count_sql);
    for value in &bind_values {
        count_query = count_query.bind(value);
    }
    let count_row = count_query
        .fetch_one(pool)
        .await
        .map_err(|e| map_db_error("count audit logs", e))?;
    let total: i64 = count_row.get("cnt");

    let data_sql = format!(
        "SELECT * FROM audit_log WHERE {where_clause} ORDER BY created_at DESC LIMIT ? OFFSET ?"
    );
    let mut data_query = sqlx::query(&data_sql);
    for value in &bind_values {
        data_query = data_query.bind(value);
    }
    data_query = data_query.bind(limit as i64).bind(offset as i64);

    let rows = data_query
        .fetch_all(pool)
        .await
        .map_err(|e| map_db_error("query audit logs", e))?;

    Ok(Page {
        items: rows.iter().map(row_to_audit).collect::<Result<_, _>>()?,
        total: total as u64,
        limit,
        offset,
    })
}
