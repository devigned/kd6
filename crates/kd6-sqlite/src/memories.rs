use chrono::{DateTime, Utc};
use kd6_core::error::OmsError;
use kd6_core::models::{
    AccessControl, CreateMemoryRequest, ListMemoriesFilter, MemoryEntry, MemoryScope, Page,
    SourceReference, UpdateMemoryRequest,
};
use sqlx::{Row, SqliteConnection, SqlitePool};
use uuid::Uuid;

use crate::helpers::{
    access_policy_to_str, bytes_to_embedding, embedding_to_bytes, escape_like, map_db_error,
    parse_access_policy, parse_layer,
};

pub(crate) fn row_to_memory(row: &sqlx::sqlite::SqliteRow) -> Result<MemoryEntry, OmsError> {
    let id_str: String = row.get("id");
    let store_id_str: String = row.get("store_id");
    let layer_str: String = row.get("layer");
    let content_json: String = row.get("content_json");
    let embedding_bytes: Option<Vec<u8>> = row.get("embedding");
    let tags_json: String = row.get("tags_json");
    let categories_json: String = row.get("categories_json");
    let source_json: Option<String> = row.get("source_json");
    let access_policy_str: String = row.get("access_policy");
    let allowed_agents_json: Option<String> = row.get("allowed_agents_json");
    let allowed_scopes_json: Option<String> = row.get("allowed_scopes_json");
    let created_str: String = row.get("created_at");
    let updated_str: String = row.get("updated_at");
    let expires_str: Option<String> = row.get("expires_at");
    let immutable_int: i32 = row.get("immutable");

    Ok(MemoryEntry {
        id: Uuid::parse_str(&id_str)
            .map_err(|e| OmsError::Internal(format!("invalid memory id: {e}")))?,
        store_id: Uuid::parse_str(&store_id_str)
            .map_err(|e| OmsError::Internal(format!("invalid store id: {e}")))?,
        layer: parse_layer(&layer_str)?,
        content: serde_json::from_str(&content_json)
            .map_err(|e| OmsError::Internal(format!("invalid content json: {e}")))?,
        embedding: embedding_bytes.map(|b| bytes_to_embedding(&b)),
        owner_agent_id: row.get("owner_agent_id"),
        scope: MemoryScope {
            tenant_id: row.get("scope_tenant_id"),
            org_id: row.get("scope_org_id"),
            team_id: row.get("scope_team_id"),
            project_id: row.get("scope_project_id"),
            user_id: row.get("scope_user_id"),
            agent_id: row.get("scope_agent_id"),
            session_id: row.get("scope_session_id"),
            run_id: row.get("scope_run_id"),
        },
        tags: serde_json::from_str(&tags_json)
            .map_err(|e| OmsError::Internal(format!("invalid tags json: {e}")))?,
        categories: serde_json::from_str(&categories_json)
            .map_err(|e| OmsError::Internal(format!("invalid categories json: {e}")))?,
        source: source_json
            .map(|s| serde_json::from_str::<SourceReference>(&s))
            .transpose()
            .map_err(|e| OmsError::Internal(format!("invalid source json: {e}")))?,
        access_control: AccessControl {
            policy: parse_access_policy(&access_policy_str)?,
            allowed_agents: allowed_agents_json
                .map(|s| serde_json::from_str(&s))
                .transpose()
                .map_err(|e| OmsError::Internal(format!("invalid allowed_agents json: {e}")))?,
            allowed_scopes: allowed_scopes_json
                .map(|s| serde_json::from_str(&s))
                .transpose()
                .map_err(|e| OmsError::Internal(format!("invalid allowed_scopes json: {e}")))?,
        },
        created_at: DateTime::parse_from_rfc3339(&created_str)
            .map_err(|e| OmsError::Internal(format!("invalid created_at: {e}")))?
            .with_timezone(&Utc),
        updated_at: DateTime::parse_from_rfc3339(&updated_str)
            .map_err(|e| OmsError::Internal(format!("invalid updated_at: {e}")))?
            .with_timezone(&Utc),
        expires_at: expires_str
            .map(|s| {
                DateTime::parse_from_rfc3339(&s)
                    .map(|dt| dt.with_timezone(&Utc))
                    .map_err(|e| OmsError::Internal(format!("invalid expires_at: {e}")))
            })
            .transpose()?,
        immutable: immutable_int != 0,
        version: row.get("version"),
        valid_from: {
            let s: Option<String> = row.get("valid_from");
            s.map(|s| DateTime::parse_from_rfc3339(&s).map(|dt| dt.with_timezone(&Utc)))
                .transpose()
                .map_err(|e| OmsError::Internal(format!("invalid valid_from: {e}")))?
        },
        valid_until: {
            let s: Option<String> = row.get("valid_until");
            s.map(|s| DateTime::parse_from_rfc3339(&s).map(|dt| dt.with_timezone(&Utc)))
                .transpose()
                .map_err(|e| OmsError::Internal(format!("invalid valid_until: {e}")))?
        },
        confidence: row.get("confidence"),
        entity_type: row.get("entity_type"),
        upsert_key: row.get("upsert_key"),
    })
}

