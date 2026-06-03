use chrono::Utc;
use kd6_core::error::OmsError;
use kd6_core::models::{
    BatchCreateRequest, BatchCreateResponse, BatchDeleteRequest, BatchDeleteResponse, BatchError,
    StoreStats,
};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::helpers::{map_db_error, parse_layer};

pub(crate) async fn purge_expired(
    pool: &SqlitePool,
    tenant_id: &str,
    store_id: Uuid,
) -> Result<u64, OmsError> {
    crate::stores::get_store(pool, tenant_id, store_id).await?;

    let now_str = Utc::now().to_rfc3339();

    let mut conn = pool
        .acquire()
        .await
        .map_err(|e| OmsError::Internal(format!("failed to acquire connection: {e}")))?;

    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *conn)
        .await
        .map_err(|e| map_db_error("begin transaction", e))?;

    let result = match sqlx::query(
        "DELETE FROM memories WHERE store_id = ? AND tenant_id = ? AND expires_at IS NOT NULL AND expires_at < ?",
    )
    .bind(store_id.to_string())
    .bind(tenant_id)
    .bind(&now_str)
    .execute(&mut *conn)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            return Err(OmsError::Internal(format!("failed to purge expired memories: {e}")));
        }
    };

    let deleted = result.rows_affected();

    if let Err(e) = crate::audit::log_audit_on_conn(
        pool,
        &mut conn,
        tenant_id,
        store_id,
        None,
        "purge_expired",
        None,
        Some(serde_json::json!({ "deleted_count": deleted })),
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

    Ok(deleted)
}

pub(crate) async fn batch_create_memories(
    pool: &SqlitePool,
    tenant_id: &str,
    store_id: Uuid,
    request: BatchCreateRequest,
) -> Result<BatchCreateResponse, OmsError> {
    const MAX_BATCH_SIZE: usize = 1000;
    if request.entries.len() > MAX_BATCH_SIZE {
        return Err(OmsError::InvalidInput(format!(
            "batch size {} exceeds maximum of {MAX_BATCH_SIZE}",
            request.entries.len()
        )));
    }

    // Verify store once up front
    crate::stores::get_store(pool, tenant_id, store_id).await?;

    let mut created = Vec::new();
    let mut errors = Vec::new();

    for (index, entry) in request.entries.into_iter().enumerate() {
        match crate::memories::create_memory(pool, tenant_id, store_id, entry).await {
            Ok(memory) => created.push(memory),
            Err(error) => errors.push(BatchError {
                index,
                error: error.to_string(),
            }),
        }
    }

    Ok(BatchCreateResponse { created, errors })
}

pub(crate) async fn batch_delete_memories(
    pool: &SqlitePool,
    tenant_id: &str,
    store_id: Uuid,
    request: BatchDeleteRequest,
) -> Result<BatchDeleteResponse, OmsError> {
    const MAX_BATCH_SIZE: usize = 1000;
    if request.memory_ids.len() > MAX_BATCH_SIZE {
        return Err(OmsError::InvalidInput(format!(
            "batch size {} exceeds maximum of {MAX_BATCH_SIZE}",
            request.memory_ids.len()
        )));
    }

    let mut deleted = 0;
    let mut errors = Vec::new();

    for (index, memory_id) in request.memory_ids.into_iter().enumerate() {
        match crate::memories::delete_memory(pool, tenant_id, store_id, memory_id).await {
            Ok(()) => deleted += 1,
            Err(error) => errors.push(BatchError {
                index,
                error: error.to_string(),
            }),
        }
    }

    Ok(BatchDeleteResponse { deleted, errors })
}

pub(crate) async fn stats(
    pool: &SqlitePool,
    tenant_id: &str,
    store_id: Uuid,
) -> Result<StoreStats, OmsError> {
    // Verify store exists and belongs to tenant
    crate::stores::get_store(pool, tenant_id, store_id).await?;

    let row =
        sqlx::query("SELECT COUNT(*) as cnt FROM memories WHERE store_id = ? AND tenant_id = ?")
            .bind(store_id.to_string())
            .bind(tenant_id)
            .fetch_one(pool)
            .await
            .map_err(|e| map_db_error("get stats", e))?;

    let total: i64 = row.get("cnt");

    let layer_rows = sqlx::query(
        "SELECT layer, COUNT(*) as cnt FROM memories WHERE store_id = ? AND tenant_id = ? GROUP BY layer",
    )
    .bind(store_id.to_string())
    .bind(tenant_id)
    .fetch_all(pool)
    .await
    .map_err(|e| map_db_error("get layer stats", e))?;

    let mut entries_by_layer = std::collections::HashMap::new();
    for lr in &layer_rows {
        let layer: String = lr.get("layer");
        let count: i64 = lr.get("cnt");
        entries_by_layer.insert(parse_layer(&layer)?, count as u64);
    }

    Ok(StoreStats {
        store_id,
        tenant_id: tenant_id.to_string(),
        total_entries: total as u64,
        entries_by_layer,
        total_size_bytes: None,
    })
}
