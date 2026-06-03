use std::collections::HashMap;

use kd6_core::error::OmsError;
use kd6_core::models::{SearchQuery, SearchResult};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::helpers::map_db_error;

use crate::helpers::{build_search_conditions, cosine_similarity, sanitize_fts5_query};
use crate::memories::row_to_memory;

pub(crate) async fn search(
    pool: &SqlitePool,
    tenant_id: &str,
    store_id: Uuid,
    query: SearchQuery,
) -> Result<Vec<SearchResult>, OmsError> {
    let use_keyword_search = query.keyword && !query.query.trim().is_empty();
    let query_embedding = query.embedding.as_ref();

    if query_embedding.is_none() && !use_keyword_search {
        return Err(OmsError::InvalidInput(
            "embedding is required for vector search unless keyword search is enabled".into(),
        ));
    }

    let mut merged_results: HashMap<Uuid, SearchResult> = HashMap::new();

    if let Some(query_embedding) = query_embedding {
        // Cap the candidate set for brute-force vector search to prevent OOM.
        const MAX_VECTOR_SCAN_ROWS: u32 = 10_000;
        let (conditions, bind_values) =
            build_search_conditions("", store_id, tenant_id, &query, true);
        let sql = format!(
            "SELECT * FROM memories WHERE {} LIMIT {}",
            conditions.join(" AND "),
            MAX_VECTOR_SCAN_ROWS
        );
        let mut db_query = sqlx::query(&sql);
        for value in &bind_values {
            db_query = db_query.bind(value);
        }

        let rows = db_query
            .fetch_all(pool)
            .await
            .map_err(|e| map_db_error("search memories", e))?;

        for row in &rows {
            let entry = row_to_memory(row)?;
            if let Some(embedding) = &entry.embedding {
                let score = cosine_similarity(query_embedding, embedding);
                if score >= query.threshold {
                    if let Some(existing) = merged_results.get_mut(&entry.id) {
                        if score > existing.score {
                            existing.entry = entry;
                            existing.score = score;
                        }
                    } else {
                        merged_results.insert(entry.id, SearchResult { entry, score });
                    }
                }
            }
        }
    }

    if use_keyword_search {
        let (conditions, bind_values) =
            build_search_conditions("m.", store_id, tenant_id, &query, false);
        let sql = format!(
            "SELECT m.*, bm25(memories_fts) AS keyword_rank                  FROM memories_fts                  JOIN memories m ON m.rowid = memories_fts.rowid                  WHERE memories_fts.content MATCH ? AND {}                  ORDER BY bm25(memories_fts) LIMIT ?",
            conditions.join(" AND ")
        );
        let sanitized_query = sanitize_fts5_query(&query.query);
        let mut keyword_query = sqlx::query(&sql).bind(&sanitized_query);
        for value in &bind_values {
            keyword_query = keyword_query.bind(value);
        }
        keyword_query = keyword_query.bind(query.top_k.max(1).saturating_mul(5) as i64);

        let rows = keyword_query.fetch_all(pool).await.map_err(|e| {
            OmsError::Internal(format!(
                "failed to run keyword search against memories_fts: {e}"
            ))
        })?;

        for row in &rows {
            let entry = row_to_memory(row)?;
            let keyword_rank = row.try_get::<f64, _>("keyword_rank").unwrap_or(0.0);
            let score = 1.0 / (1.0 + keyword_rank.abs() as f32);
            if score < query.threshold {
                continue;
            }

            if let Some(existing) = merged_results.get_mut(&entry.id) {
                if score > existing.score {
                    existing.entry = entry;
                    existing.score = score;
                }
            } else {
                merged_results.insert(entry.id, SearchResult { entry, score });
            }
        }
    }

    let mut results: Vec<SearchResult> = merged_results.into_values().collect();
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results.truncate(query.top_k);
    Ok(results)
}
