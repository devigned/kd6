use kd6_core::error::OmsError;
use kd6_core::models::MemoryScope;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::helpers::map_db_error;

pub(crate) async fn gdpr_purge(
    pool: &SqlitePool,
    tenant_id: &str,
    store_id: Uuid,
    scope: MemoryScope,
) -> Result<u64, OmsError> {
    crate::stores::get_store(pool, tenant_id, store_id).await?;

    // Normalize scope to use authenticated tenant_id
    let scope = scope.normalize(tenant_id);

    let has_scope_filter = scope.user_id.is_some()
        || scope.agent_id.is_some()
        || scope.org_id.is_some()
        || scope.team_id.is_some()
        || scope.project_id.is_some()
        || scope.session_id.is_some()
        || scope.run_id.is_some();

    if !has_scope_filter {
        return Err(OmsError::InvalidInput(
            "GDPR purge requires at least one scope field (user_id, agent_id, org_id, team_id, project_id, session_id, or run_id)".into(),
        ));
    }

    let mut conditions = vec!["store_id = ?".to_string(), "tenant_id = ?".to_string()];
    let mut bind_values: Vec<Option<String>> =
        vec![Some(store_id.to_string()), Some(tenant_id.to_string())];

    if let Some(ref v) = scope.user_id {
        conditions.push("scope_user_id = ?".to_string());
        bind_values.push(Some(v.clone()));
    }
    if let Some(ref v) = scope.agent_id {
        conditions.push("scope_agent_id = ?".to_string());
        bind_values.push(Some(v.clone()));
    }
    if let Some(ref v) = scope.org_id {
        conditions.push("scope_org_id = ?".to_string());
        bind_values.push(Some(v.clone()));
    }
    if let Some(ref v) = scope.team_id {
        conditions.push("scope_team_id = ?".to_string());
        bind_values.push(Some(v.clone()));
    }
    if let Some(ref v) = scope.project_id {
        conditions.push("scope_project_id = ?".to_string());
        bind_values.push(Some(v.clone()));
    }
    if let Some(ref v) = scope.session_id {
        conditions.push("scope_session_id = ?".to_string());
        bind_values.push(Some(v.clone()));
    }
    if let Some(ref v) = scope.run_id {
        conditions.push("scope_run_id = ?".to_string());
        bind_values.push(Some(v.clone()));
    }

    let where_clause = conditions.join(" AND ");

    let mut conn = pool
        .acquire()
        .await
        .map_err(|e| OmsError::Internal(format!("failed to acquire connection: {e}")))?;

    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *conn)
        .await
        .map_err(|e| map_db_error("begin transaction", e))?;

    // Collect memory IDs that will be purged (for audit cleanup)
    let select_sql = format!("SELECT id FROM memories WHERE {where_clause}");
    let mut select_q = sqlx::query_scalar::<_, String>(&select_sql);
    for val in &bind_values {
        select_q = select_q.bind(val.as_deref());
    }
    let purged_ids: Vec<String> = match select_q.fetch_all(&mut *conn).await {
        Ok(ids) => ids,
        Err(e) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            return Err(OmsError::Internal(format!("GDPR purge select failed: {e}")));
        }
    };

    // Delete matching memories
    let delete_sql = format!("DELETE FROM memories WHERE {where_clause}");
    let mut q = sqlx::query(&delete_sql);
    for val in &bind_values {
        q = q.bind(val.as_deref());
    }
    let result = match q.execute(&mut *conn).await {
        Ok(r) => r,
        Err(e) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            return Err(OmsError::Internal(format!("GDPR purge failed: {e}")));
        }
    };
    let deleted = result.rows_affected();

    // Anonymize audit log entries referencing purged memories (GDPR Art. 17).
    // We retain the audit entry for compliance proof but strip PII fields.
    // The `redacted` flag signals that entry_hash will no longer match current
    // row content, but the hash chain (prev_hash links) remains intact.
    if !purged_ids.is_empty() {
        for chunk in purged_ids.chunks(500) {
            let placeholders: Vec<&str> = chunk.iter().map(|_| "?").collect();
            let anon_sql = format!(
                "UPDATE audit_log SET agent_id = NULL, details_json = NULL, redacted = 1 \
                 WHERE store_id = ? AND tenant_id = ? AND memory_id IN ({})",
                placeholders.join(",")
            );
            let mut anon_q = sqlx::query(&anon_sql)
                .bind(store_id.to_string())
                .bind(tenant_id);
            for id in chunk {
                anon_q = anon_q.bind(id);
            }
            if let Err(e) = anon_q.execute(&mut *conn).await {
                let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
                return Err(OmsError::Internal(format!(
                    "GDPR audit anonymization failed: {e}"
                )));
            }
        }
    }

    // Log the purge action itself
    if let Err(e) = crate::audit::log_audit_on_conn(
        pool,
        &mut conn,
        tenant_id,
        store_id,
        None,
        "gdpr_purge",
        None,
        Some(serde_json::json!({
            "scope": scope,
            "deleted_count": deleted
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

    Ok(deleted)
}
