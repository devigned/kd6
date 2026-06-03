use chrono::{DateTime, Utc};
use kd6_core::error::OmsError;
use kd6_core::models::{
    AccessControl, CreateInheritanceRequest, CreateMemoryRequest, InheritanceSpec, MemoryEntry,
    MemoryLayer, MemoryScope, SourceReference,
};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::audit::log_audit_on_conn;
use crate::helpers::{
    escape_like, inheritance_access_to_str, map_db_error, parse_inheritance_access,
};
use crate::memories::{insert_memory_on_conn, row_to_memory};

pub(crate) fn row_to_inheritance(
    row: &sqlx::sqlite::SqliteRow,
) -> Result<InheritanceSpec, OmsError> {
    let id_str: String = row.get("id");
    let store_id_str: String = row.get("store_id");
    let inherit_layers_json: String = row.get("inherit_layers_json");
    let filter_json: String = row.get("filter_json");
    let bubble_up_json: String = row.get("bubble_up_json");
    let created_at: String = row.get("created_at");

    Ok(InheritanceSpec {
        id: Uuid::parse_str(&id_str)
            .map_err(|e| OmsError::Internal(format!("invalid inheritance id: {e}")))?,
        store_id: Uuid::parse_str(&store_id_str)
            .map_err(|e| OmsError::Internal(format!("invalid inheritance store id: {e}")))?,
        tenant_id: row.get("tenant_id"),
        parent_agent_id: row.get("parent_agent_id"),
        child_agent_id: row.get("child_agent_id"),
        inherit_layers: serde_json::from_str(&inherit_layers_json)
            .map_err(|e| OmsError::Internal(format!("invalid inherit_layers json: {e}")))?,
        filter: serde_json::from_str(&filter_json)
            .map_err(|e| OmsError::Internal(format!("invalid inheritance filter json: {e}")))?,
        access: parse_inheritance_access(&row.get::<String, _>("access"))?,
        bubble_up: serde_json::from_str(&bubble_up_json)
            .map_err(|e| OmsError::Internal(format!("invalid bubble_up json: {e}")))?,
        created_at: DateTime::parse_from_rfc3339(&created_at)
            .map_err(|e| OmsError::Internal(format!("invalid inheritance created_at: {e}")))?
            .with_timezone(&Utc),
    })
}