/// Does NOT manage transactions — caller is responsible for BEGIN/COMMIT/ROLLBACK.
/// Returns the created `MemoryEntry`.
pub(crate) async fn insert_memory_on_conn(
    pool: &SqlitePool,
    conn: &mut SqliteConnection,
    tenant_id: &str,
    store_id: Uuid,
    request: CreateMemoryRequest,
) -> Result<MemoryEntry, OmsError> {
    let mut request = request;
    request.scope = request.scope.normalize(tenant_id);

    let id = Uuid::new_v4();
    let now = Utc::now();
    let now_str = now.to_rfc3339();
    let embedding_blob = request.embedding.as_ref().map(|v| embedding_to_bytes(v));
    let tags_json = serde_json::to_string(&request.tags)
        .map_err(|e| OmsError::Internal(format!("failed to serialize tags: {e}")))?;
    let categories_json = serde_json::to_string(&request.categories)
        .map_err(|e| OmsError::Internal(format!("failed to serialize categories: {e}")))?;
    let source_json = request
        .source
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|e| OmsError::Internal(format!("failed to serialize source: {e}")))?;
    let access_policy = access_policy_to_str(&request.access_control.policy);
    let allowed_agents_json = request
        .access_control
        .allowed_agents
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|e| OmsError::Internal(format!("failed to serialize allowed_agents: {e}")))?;
    let allowed_scopes_json = request
        .access_control
        .allowed_scopes
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|e| OmsError::Internal(format!("failed to serialize allowed_scopes: {e}")))?;
    let content_json = serde_json::to_string(&request.content)
        .map_err(|e| OmsError::Internal(format!("failed to serialize content: {e}")))?;
    let layer_str = request.layer.to_string();
    let expires_str = request.expires_at.map(|t| t.to_rfc3339());
    let valid_from_str = request.valid_from.as_ref().map(DateTime::to_rfc3339);
    let valid_until_str = request.valid_until.as_ref().map(DateTime::to_rfc3339);

    sqlx::query(
        "INSERT INTO memories (
            id, store_id, tenant_id, layer, content_json, embedding,
            owner_agent_id,
            scope_tenant_id, scope_org_id, scope_team_id, scope_project_id,
            scope_user_id, scope_agent_id, scope_session_id, scope_run_id,
            tags_json, categories_json, source_json,
            access_policy, allowed_agents_json, allowed_scopes_json,
            created_at, updated_at, expires_at, immutable, version,
            valid_from, valid_until, confidence, entity_type, upsert_key
        ) VALUES (
            ?, ?, ?, ?, ?, ?,
            ?,
            ?, ?, ?, ?,
            ?, ?, ?, ?,
            ?, ?, ?,
            ?, ?, ?,
            ?, ?, ?, ?, 1,
            ?, ?, ?, ?, ?
        )",
    )
    .bind(id.to_string())
    .bind(store_id.to_string())
    .bind(tenant_id)
    .bind(&layer_str)
    .bind(&content_json)
    .bind(&embedding_blob)
    .bind(&request.owner_agent_id)
    .bind(&request.scope.tenant_id)
    .bind(&request.scope.org_id)
    .bind(&request.scope.team_id)
    .bind(&request.scope.project_id)
    .bind(&request.scope.user_id)
    .bind(&request.scope.agent_id)
    .bind(&request.scope.session_id)
    .bind(&request.scope.run_id)
    .bind(&tags_json)
    .bind(&categories_json)
    .bind(&source_json)
    .bind(access_policy)
    .bind(&allowed_agents_json)
    .bind(&allowed_scopes_json)
    .bind(&now_str)
    .bind(&now_str)
    .bind(&expires_str)
    .bind(request.immutable)
    .bind(&valid_from_str)
    .bind(&valid_until_str)
    .bind(request.confidence)
    .bind(&request.entity_type)
    .bind(&request.upsert_key)
    .execute(&mut *conn)
    .await
    .map_err(|e| map_db_error("insert memory", e))?;

    let entry = MemoryEntry {
        id,
        store_id,
        layer: request.layer,
        content: request.content,
        embedding: request.embedding,
        owner_agent_id: request.owner_agent_id,
        scope: request.scope,
        tags: request.tags,
        categories: request.categories,
        source: request.source,
        access_control: request.access_control,
        created_at: now,
        updated_at: now,
        expires_at: request.expires_at,
        immutable: request.immutable,
        version: 1,
        valid_from: request.valid_from,
        valid_until: request.valid_until,
        confidence: request.confidence,
        entity_type: request.entity_type,
        upsert_key: request.upsert_key,
    };

    crate::audit::log_audit_on_conn(
        pool,
        conn,
        tenant_id,
        store_id,
        Some(entry.id),
        "create",
        Some(entry.owner_agent_id.as_str()),
        Some(serde_json::json!({"version": entry.version})),
    )
    .await?;

    Ok(entry)
}

