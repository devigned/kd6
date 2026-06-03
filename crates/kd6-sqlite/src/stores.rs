use chrono::{DateTime, Utc};
use kd6_core::error::OmsError;
use kd6_core::models::{CreateStoreRequest, MemoryStore, SovereigntyConfig, UpdateStoreRequest};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::audit::log_audit_on_conn;
use crate::helpers::map_db_error;

pub(crate) fn row_to_store(row: &sqlx::sqlite::SqliteRow) -> Result<MemoryStore, OmsError> {
    let id_str: String = row.get("id");
    let config_json: String = row.get("config_json");
    let metadata_json: String = row.get("metadata_json");
    let created_str: String = row.get("created_at");
    let updated_str: String = row.get("updated_at");

    Ok(MemoryStore {
        id: Uuid::parse_str(&id_str)
            .map_err(|e| OmsError::Internal(format!("invalid store id: {e}")))?,
        name: row.get("name"),
        tenant_id: row.get("tenant_id"),
        region: row.get("region"),
        config: serde_json::from_str(&config_json)
            .map_err(|e| OmsError::Internal(format!("invalid config json: {e}")))?,
        sovereignty: {
            let s: String = row.get("sovereignty_json");
            serde_json::from_str(&s)
                .map_err(|e| OmsError::Internal(format!("invalid sovereignty json: {e}")))?
        },
        metadata: serde_json::from_str(&metadata_json)
            .map_err(|e| OmsError::Internal(format!("invalid metadata json: {e}")))?,
        created_at: DateTime::parse_from_rfc3339(&created_str)
            .map_err(|e| OmsError::Internal(format!("invalid created_at: {e}")))?
            .with_timezone(&Utc),
        updated_at: DateTime::parse_from_rfc3339(&updated_str)
            .map_err(|e| OmsError::Internal(format!("invalid updated_at: {e}")))?
            .with_timezone(&Utc),
    })
}