pub(crate) async fn create_inheritance(
    pool: &SqlitePool,
    tenant_id: &str,
    store_id: Uuid,
    request: CreateInheritanceRequest,
) -> Result<InheritanceSpec, OmsError> {
    crate::stores::get_store(pool, tenant_id, store_id).await?;

    if request.parent_agent_id == request.child_agent_id {
        return Err(OmsError::InvalidInput(
            "parent_agent_id and child_agent_id must be different".into(),
        ));
    }

    let inheritance = InheritanceSpec {
        id: Uuid::new_v4(),
        store_id,
        tenant_id: tenant_id.to_string(),
        parent_agent_id: request.parent_agent_id,
        child_agent_id: request.child_agent_id,
        inherit_layers: request.inherit_layers,
        filter: request.filter,
        access: request.access,
        bubble_up: request.bubble_up,
        created_at: Utc::now(),
    };

    let mut conn = pool
        .acquire()
        .await
        .map_err(|e| OmsError::Internal(format!("failed to acquire connection: {e}")))?;

    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *conn)
        .await
        .map_err(|e| map_db_error("begin transaction", e))?;

    if let Err(e) = sqlx::query(
        "INSERT INTO inheritance (id, store_id, tenant_id, parent_agent_id, child_agent_id, inherit_layers_json, filter_json, access, bubble_up_json, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(inheritance.id.to_string())
    .bind(store_id.to_string())
    .bind(tenant_id)
    .bind(&inheritance.parent_agent_id)
    .bind(&inheritance.child_agent_id)
    .bind(
        serde_json::to_string(&inheritance.inherit_layers)
            .map_err(|e| OmsError::Internal(format!("failed to serialize inherit layers: {e}")))?,
    )
    .bind(
        serde_json::to_string(&inheritance.filter)
            .map_err(|e| OmsError::Internal(format!("failed to serialize inheritance filter: {e}")))?,
    )
    .bind(inheritance_access_to_str(inheritance.access))
    .bind(
        serde_json::to_string(&inheritance.bubble_up)
            .map_err(|e| OmsError::Internal(format!("failed to serialize bubble_up config: {e}")))?,
    )
    .bind(inheritance.created_at.to_rfc3339())
    .execute(&mut *conn)
    .await
    {
        let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
        return Err(map_db_error("create inheritance", e));
    }

    if let Err(e) = log_audit_on_conn(
        pool,
        &mut conn,
        tenant_id,
        store_id,
        None,
        "create_inheritance",
        Some(inheritance.child_agent_id.as_str()),
        Some(serde_json::json!({
            "entity": "inheritance",
            "inheritance_id": inheritance.id.to_string(),
            "parent_agent_id": &inheritance.parent_agent_id,
            "child_agent_id": &inheritance.child_agent_id,
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

    Ok(inheritance)
}

pub(crate) async fn delete_inheritance(
    pool: &SqlitePool,
    tenant_id: &str,
    store_id: Uuid,
    inheritance_id: Uuid,
) -> Result<(), OmsError> {
    let result =
        sqlx::query("DELETE FROM inheritance WHERE id = ? AND store_id = ? AND tenant_id = ?")
            .bind(inheritance_id.to_string())
            .bind(store_id.to_string())
            .bind(tenant_id)
            .execute(pool)
            .await
            .map_err(|e| map_db_error("delete inheritance", e))?;

    if result.rows_affected() == 0 {
        return Err(OmsError::InvalidInput(format!(
            "inheritance not found: {inheritance_id}"
        )));
    }

    Ok(())
}

pub(crate) async fn bubble_up(
    pool: &SqlitePool,
    tenant_id: &str,
    store_id: Uuid,
    request: kd6_core::models::BubbleUpRequest,
) -> Result<Vec<MemoryEntry>, OmsError> {
    let inheritance_row = sqlx::query(
        "SELECT * FROM inheritance WHERE store_id = ? AND tenant_id = ? AND parent_agent_id = ? AND child_agent_id = ?",
    )
    .bind(store_id.to_string())
    .bind(tenant_id)
    .bind(&request.parent_agent_id)
    .bind(&request.child_agent_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| map_db_error("query inheritance", e))?
    .ok_or_else(|| {
        OmsError::InvalidInput(format!(
            "inheritance not found for parent {} and child {}",
            request.parent_agent_id, request.child_agent_id
        ))
    })?;
    let inheritance = row_to_inheritance(&inheritance_row)?;

    if !inheritance.bubble_up.enabled {
        return Err(OmsError::InvalidInput(
            "bubble up is not enabled for this inheritance".into(),
        ));
    }

    let mut requested_layers = if request.layers.is_empty() {
        if inheritance.bubble_up.layers.is_empty() {
            inheritance.inherit_layers.clone()
        } else {
            inheritance.bubble_up.layers.clone()
        }
    } else {
        request.layers.clone()
    };

    if !inheritance.inherit_layers.is_empty() {
        requested_layers.retain(|layer| inheritance.inherit_layers.contains(layer));
    }
    if !inheritance.bubble_up.layers.is_empty() {
        requested_layers.retain(|layer| inheritance.bubble_up.layers.contains(layer));
    }
    if requested_layers.is_empty() {
        return Ok(Vec::new());
    }

    let mut conditions = vec![
        "store_id = ?".to_string(),
        "tenant_id = ?".to_string(),
        "owner_agent_id = ?".to_string(),
    ];
    let mut bind_values = vec![
        store_id.to_string(),
        tenant_id.to_string(),
        request.child_agent_id.clone(),
    ];

    let layer_placeholders: Vec<&str> = requested_layers.iter().map(|_| "?").collect();
    conditions.push(format!("layer IN ({})", layer_placeholders.join(",")));
    for layer in &requested_layers {
        bind_values.push(layer.to_string());
    }

    if let Some(tags) = &inheritance.filter.tags {
        for tag in tags {
            conditions.push("tags_json LIKE ? ESCAPE '\\'".to_string());
            bind_values.push(format!("%\"{}\"%", escape_like(tag)));
        }
    }
    if let Some(categories) = &inheritance.filter.categories {
        for category in categories {
            conditions.push("categories_json LIKE ? ESCAPE '\\'".to_string());
            bind_values.push(format!("%\"{}\"%", escape_like(category)));
        }
    }
    if let Some(time_from) = inheritance.filter.time_from {
        conditions.push("created_at >= ?".to_string());
        bind_values.push(time_from.to_rfc3339());
    }
    if let Some(time_to) = inheritance.filter.time_to {
        conditions.push("created_at <= ?".to_string());
        bind_values.push(time_to.to_rfc3339());
    }

    let sql = format!(
        "SELECT * FROM memories WHERE {} ORDER BY created_at DESC LIMIT ?",
        conditions.join(" AND ")
    );
    let mut memory_query = sqlx::query(&sql);
    for value in &bind_values {
        memory_query = memory_query.bind(value);
    }
    memory_query = memory_query.bind(inheritance.filter.max_entries.unwrap_or(100) as i64);

    let source_rows = memory_query.fetch_all(pool).await.map_err(|e| {
        OmsError::Internal(format!("failed to query child memories for bubble up: {e}"))
    })?;

    let mut created = Vec::new();

    // Run all reads AND inserts under a single transaction for atomicity
    // and to prevent concurrent duplicates.
    let mut conn = pool
        .acquire()
        .await
        .map_err(|e| OmsError::Internal(format!("failed to acquire connection: {e}")))?;

    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *conn)
        .await
        .map_err(|e| OmsError::Internal(format!("failed to begin bubble_up transaction: {e}")))?;

    let result: Result<(), OmsError> = async {
        // Check which source memories have already been bubbled up to this parent
        // (inside the transaction to prevent concurrent duplicates).
        let mut already_bubbled = std::collections::HashSet::new();
        {
            let existing_sql =
                "SELECT source_json FROM memories WHERE store_id = ? AND tenant_id = ? AND scope_agent_id = ? AND source_json IS NOT NULL";
            let rows = sqlx::query(existing_sql)
                .bind(store_id.to_string())
                .bind(tenant_id)
                .bind(&request.parent_agent_id)
                .fetch_all(&mut *conn)
                .await
                .map_err(|e| {
                    OmsError::Internal(format!("failed to check existing bubbled memories: {e}"))
                })?;
            for row in &rows {
                let source_str: Option<String> = row.get("source_json");
                if let Some(json_str) = source_str {
                    if let Ok(src) = serde_json::from_str::<SourceReference>(&json_str) {
                        if let Some(ref uri) = src.uri {
                            if let Some(ref_id) = uri.strip_prefix("bubble_up:") {
                                if let Ok(uid) = Uuid::parse_str(ref_id) {
                                    already_bubbled.insert(uid);
                                }
                            }
                        }
                    }
                }
            }
        }

        for row in &source_rows {
            let source = row_to_memory(row)?;

            if already_bubbled.contains(&source.id) {
                continue;
            }

            let mut parent_scope = source.scope.clone();
            parent_scope.agent_id = Some(request.parent_agent_id.clone());
            parent_scope.session_id = None;
            parent_scope.run_id = None;

            let memory = insert_memory_on_conn(
                    pool,
                    &mut conn,
                    tenant_id,
                    store_id,
                    CreateMemoryRequest {
                        layer: source.layer,
                        content: source.content,
                        embedding: source.embedding,
                        owner_agent_id: request.parent_agent_id.clone(),
                        scope: parent_scope,
                        tags: source.tags,
                        categories: source.categories,
                        source: Some(SourceReference {
                            conversation_id: None,
                            document_id: None,
                            uri: Some(format!("bubble_up:{}", source.id)),
                        }),
                        access_control: source.access_control,
                        expires_at: source.expires_at,
                        immutable: source.immutable,
                        valid_from: source.valid_from,
                        valid_until: source.valid_until,
                        confidence: source.confidence,
                        entity_type: source.entity_type,
                        upsert_key: source.upsert_key,
                    },
                )
                .await?;
            created.push(memory);
        }

        if let Some(summary) = request.summary {
            let layer = requested_layers
                .first()
                .copied()
                .unwrap_or(MemoryLayer::Working);
            let summary_entry = insert_memory_on_conn(
                    pool,
                    &mut conn,
                    tenant_id,
                    store_id,
                    CreateMemoryRequest {
                        layer,
                        content: summary,
                        embedding: None,
                        owner_agent_id: request.parent_agent_id.clone(),
                        scope: MemoryScope {
                            tenant_id: tenant_id.to_string(),
                            org_id: None,
                            team_id: None,
                            project_id: None,
                            user_id: None,
                            agent_id: Some(request.parent_agent_id.clone()),
                            session_id: None,
                            run_id: None,
                        },
                        tags: vec!["bubble_up".into(), "summary".into()],
                        categories: vec!["summary".into()],
                        source: None,
                        access_control: AccessControl::default(),
                        expires_at: None,
                        immutable: false,
                        valid_from: None,
                        valid_until: None,
                        confidence: None,
                        entity_type: None,
                        upsert_key: None,
                    },
                )
                .await?;
            created.push(summary_entry);
        }

        Ok(())
    }
    .await;

    match result {
        Ok(()) => {
            sqlx::query("COMMIT")
                .execute(&mut *conn)
                .await
                .map_err(|e| map_db_error("commit bubble_up", e))?;
            Ok(created)
        }
        Err(e) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            Err(e)
        }
    }
}