pub(crate) async fn create_memory(
    pool: &SqlitePool,
    tenant_id: &str,
    store_id: Uuid,
    request: CreateMemoryRequest,
) -> Result<MemoryEntry, OmsError> {
    // Verify store exists and belongs to tenant
    crate::stores::get_store(pool, tenant_id, store_id).await?;

    // Normalize scope: override tenant_id with authenticated value
    let mut request = request;
    request.scope = request.scope.normalize(tenant_id);

    let id = Uuid::new_v4();
    let now = Utc::now();
    let now_str = now.to_rfc3339();
    let embedding_blob = request.embedding.as_ref().map(|v| embedding_to_bytes(v));
    let tags_json = serde_json::to_string(&request.tags)
        .map_err(|e| OmsError::Internal(format!("failed to serialize tags: {e}")))?;
    let categories_json = serde_json::to_string(&request.categories)
        .map_err(|e| OmsError::Internal(format!("failed to serialize categories: {e}")))?;
    let source_json = request
        .source
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|e| OmsError::Internal(format!("failed to serialize source: {e}")))?;
    let access_policy = access_policy_to_str(&request.access_control.policy);
    let allowed_agents_json = request
        .access_control
        .allowed_agents
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|e| OmsError::Internal(format!("failed to serialize allowed_agents: {e}")))?;
    let allowed_scopes_json = request
        .access_control
        .allowed_scopes
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|e| OmsError::Internal(format!("failed to serialize allowed_scopes: {e}")))?;
    let content_json = serde_json::to_string(&request.content)
        .map_err(|e| OmsError::Internal(format!("failed to serialize content: {e}")))?;
    let layer_str = request.layer.to_string();
    let expires_str = request.expires_at.map(|t| t.to_rfc3339());
    let valid_from_str = request.valid_from.as_ref().map(DateTime::to_rfc3339);
    let valid_until_str = request.valid_until.as_ref().map(DateTime::to_rfc3339);

    let mut conn = pool
        .acquire()
        .await
        .map_err(|e| OmsError::Internal(format!("failed to acquire connection: {e}")))?;

    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *conn)
        .await
        .map_err(|e| map_db_error("begin transaction", e))?;

    // Upsert: if upsert_key is set, look for an existing entry to replace.
    // Match on full normalized scope so that the same upsert_key in different
    // scopes creates distinct entries.
    if let Some(ref upsert_key) = request.upsert_key {
        let existing: Option<sqlx::sqlite::SqliteRow> = sqlx::query(
            "SELECT id, version, content_json, created_at FROM memories
             WHERE store_id = ? AND layer = ? AND upsert_key = ?
               AND scope_tenant_id = ?
               AND COALESCE(scope_org_id, '') = COALESCE(?, '')
               AND COALESCE(scope_team_id, '') = COALESCE(?, '')
               AND COALESCE(scope_project_id, '') = COALESCE(?, '')
               AND COALESCE(scope_user_id, '') = COALESCE(?, '')
               AND COALESCE(scope_agent_id, '') = COALESCE(?, '')
               AND COALESCE(scope_session_id, '') = COALESCE(?, '')
               AND COALESCE(scope_run_id, '') = COALESCE(?, '')
             LIMIT 1",
        )
        .bind(store_id.to_string())
        .bind(&layer_str)
        .bind(upsert_key)
        .bind(&request.scope.tenant_id)
        .bind(&request.scope.org_id)
        .bind(&request.scope.team_id)
        .bind(&request.scope.project_id)
        .bind(&request.scope.user_id)
        .bind(&request.scope.agent_id)
        .bind(&request.scope.session_id)
        .bind(&request.scope.run_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(|e| map_db_error("check upsert key", e))?;

        if let Some(row) = existing {
            let existing_id_str: String = row.get("id");
            let existing_id = Uuid::parse_str(&existing_id_str)
                .map_err(|e| OmsError::Internal(format!("invalid existing id: {e}")))?;
            let existing_version: i64 = row.get("version");
            let prev_content: String = row.get("content_json");
            let original_created_at: String = row.get("created_at");
            let new_version = existing_version + 1;

            if let Err(e) = sqlx::query(
                "UPDATE memories SET
                    content_json = ?, embedding = ?, tags_json = ?, categories_json = ?,
                    source_json = ?, access_policy = ?, allowed_agents_json = ?,
                    allowed_scopes_json = ?,
                    scope_tenant_id = ?, scope_org_id = ?, scope_team_id = ?,
                    scope_project_id = ?, scope_user_id = ?, scope_agent_id = ?,
                    scope_session_id = ?, scope_run_id = ?,
                    updated_at = ?, expires_at = ?,
                    valid_from = ?, valid_until = ?, confidence = ?, entity_type = ?,
                    owner_agent_id = ?, version = ?
                 WHERE id = ?",
            )
            .bind(&content_json)
            .bind(&embedding_blob)
            .bind(&tags_json)
            .bind(&categories_json)
            .bind(&source_json)
            .bind(access_policy)
            .bind(&allowed_agents_json)
            .bind(&allowed_scopes_json)
            // Persist scope columns (ensures normalization changes are applied)
            .bind(&request.scope.tenant_id)
            .bind(&request.scope.org_id)
            .bind(&request.scope.team_id)
            .bind(&request.scope.project_id)
            .bind(&request.scope.user_id)
            .bind(&request.scope.agent_id)
            .bind(&request.scope.session_id)
            .bind(&request.scope.run_id)
            .bind(&now_str)
            .bind(&expires_str)
            .bind(&valid_from_str)
            .bind(&valid_until_str)
            .bind(request.confidence)
            .bind(&request.entity_type)
            .bind(&request.owner_agent_id)
            .bind(new_version)
            .bind(&existing_id_str)
            .execute(&mut *conn)
            .await
            {
                let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
                return Err(OmsError::Internal(format!("failed to upsert memory: {e}")));
            }

            // Return DB-truth: use original created_at, not current time
            let original_created = DateTime::parse_from_rfc3339(&original_created_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or(now);

            let entry = MemoryEntry {
                id: existing_id,
                store_id,
                layer: request.layer,
                content: request.content,
                embedding: request.embedding,
                owner_agent_id: request.owner_agent_id,
                scope: request.scope,
                tags: request.tags,
                categories: request.categories,
                source: request.source,
                access_control: request.access_control,
                created_at: original_created,
                updated_at: now,
                expires_at: request.expires_at,
                immutable: request.immutable,
                version: new_version,
                valid_from: request.valid_from,
                valid_until: request.valid_until,
                confidence: request.confidence,
                entity_type: request.entity_type,
                upsert_key: request.upsert_key,
            };

            if let Err(e) = crate::audit::log_audit_on_conn(
                pool,
                &mut conn,
                tenant_id,
                store_id,
                Some(entry.id),
                "upsert",
                Some(entry.owner_agent_id.as_str()),
                Some(serde_json::json!({
                    "version": entry.version,
                    "previous_content": prev_content,
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
                .map_err(|e| map_db_error("commit", e))?;

            return Ok(entry);
        }
    }

    if let Err(e) = sqlx::query(
        "INSERT INTO memories (
            id, store_id, tenant_id, layer, content_json, embedding,
            owner_agent_id,
            scope_tenant_id, scope_org_id, scope_team_id, scope_project_id,
            scope_user_id, scope_agent_id, scope_session_id, scope_run_id,
            tags_json, categories_json, source_json,
            access_policy, allowed_agents_json, allowed_scopes_json,
            created_at, updated_at, expires_at, immutable, version,
            valid_from, valid_until, confidence, entity_type, upsert_key
        ) VALUES (
            ?, ?, ?, ?, ?, ?,
            ?,
            ?, ?, ?, ?,
            ?, ?, ?, ?,
            ?, ?, ?,
            ?, ?, ?,
            ?, ?, ?, ?, 1,
            ?, ?, ?, ?, ?
        )",
    )
    .bind(id.to_string())
    .bind(store_id.to_string())
    .bind(tenant_id)
    .bind(&layer_str)
    .bind(&content_json)
    .bind(&embedding_blob)
    .bind(&request.owner_agent_id)
    .bind(&request.scope.tenant_id)
    .bind(&request.scope.org_id)
    .bind(&request.scope.team_id)
    .bind(&request.scope.project_id)
    .bind(&request.scope.user_id)
    .bind(&request.scope.agent_id)
    .bind(&request.scope.session_id)
    .bind(&request.scope.run_id)
    .bind(&tags_json)
    .bind(&categories_json)
    .bind(&source_json)
    .bind(access_policy)
    .bind(&allowed_agents_json)
    .bind(&allowed_scopes_json)
    .bind(&now_str)
    .bind(&now_str)
    .bind(&expires_str)
    .bind(request.immutable)
    .bind(&valid_from_str)
    .bind(&valid_until_str)
    .bind(request.confidence)
    .bind(&request.entity_type)
    .bind(&request.upsert_key)
    .execute(&mut *conn)
    .await
    {
        let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
        return Err(map_db_error("insert memory", e));
    }

    let entry = MemoryEntry {
        id,
        store_id,
        layer: request.layer,
        content: request.content,
        embedding: request.embedding,
        owner_agent_id: request.owner_agent_id,
        scope: request.scope,
        tags: request.tags,
        categories: request.categories,
        source: request.source,
        access_control: request.access_control,
        created_at: now,
        updated_at: now,
        expires_at: request.expires_at,
        immutable: request.immutable,
        version: 1,
        valid_from: request.valid_from,
        valid_until: request.valid_until,
        confidence: request.confidence,
        entity_type: request.entity_type,
        upsert_key: request.upsert_key,
    };

    if let Err(e) = crate::audit::log_audit_on_conn(
        pool,
        &mut conn,
        tenant_id,
        store_id,
        Some(entry.id),
        "create",
        Some(entry.owner_agent_id.as_str()),
        Some(serde_json::json!({"version": entry.version})),
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

    Ok(entry)
}

pub(crate) async fn get_memory(
    pool: &SqlitePool,
    tenant_id: &str,
    store_id: Uuid,
    memory_id: Uuid,
) -> Result<MemoryEntry, OmsError> {
    let row = sqlx::query("SELECT * FROM memories WHERE id = ? AND store_id = ? AND tenant_id = ?")
        .bind(memory_id.to_string())
        .bind(store_id.to_string())
        .bind(tenant_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| map_db_error("query memory", e))?
        .ok_or_else(|| OmsError::MemoryNotFound(memory_id.to_string()))?;

    row_to_memory(&row)
}

pub(crate) async fn list_memories(
    pool: &SqlitePool,
    tenant_id: &str,
    store_id: Uuid,
    filter: ListMemoriesFilter,
) -> Result<Page<MemoryEntry>, OmsError> {
    let limit = filter.limit.unwrap_or(50).min(1000);
    let offset = filter.offset.unwrap_or(0);

    // Build dynamic query
    let mut conditions = vec!["store_id = ?".to_string(), "tenant_id = ?".to_string()];
    let mut bind_values: Vec<String> = vec![store_id.to_string(), tenant_id.to_string()];

    if let Some(layer) = &filter.layer {
        conditions.push("layer = ?".to_string());
        bind_values.push(layer.to_string());
    }
    if let Some(owner) = &filter.owner_agent_id {
        conditions.push("owner_agent_id = ?".to_string());
        bind_values.push(owner.clone());
    }
    if let Some(tags) = &filter.tags {
        for tag in tags {
            conditions.push("tags_json LIKE ? ESCAPE '\\'".to_string());
            bind_values.push(format!("%\"{}\"%", escape_like(tag)));
        }
    }
    if let Some(categories) = &filter.categories {
        for category in categories {
            conditions.push("categories_json LIKE ? ESCAPE '\\'".to_string());
            bind_values.push(format!("%\"{}\"%", escape_like(category)));
        }
    }
    if let Some(scope) = &filter.scope {
        if let Some(org_id) = &scope.org_id {
            conditions.push("scope_org_id = ?".to_string());
            bind_values.push(org_id.clone());
        }
        if let Some(team_id) = &scope.team_id {
            conditions.push("scope_team_id = ?".to_string());
            bind_values.push(team_id.clone());
        }
        if let Some(project_id) = &scope.project_id {
            conditions.push("scope_project_id = ?".to_string());
            bind_values.push(project_id.clone());
        }
        if let Some(user_id) = &scope.user_id {
            conditions.push("scope_user_id = ?".to_string());
            bind_values.push(user_id.clone());
        }
        if let Some(agent_id) = &scope.agent_id {
            conditions.push("scope_agent_id = ?".to_string());
            bind_values.push(agent_id.clone());
        }
        if let Some(session_id) = &scope.session_id {
            conditions.push("scope_session_id = ?".to_string());
            bind_values.push(session_id.clone());
        }
        if let Some(run_id) = &scope.run_id {
            conditions.push("scope_run_id = ?".to_string());
            bind_values.push(run_id.clone());
        }
    }

    let where_clause = conditions.join(" AND ");

    // Count query
    let count_sql = format!("SELECT COUNT(*) as cnt FROM memories WHERE {where_clause}");
    let mut count_query = sqlx::query(&count_sql);
    for v in &bind_values {
        count_query = count_query.bind(v);
    }
    let count_row = count_query
        .fetch_one(pool)
        .await
        .map_err(|e| map_db_error("count memories", e))?;
    let total: i64 = count_row.get("cnt");

    // Data query
    let data_sql = format!(
        "SELECT * FROM memories WHERE {where_clause} ORDER BY created_at DESC LIMIT ? OFFSET ?"
    );
    let mut data_query = sqlx::query(&data_sql);
    for v in &bind_values {
        data_query = data_query.bind(v);
    }
    data_query = data_query.bind(limit as i64).bind(offset as i64);

    let rows = data_query
        .fetch_all(pool)
        .await
        .map_err(|e| map_db_error("list memories", e))?;

    let items: Result<Vec<MemoryEntry>, OmsError> = rows.iter().map(row_to_memory).collect();

    Ok(Page {
        items: items?,
        total: total as u64,
        limit,
        offset,
    })
}

pub(crate) async fn update_memory(
    pool: &SqlitePool,
    tenant_id: &str,
    store_id: Uuid,
    memory_id: Uuid,
    request: UpdateMemoryRequest,
) -> Result<MemoryEntry, OmsError> {
    let existing = crate::memories::get_memory(pool, tenant_id, store_id, memory_id).await?;

    if existing.immutable {
        return Err(OmsError::Immutable(memory_id.to_string()));
    }

    let content = request.content.unwrap_or(existing.content);
    // Double-option: None = keep existing, Some(None) = clear, Some(Some(v)) = set
    let embedding = match request.embedding {
        Some(v) => v,
        None => existing.embedding,
    };
    let tags = request.tags.unwrap_or(existing.tags);
    let categories = request.categories.unwrap_or(existing.categories);
    let access_control = request.access_control.unwrap_or(existing.access_control);
    let expires_at = match request.expires_at {
        Some(v) => v,
        None => existing.expires_at,
    };
    let new_version = existing.version + 1;
    let now_str = Utc::now().to_rfc3339();

    let content_json = serde_json::to_string(&content)
        .map_err(|e| OmsError::Internal(format!("failed to serialize content: {e}")))?;
    let embedding_blob = embedding.as_ref().map(|v| embedding_to_bytes(v));
    let tags_json = serde_json::to_string(&tags)
        .map_err(|e| OmsError::Internal(format!("failed to serialize tags: {e}")))?;
    let categories_json = serde_json::to_string(&categories)
        .map_err(|e| OmsError::Internal(format!("failed to serialize categories: {e}")))?;
    let access_policy = access_policy_to_str(&access_control.policy);
    let allowed_agents_json = access_control
        .allowed_agents
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|e| OmsError::Internal(format!("failed to serialize allowed_agents: {e}")))?;
    let allowed_scopes_json = access_control
        .allowed_scopes
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|e| OmsError::Internal(format!("failed to serialize allowed_scopes: {e}")))?;
    let expires_str = expires_at.map(|t| t.to_rfc3339());

    let mut conn = pool
        .acquire()
        .await
        .map_err(|e| OmsError::Internal(format!("failed to acquire connection: {e}")))?;

    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *conn)
        .await
        .map_err(|e| map_db_error("begin transaction", e))?;

    let result = match sqlx::query(
        "UPDATE memories SET
            content_json = ?, embedding = ?,
            tags_json = ?, categories_json = ?,
            access_policy = ?, allowed_agents_json = ?, allowed_scopes_json = ?,
            expires_at = ?, updated_at = ?, version = ?
         WHERE id = ? AND store_id = ? AND tenant_id = ? AND version = ?",
    )
    .bind(&content_json)
    .bind(&embedding_blob)
    .bind(&tags_json)
    .bind(&categories_json)
    .bind(access_policy)
    .bind(&allowed_agents_json)
    .bind(&allowed_scopes_json)
    .bind(&expires_str)
    .bind(&now_str)
    .bind(new_version)
    .bind(memory_id.to_string())
    .bind(store_id.to_string())
    .bind(tenant_id)
    .bind(existing.version)
    .execute(&mut *conn)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            return Err(OmsError::Internal(format!("failed to update memory: {e}")));
        }
    };

    if result.rows_affected() == 0 {
        let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
        return Err(OmsError::Conflict(format!(
            "memory {memory_id} was modified concurrently (expected version {})",
            existing.version
        )));
    }

    if let Err(e) = crate::audit::log_audit_on_conn(
        pool,
        &mut conn,
        tenant_id,
        store_id,
        Some(memory_id),
        "update",
        Some(existing.owner_agent_id.as_str()),
        Some(serde_json::json!({"version": new_version})),
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

    // Re-fetch the updated entry
    crate::memories::get_memory(pool, tenant_id, store_id, memory_id).await
}

pub(crate) async fn delete_memory(
    pool: &SqlitePool,
    tenant_id: &str,
    store_id: Uuid,
    memory_id: Uuid,
) -> Result<(), OmsError> {
    let existing = crate::memories::get_memory(pool, tenant_id, store_id, memory_id).await?;

    let mut conn = pool
        .acquire()
        .await
        .map_err(|e| OmsError::Internal(format!("failed to acquire connection: {e}")))?;

    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *conn)
        .await
        .map_err(|e| map_db_error("begin transaction", e))?;

    let result =
        match sqlx::query("DELETE FROM memories WHERE id = ? AND store_id = ? AND tenant_id = ?")
            .bind(memory_id.to_string())
            .bind(store_id.to_string())
            .bind(tenant_id)
            .execute(&mut *conn)
            .await
        {
            Ok(r) => r,
            Err(e) => {
                let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
                return Err(OmsError::Internal(format!("failed to delete memory: {e}")));
            }
        };

    if result.rows_affected() == 0 {
        let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
        return Err(OmsError::MemoryNotFound(memory_id.to_string()));
    }

    if let Err(e) = crate::audit::log_audit_on_conn(
        pool,
        &mut conn,
        tenant_id,
        store_id,
        Some(existing.id),
        "delete",
        Some(existing.owner_agent_id.as_str()),
        Some(serde_json::json!({"version": existing.version})),
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