pub(crate) async fn create_store(
    pool: &SqlitePool,
    tenant_id: &str,
    request: CreateStoreRequest,
) -> Result<MemoryStore, OmsError> {
    let id = Uuid::new_v4();
    let now = Utc::now();
    let now_str = now.to_rfc3339();
    let config_json = serde_json::to_string(&request.config)
        .map_err(|e| OmsError::Internal(format!("failed to serialize config: {e}")))?;
    let metadata_json = serde_json::to_string(&request.metadata)
        .map_err(|e| OmsError::Internal(format!("failed to serialize metadata: {e}")))?;

    let mut conn = pool
        .acquire()
        .await
        .map_err(|e| OmsError::Internal(format!("failed to acquire connection: {e}")))?;

    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *conn)
        .await
        .map_err(|e| map_db_error("begin transaction", e))?;

    if let Err(e) = sqlx::query(
        "INSERT INTO stores (id, name, tenant_id, region, config_json, metadata_json, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(&request.name)
    .bind(tenant_id)
    .bind(&request.region)
    .bind(&config_json)
    .bind(&metadata_json)
    .bind(&now_str)
    .bind(&now_str)
    .execute(&mut *conn)
    .await
    {
        let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
        return Err(map_db_error("insert store", e));
    }

    if let Err(e) = log_audit_on_conn(
        pool,
        &mut conn,
        tenant_id,
        id,
        None,
        "create_store",
        None,
        Some(serde_json::json!({
            "entity": "store",
            "store_id": id.to_string(),
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

    Ok(MemoryStore {
        id,
        name: request.name,
        tenant_id: tenant_id.to_string(),
        region: request.region,
        config: request.config,
        sovereignty: SovereigntyConfig::default(),
        metadata: request.metadata,
        created_at: now,
        updated_at: now,
    })
}

pub(crate) async fn get_store(
    pool: &SqlitePool,
    tenant_id: &str,
    store_id: Uuid,
) -> Result<MemoryStore, OmsError> {
    let row = sqlx::query("SELECT * FROM stores WHERE id = ? AND tenant_id = ?")
        .bind(store_id.to_string())
        .bind(tenant_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| map_db_error("query store", e))?
        .ok_or_else(|| OmsError::StoreNotFound(store_id.to_string()))?;

    row_to_store(&row)
}

pub(crate) async fn list_stores(
    pool: &SqlitePool,
    tenant_id: &str,
) -> Result<Vec<MemoryStore>, OmsError> {
    let rows = sqlx::query("SELECT * FROM stores WHERE tenant_id = ? ORDER BY created_at DESC")
        .bind(tenant_id)
        .fetch_all(pool)
        .await
        .map_err(|e| map_db_error("list stores", e))?;

    rows.iter().map(row_to_store).collect()
}

pub(crate) async fn get_or_create_store(
    pool: &SqlitePool,
    tenant_id: &str,
    name: &str,
    request: CreateStoreRequest,
) -> Result<MemoryStore, OmsError> {
    // Try to find existing store first
    if let Some(row) = sqlx::query("SELECT * FROM stores WHERE tenant_id = ? AND name = ?")
        .bind(tenant_id)
        .bind(name)
        .fetch_optional(pool)
        .await
        .map_err(|e| map_db_error("query store by name", e))?
    {
        return row_to_store(&row);
    }

    // Attempt atomic insert; unique index prevents duplicates
    let id = Uuid::new_v4();
    let now = Utc::now();
    let now_str = now.to_rfc3339();
    let config_json = serde_json::to_string(&request.config)
        .map_err(|e| OmsError::Internal(format!("failed to serialize config: {e}")))?;
    let metadata_json = serde_json::to_string(&request.metadata)
        .map_err(|e| OmsError::Internal(format!("failed to serialize metadata: {e}")))?;

    let result = sqlx::query(
        "INSERT OR IGNORE INTO stores (id, name, tenant_id, region, config_json, metadata_json, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(name)
    .bind(tenant_id)
    .bind(&request.region)
    .bind(&config_json)
    .bind(&metadata_json)
    .bind(&now_str)
    .bind(&now_str)
    .execute(pool)
    .await
    .map_err(|e| map_db_error("insert store", e))?;

    if result.rows_affected() == 0 {
        // Another request created it concurrently — fetch it
        let row = sqlx::query("SELECT * FROM stores WHERE tenant_id = ? AND name = ?")
            .bind(tenant_id)
            .bind(name)
            .fetch_one(pool)
            .await
            .map_err(|e| OmsError::Internal(format!("failed to fetch concurrent store: {e}")))?;
        return row_to_store(&row);
    }

    Ok(MemoryStore {
        id,
        name: name.to_string(),
        tenant_id: tenant_id.to_string(),
        region: request.region,
        config: request.config,
        sovereignty: SovereigntyConfig::default(),
        metadata: request.metadata,
        created_at: now,
        updated_at: now,
    })
}

pub(crate) async fn update_store(
    pool: &SqlitePool,
    tenant_id: &str,
    store_id: Uuid,
    request: UpdateStoreRequest,
) -> Result<MemoryStore, OmsError> {
    // Fetch existing store (also verifies tenant ownership)
    let existing = crate::stores::get_store(pool, tenant_id, store_id).await?;

    let name = request.name.unwrap_or(existing.name);
    let config = request.config.unwrap_or(existing.config);
    let metadata = request.metadata.unwrap_or(existing.metadata);
    let now_str = Utc::now().to_rfc3339();
    let config_json = serde_json::to_string(&config)
        .map_err(|e| OmsError::Internal(format!("failed to serialize config: {e}")))?;
    let metadata_json = serde_json::to_string(&metadata)
        .map_err(|e| OmsError::Internal(format!("failed to serialize metadata: {e}")))?;

    let mut conn = pool
        .acquire()
        .await
        .map_err(|e| OmsError::Internal(format!("failed to acquire connection: {e}")))?;

    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *conn)
        .await
        .map_err(|e| map_db_error("begin transaction", e))?;

    let result = match sqlx::query(
        "UPDATE stores SET name = ?, config_json = ?, metadata_json = ?, updated_at = ?
         WHERE id = ? AND tenant_id = ?",
    )
    .bind(&name)
    .bind(&config_json)
    .bind(&metadata_json)
    .bind(&now_str)
    .bind(store_id.to_string())
    .bind(tenant_id)
    .execute(&mut *conn)
    .await
    {
        Ok(result) => result,
        Err(e) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            return Err(map_db_error("update store", e));
        }
    };

    if result.rows_affected() == 0 {
        let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
        return Err(OmsError::StoreNotFound(store_id.to_string()));
    }

    if let Err(e) = log_audit_on_conn(
        pool,
        &mut conn,
        tenant_id,
        store_id,
        None,
        "update_store",
        None,
        Some(serde_json::json!({
            "entity": "store",
            "store_id": store_id.to_string(),
            "name": &name,
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

    crate::stores::get_store(pool, tenant_id, store_id).await
}

pub(crate) async fn delete_store(
    pool: &SqlitePool,
    tenant_id: &str,
    store_id: Uuid,
) -> Result<(), OmsError> {
    let result = sqlx::query("DELETE FROM stores WHERE id = ? AND tenant_id = ?")
        .bind(store_id.to_string())
        .bind(tenant_id)
        .execute(pool)
        .await
        .map_err(|e| map_db_error("delete store", e))?;

    if result.rows_affected() == 0 {
        return Err(OmsError::StoreNotFound(store_id.to_string()));
    }
    Ok(())
}
