use chrono::{DateTime, Utc};
use kd6_core::error::OmsError;
use kd6_core::models::{CreateEdgeRequest, GraphEdge, GraphTraversalRequest, GraphTraversalResult};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::audit::log_audit_on_conn;
use crate::helpers::map_db_error;

use crate::memories::row_to_memory;

pub(crate) async fn create_edge(
    pool: &SqlitePool,
    tenant_id: &str,
    store_id: Uuid,
    request: CreateEdgeRequest,
) -> Result<GraphEdge, OmsError> {
    crate::stores::get_store(pool, tenant_id, store_id).await?;
    crate::memories::get_memory(pool, tenant_id, store_id, request.source_memory_id).await?;
    crate::memories::get_memory(pool, tenant_id, store_id, request.target_memory_id).await?;

    let id = Uuid::new_v4();
    let now = Utc::now();
    let metadata_json = serde_json::to_string(&request.metadata)
        .map_err(|e| OmsError::Internal(format!("failed to serialize edge metadata: {e}")))?;

    let mut conn = pool
        .acquire()
        .await
        .map_err(|e| OmsError::Internal(format!("failed to acquire connection: {e}")))?;

    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *conn)
        .await
        .map_err(|e| map_db_error("begin transaction", e))?;

    if let Err(e) = sqlx::query(
        "INSERT INTO graph_edges (id, store_id, tenant_id, source_memory_id, target_memory_id, relation_type, weight, metadata_json, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(store_id.to_string())
    .bind(tenant_id)
    .bind(request.source_memory_id.to_string())
    .bind(request.target_memory_id.to_string())
    .bind(&request.relation_type)
    .bind(request.weight)
    .bind(&metadata_json)
    .bind(now.to_rfc3339())
    .execute(&mut *conn)
    .await
    {
        let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
        return Err(map_db_error("insert graph edge", e));
    }

    if let Err(e) = log_audit_on_conn(
        pool,
        &mut conn,
        tenant_id,
        store_id,
        None,
        "create_edge",
        None,
        Some(serde_json::json!({
            "entity": "graph_edge",
            "edge_id": id.to_string(),
            "source_memory_id": request.source_memory_id.to_string(),
            "target_memory_id": request.target_memory_id.to_string(),
            "relation_type": &request.relation_type,
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

    Ok(GraphEdge {
        id,
        store_id,
        tenant_id: tenant_id.to_string(),
        source_memory_id: request.source_memory_id,
        target_memory_id: request.target_memory_id,
        relation_type: request.relation_type,
        weight: request.weight,
        metadata: request.metadata,
        created_at: now,
    })
}

pub(crate) async fn delete_edge(
    pool: &SqlitePool,
    tenant_id: &str,
    store_id: Uuid,
    edge_id: Uuid,
) -> Result<(), OmsError> {
    let result =
        sqlx::query("DELETE FROM graph_edges WHERE id = ? AND store_id = ? AND tenant_id = ?")
            .bind(edge_id.to_string())
            .bind(store_id.to_string())
            .bind(tenant_id)
            .execute(pool)
            .await
            .map_err(|e| map_db_error("delete graph edge", e))?;

    if result.rows_affected() == 0 {
        return Err(OmsError::InvalidInput(format!(
            "graph edge not found: {edge_id}"
        )));
    }
    Ok(())
}

pub(crate) async fn graph_traverse(
    pool: &SqlitePool,
    tenant_id: &str,
    store_id: Uuid,
    request: GraphTraversalRequest,
) -> Result<GraphTraversalResult, OmsError> {
    use std::collections::{HashSet, VecDeque};

    const MAX_TRAVERSAL_DEPTH: u32 = 10;
    let depth = request.depth.min(MAX_TRAVERSAL_DEPTH);

    let start =
        crate::memories::get_memory(pool, tenant_id, store_id, request.start_memory_id).await?;

    let mut visited: HashSet<Uuid> = HashSet::new();
    let mut seen_edges: HashSet<Uuid> = HashSet::new();
    let mut queue: VecDeque<(Uuid, u32)> = VecDeque::new();
    let mut result_nodes = vec![start];
    let mut result_edges = Vec::new();

    visited.insert(request.start_memory_id);
    queue.push_back((request.start_memory_id, 0));

    const MAX_TRAVERSAL_NODES: usize = 1000;
    const MAX_EDGES_PER_NODE: i64 = 500;

    while let Some((current_id, current_depth)) = queue.pop_front() {
        if current_depth >= depth {
            continue;
        }
        if visited.len() >= MAX_TRAVERSAL_NODES {
            break;
        }

        let mut new_neighbors: Vec<Uuid> = Vec::new();
        let mut query = String::from(
            "SELECT id, store_id, tenant_id, source_memory_id, target_memory_id, relation_type, weight, metadata_json, created_at
             FROM graph_edges
             WHERE store_id = ? AND tenant_id = ? AND (source_memory_id = ? OR target_memory_id = ?)",
        );

        if let Some(ref types) = request.relation_types {
            if !types.is_empty() {
                let placeholders: Vec<&str> = types.iter().map(|_| "?").collect();
                query.push_str(&format!(
                    " AND relation_type IN ({})",
                    placeholders.join(",")
                ));
            }
        }

        query.push_str(&format!(" LIMIT {MAX_EDGES_PER_NODE}"));

        let mut q = sqlx::query(&query)
            .bind(store_id.to_string())
            .bind(tenant_id)
            .bind(current_id.to_string())
            .bind(current_id.to_string());

        if let Some(ref types) = request.relation_types {
            for t in types {
                q = q.bind(t);
            }
        }

        let rows = q
            .fetch_all(pool)
            .await
            .map_err(|e| map_db_error("query graph edges", e))?;

        for row in &rows {
            let edge_id_str: String = row.get("id");
            let source_str: String = row.get("source_memory_id");
            let target_str: String = row.get("target_memory_id");
            let metadata_str: String = row.get("metadata_json");
            let created_str: String = row.get("created_at");

            let edge_id = Uuid::parse_str(&edge_id_str)
                .map_err(|e| OmsError::Internal(format!("invalid edge id: {e}")))?;
            let source_id = Uuid::parse_str(&source_str)
                .map_err(|e| OmsError::Internal(format!("invalid source id: {e}")))?;
            let target_id = Uuid::parse_str(&target_str)
                .map_err(|e| OmsError::Internal(format!("invalid target id: {e}")))?;

            let neighbor_id = if source_id == current_id {
                target_id
            } else {
                source_id
            };

            if seen_edges.insert(edge_id) {
                result_edges.push(GraphEdge {
                    id: edge_id,
                    store_id,
                    tenant_id: tenant_id.to_string(),
                    source_memory_id: source_id,
                    target_memory_id: target_id,
                    relation_type: row.get("relation_type"),
                    weight: row.get("weight"),
                    metadata: serde_json::from_str(&metadata_str)
                        .map_err(|e| OmsError::Internal(format!("invalid edge metadata: {e}")))?,
                    created_at: DateTime::parse_from_rfc3339(&created_str)
                        .map_err(|e| OmsError::Internal(format!("invalid edge created_at: {e}")))?
                        .with_timezone(&Utc),
                });
            }

            if !visited.contains(&neighbor_id) {
                visited.insert(neighbor_id);
                new_neighbors.push(neighbor_id);
                queue.push_back((neighbor_id, current_depth + 1));
            }
        }

        // Batch-fetch all new neighbor nodes in a single query
        if !new_neighbors.is_empty() {
            let placeholders: Vec<&str> = new_neighbors.iter().map(|_| "?").collect();
            let batch_sql = format!(
                "SELECT * FROM memories WHERE id IN ({}) AND store_id = ? AND tenant_id = ?",
                placeholders.join(",")
            );
            let mut batch_q = sqlx::query(&batch_sql);
            for nid in &new_neighbors {
                batch_q = batch_q.bind(nid.to_string());
            }
            batch_q = batch_q.bind(store_id.to_string()).bind(tenant_id);

            let node_rows = batch_q.fetch_all(pool).await.map_err(|e| {
                OmsError::Internal(format!("failed to batch-fetch graph nodes: {e}"))
            })?;
            for node_row in &node_rows {
                if let Ok(node) = row_to_memory(node_row) {
                    result_nodes.push(node);
                }
            }
        }
    }

    Ok(GraphTraversalResult {
        nodes: result_nodes,
        edges: result_edges,
    })
}
