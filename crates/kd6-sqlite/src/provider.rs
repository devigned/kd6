use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{Row, SqliteConnection, SqlitePool};
use uuid::Uuid;

use kd6_core::error::OmsError;
use kd6_core::models::{
    AccessControl, AccessPolicy, AuditEntry, AuditFilter, BatchCreateRequest, BatchCreateResponse,
    BatchDeleteRequest, BatchDeleteResponse, BatchError, ConflictResolution, CreateEdgeRequest,
    CreateInheritanceRequest, CreateMemoryRequest, CreateSharedSpaceRequest, CreateStoreRequest,
    GraphEdge, GraphTraversalRequest, GraphTraversalResult, InheritanceAccess, InheritanceSpec,
    JoinSpaceRequest, LeaveSpaceRequest, ListMemoriesFilter, MemoryEntry, MemoryLayer, MemoryScope,
    MemoryStore, Page, ParticipantAccess, ProviderCapabilities, SearchQuery, SearchResult,
    SharedSpace, SourceReference, SovereigntyConfig, SpaceParticipant, StoreStats,
    UpdateMemoryRequest, UpdateStoreRequest,
};
use kd6_core::OmsProvider;

fn embedding_to_bytes(embedding: &[f32]) -> Vec<u8> {
    embedding.iter().flat_map(|f| f.to_le_bytes()).collect()
}

fn bytes_to_embedding(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

fn parse_layer(s: &str) -> Result<MemoryLayer, OmsError> {
    match s {
        "working" => Ok(MemoryLayer::Working),
        "episodic" => Ok(MemoryLayer::Episodic),
        "semantic" => Ok(MemoryLayer::Semantic),
        "procedural" => Ok(MemoryLayer::Procedural),
        "archival" => Ok(MemoryLayer::Archival),
        other => Err(OmsError::Internal(format!("unknown layer: {other}"))),
    }
}

fn access_policy_to_str(policy: &AccessPolicy) -> &'static str {
    match policy {
        AccessPolicy::Private => "private",
        AccessPolicy::Inherit => "inherit",
        AccessPolicy::Shared => "shared",
        AccessPolicy::PublicRead => "public_read",
    }
}

fn parse_access_policy(s: &str) -> Result<AccessPolicy, OmsError> {
    match s {
        "private" => Ok(AccessPolicy::Private),
        "inherit" => Ok(AccessPolicy::Inherit),
        "shared" => Ok(AccessPolicy::Shared),
        "public_read" | "publicread" => Ok(AccessPolicy::PublicRead),
        other => Err(OmsError::Internal(format!(
            "unknown access policy: {other}"
        ))),
    }
}

fn parse_inheritance_access(s: &str) -> Result<InheritanceAccess, OmsError> {
    match s {
        "read_only" => Ok(InheritanceAccess::ReadOnly),
        "read_write" => Ok(InheritanceAccess::ReadWrite),
        other => Err(OmsError::Internal(format!(
            "unknown inheritance access: {other}"
        ))),
    }
}

fn inheritance_access_to_str(access: InheritanceAccess) -> &'static str {
    match access {
        InheritanceAccess::ReadOnly => "read_only",
        InheritanceAccess::ReadWrite => "read_write",
    }
}

fn parse_conflict_resolution(s: &str) -> Result<ConflictResolution, OmsError> {
    match s {
        "last_write_wins" => Ok(ConflictResolution::LastWriteWins),
        "orchestrator_merge" => Ok(ConflictResolution::OrchestratorMerge),
        "crdt" => Ok(ConflictResolution::Crdt),
        other => Err(OmsError::Internal(format!(
            "unknown conflict resolution: {other}"
        ))),
    }
}

fn conflict_resolution_to_str(conflict_resolution: ConflictResolution) -> &'static str {
    match conflict_resolution {
        ConflictResolution::LastWriteWins => "last_write_wins",
        ConflictResolution::OrchestratorMerge => "orchestrator_merge",
        ConflictResolution::Crdt => "crdt",
    }
}

fn parse_participant_access(s: &str) -> Result<ParticipantAccess, OmsError> {
    match s {
        "read_only" => Ok(ParticipantAccess::ReadOnly),
        "read_write" => Ok(ParticipantAccess::ReadWrite),
        "admin" => Ok(ParticipantAccess::Admin),
        other => Err(OmsError::Internal(format!(
            "unknown participant access: {other}"
        ))),
    }
}

fn participant_access_to_str(access: ParticipantAccess) -> &'static str {
    match access {
        ParticipantAccess::ReadOnly => "read_only",
        ParticipantAccess::ReadWrite => "read_write",
        ParticipantAccess::Admin => "admin",
    }
}

fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// Escape user input for safe use with FTS5 MATCH queries.
/// Wraps each token in double quotes to prevent FTS5 operator injection.
fn sanitize_fts5_query(input: &str) -> String {
    input
        .split_whitespace()
        .map(|token| format!("\"{}\"", token.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" ")
}

fn build_search_conditions(
    prefix: &str,
    store_id: Uuid,
    tenant_id: &str,
    query: &SearchQuery,
    include_embedding: bool,
) -> (Vec<String>, Vec<String>) {
    let mut conditions = vec![
        format!("{prefix}store_id = ?"),
        format!("{prefix}tenant_id = ?"),
    ];
    let mut bind_values = vec![store_id.to_string(), tenant_id.to_string()];

    if include_embedding {
        conditions.push(format!("{prefix}embedding IS NOT NULL"));
    }

    if !query.layers.is_empty() {
        let placeholders: Vec<&str> = query.layers.iter().map(|_| "?").collect();
        conditions.push(format!("{prefix}layer IN ({})", placeholders.join(",")));
        for layer in &query.layers {
            bind_values.push(layer.to_string());
        }
    }

    if let Some(tags) = &query.filters.tags {
        for tag in tags {
            conditions.push(format!("{prefix}tags_json LIKE ? ESCAPE '\\'"));
            bind_values.push(format!("%\"{}\"%", escape_like(tag)));
        }
    }

    if let Some(categories) = &query.filters.categories {
        for category in categories {
            conditions.push(format!("{prefix}categories_json LIKE ? ESCAPE '\\'"));
            bind_values.push(format!("%\"{}\"%", escape_like(category)));
        }
    }

    if let Some(owner_agent_id) = &query.filters.owner_agent_id {
        conditions.push(format!("{prefix}owner_agent_id = ?"));
        bind_values.push(owner_agent_id.clone());
    }

    if let Some(scope) = &query.scope {
        conditions.push(format!("{prefix}scope_tenant_id = ?"));
        bind_values.push(scope.tenant_id.clone());

        if let Some(org_id) = &scope.org_id {
            conditions.push(format!("{prefix}scope_org_id = ?"));
            bind_values.push(org_id.clone());
        }
        if let Some(team_id) = &scope.team_id {
            conditions.push(format!("{prefix}scope_team_id = ?"));
            bind_values.push(team_id.clone());
        }
        if let Some(project_id) = &scope.project_id {
            conditions.push(format!("{prefix}scope_project_id = ?"));
            bind_values.push(project_id.clone());
        }
        if let Some(user_id) = &scope.user_id {
            conditions.push(format!("{prefix}scope_user_id = ?"));
            bind_values.push(user_id.clone());
        }
        if let Some(agent_id) = &scope.agent_id {
            conditions.push(format!("{prefix}scope_agent_id = ?"));
            bind_values.push(agent_id.clone());
        }
        if let Some(session_id) = &scope.session_id {
            conditions.push(format!("{prefix}scope_session_id = ?"));
            bind_values.push(session_id.clone());
        }
        if let Some(run_id) = &scope.run_id {
            conditions.push(format!("{prefix}scope_run_id = ?"));
            bind_values.push(run_id.clone());
        }
    }

    (conditions, bind_values)
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f64;
    let mut norm_a = 0.0f64;
    let mut norm_b = 0.0f64;
    for (x, y) in a.iter().zip(b.iter()) {
        let x = *x as f64;
        let y = *y as f64;
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }
    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom == 0.0 {
        0.0
    } else {
        (dot / denom) as f32
    }
}

fn row_to_audit(row: &sqlx::sqlite::SqliteRow) -> Result<AuditEntry, OmsError> {
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
    })
}

fn row_to_inheritance(row: &sqlx::sqlite::SqliteRow) -> Result<InheritanceSpec, OmsError> {
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

fn row_to_space_participant(row: &sqlx::sqlite::SqliteRow) -> Result<SpaceParticipant, OmsError> {
    let joined_at: String = row.get("joined_at");

    Ok(SpaceParticipant {
        agent_id: row.get("agent_id"),
        access: parse_participant_access(&row.get::<String, _>("access"))?,
        joined_at: DateTime::parse_from_rfc3339(&joined_at)
            .map_err(|e| OmsError::Internal(format!("invalid participant joined_at: {e}")))?
            .with_timezone(&Utc),
    })
}

fn row_to_shared_space(
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

fn row_to_memory(row: &sqlx::sqlite::SqliteRow) -> Result<MemoryEntry, OmsError> {
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
    })
}

/// SQLite-backed OMS provider for local development, testing, and embedded use.
pub struct SqliteProvider {
    pub(crate) pool: SqlitePool,
}

impl SqliteProvider {
    pub async fn new(database_url: &str) -> Result<Self, OmsError> {
        use sqlx::sqlite::SqliteConnectOptions;
        use std::str::FromStr;

        let options = SqliteConnectOptions::from_str(database_url)
            .map_err(|e| OmsError::Internal(format!("invalid database URL: {e}")))?
            .pragma("journal_mode", "WAL")
            .pragma("foreign_keys", "ON")
            .create_if_missing(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await
            .map_err(|e| OmsError::Internal(format!("failed to connect to SQLite: {e}")))?;

        let provider = Self { pool };
        provider.run_migrations().await?;
        Ok(provider)
    }

    async fn run_migrations(&self) -> Result<(), OmsError> {
        sqlx::migrate!("./migrations")
            .run(&self.pool)
            .await
            .map_err(|e| OmsError::Internal(format!("migration failed: {e}")))?;
        Ok(())
    }

    fn row_to_store(&self, row: &sqlx::sqlite::SqliteRow) -> Result<MemoryStore, OmsError> {
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

    /// Insert an audit entry on the given connection. Caller is responsible for
    /// transaction management (BEGIN/COMMIT/ROLLBACK).
    #[allow(clippy::too_many_arguments)]
    async fn log_audit_on_conn(
        &self,
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
        .map_err(|e| OmsError::Internal(format!("failed to query prev audit hash: {e}")))?
        .flatten();

        let mut hasher = Sha256::new();
        hasher.update(id.to_string().as_bytes());
        hasher.update(store_id.to_string().as_bytes());
        hasher.update(tenant_id.as_bytes());
        hasher.update(memory_id_str.as_deref().unwrap_or("").as_bytes());
        hasher.update(action.as_bytes());
        hasher.update(agent_id.unwrap_or("").as_bytes());
        hasher.update(details_json.as_deref().unwrap_or("").as_bytes());
        hasher.update(now_str.as_bytes());
        hasher.update(prev_hash.as_deref().unwrap_or("").as_bytes());
        let entry_hash = format!("{:x}", hasher.finalize());

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
        .map_err(|e| OmsError::Internal(format!("failed to insert audit log: {e}")))?;

        Ok(())
    }

    /// Standalone audit log entry — acquires its own connection and transaction.
    #[allow(dead_code)]
    async fn log_audit(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        memory_id: Option<Uuid>,
        action: &str,
        agent_id: Option<&str>,
        details: Option<serde_json::Value>,
    ) -> Result<(), OmsError> {
        let mut conn =
            self.pool.acquire().await.map_err(|e| {
                OmsError::Internal(format!("failed to acquire audit connection: {e}"))
            })?;

        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut *conn)
            .await
            .map_err(|e| OmsError::Internal(format!("failed to begin audit transaction: {e}")))?;

        if let Err(e) = self
            .log_audit_on_conn(
                &mut conn, tenant_id, store_id, memory_id, action, agent_id, details,
            )
            .await
        {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            return Err(e);
        }

        sqlx::query("COMMIT")
            .execute(&mut *conn)
            .await
            .map_err(|e| OmsError::Internal(format!("failed to commit audit transaction: {e}")))?;

        Ok(())
    }

    async fn get_space_participants(
        &self,
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
        .fetch_all(&self.pool)
        .await
        .map_err(|e| OmsError::Internal(format!("failed to query space participants: {e}")))?;

        rows.iter().map(row_to_space_participant).collect()
    }

    async fn hydrate_shared_space(
        &self,
        row: &sqlx::sqlite::SqliteRow,
    ) -> Result<SharedSpace, OmsError> {
        let id_str: String = row.get("id");
        let store_id_str: String = row.get("store_id");
        let tenant_id: String = row.get("tenant_id");
        let space_id = Uuid::parse_str(&id_str)
            .map_err(|e| OmsError::Internal(format!("invalid shared space id: {e}")))?;
        let store_id = Uuid::parse_str(&store_id_str)
            .map_err(|e| OmsError::Internal(format!("invalid shared space store id: {e}")))?;
        let participants = self
            .get_space_participants(&tenant_id, store_id, space_id)
            .await?;
        row_to_shared_space(row, participants)
    }
}

#[async_trait]
impl OmsProvider for SqliteProvider {
    // --- Store Management ---

    async fn create_store(
        &self,
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

        sqlx::query(
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
        .execute(&self.pool)
        .await
        .map_err(|e| OmsError::Internal(format!("failed to insert store: {e}")))?;

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

    async fn get_store(&self, tenant_id: &str, store_id: Uuid) -> Result<MemoryStore, OmsError> {
        let row = sqlx::query("SELECT * FROM stores WHERE id = ? AND tenant_id = ?")
            .bind(store_id.to_string())
            .bind(tenant_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| OmsError::Internal(format!("failed to query store: {e}")))?
            .ok_or_else(|| OmsError::StoreNotFound(store_id.to_string()))?;

        self.row_to_store(&row)
    }

    async fn list_stores(&self, tenant_id: &str) -> Result<Vec<MemoryStore>, OmsError> {
        let rows = sqlx::query("SELECT * FROM stores WHERE tenant_id = ? ORDER BY created_at DESC")
            .bind(tenant_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| OmsError::Internal(format!("failed to list stores: {e}")))?;

        rows.iter().map(|row| self.row_to_store(row)).collect()
    }

    async fn update_store(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        request: UpdateStoreRequest,
    ) -> Result<MemoryStore, OmsError> {
        // Fetch existing store (also verifies tenant ownership)
        let existing = self.get_store(tenant_id, store_id).await?;

        let name = request.name.unwrap_or(existing.name);
        let config = request.config.unwrap_or(existing.config);
        let metadata = request.metadata.unwrap_or(existing.metadata);
        let now_str = Utc::now().to_rfc3339();
        let config_json = serde_json::to_string(&config)
            .map_err(|e| OmsError::Internal(format!("failed to serialize config: {e}")))?;
        let metadata_json = serde_json::to_string(&metadata)
            .map_err(|e| OmsError::Internal(format!("failed to serialize metadata: {e}")))?;

        sqlx::query(
            "UPDATE stores SET name = ?, config_json = ?, metadata_json = ?, updated_at = ?
             WHERE id = ? AND tenant_id = ?",
        )
        .bind(&name)
        .bind(&config_json)
        .bind(&metadata_json)
        .bind(&now_str)
        .bind(store_id.to_string())
        .bind(tenant_id)
        .execute(&self.pool)
        .await
        .map_err(|e| OmsError::Internal(format!("failed to update store: {e}")))?;

        self.get_store(tenant_id, store_id).await
    }

    async fn delete_store(&self, tenant_id: &str, store_id: Uuid) -> Result<(), OmsError> {
        let result = sqlx::query("DELETE FROM stores WHERE id = ? AND tenant_id = ?")
            .bind(store_id.to_string())
            .bind(tenant_id)
            .execute(&self.pool)
            .await
            .map_err(|e| OmsError::Internal(format!("failed to delete store: {e}")))?;

        if result.rows_affected() == 0 {
            return Err(OmsError::StoreNotFound(store_id.to_string()));
        }
        Ok(())
    }

    // --- Memory CRUD ---

    async fn create_memory(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        request: CreateMemoryRequest,
    ) -> Result<MemoryEntry, OmsError> {
        // Verify store exists and belongs to tenant
        self.get_store(tenant_id, store_id).await?;

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

        let mut conn = self
            .pool
            .acquire()
            .await
            .map_err(|e| OmsError::Internal(format!("failed to acquire connection: {e}")))?;

        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut *conn)
            .await
            .map_err(|e| OmsError::Internal(format!("failed to begin transaction: {e}")))?;

        if let Err(e) = sqlx::query(
            "INSERT INTO memories (
                id, store_id, tenant_id, layer, content_json, embedding,
                owner_agent_id,
                scope_tenant_id, scope_org_id, scope_team_id, scope_project_id,
                scope_user_id, scope_agent_id, scope_session_id, scope_run_id,
                tags_json, categories_json, source_json,
                access_policy, allowed_agents_json, allowed_scopes_json,
                created_at, updated_at, expires_at, immutable, version,
                valid_from, valid_until, confidence, entity_type
            ) VALUES (
                ?, ?, ?, ?, ?, ?,
                ?,
                ?, ?, ?, ?,
                ?, ?, ?, ?,
                ?, ?, ?,
                ?, ?, ?,
                ?, ?, ?, ?, 1,
                ?, ?, ?, ?
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
        .execute(&mut *conn)
        .await
        {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            return Err(OmsError::Internal(format!("failed to insert memory: {e}")));
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
        };

        if let Err(e) = self
            .log_audit_on_conn(
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
            .map_err(|e| OmsError::Internal(format!("failed to commit transaction: {e}")))?;

        Ok(entry)
    }

    async fn get_memory(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        memory_id: Uuid,
    ) -> Result<MemoryEntry, OmsError> {
        let row =
            sqlx::query("SELECT * FROM memories WHERE id = ? AND store_id = ? AND tenant_id = ?")
                .bind(memory_id.to_string())
                .bind(store_id.to_string())
                .bind(tenant_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| OmsError::Internal(format!("failed to query memory: {e}")))?
                .ok_or_else(|| OmsError::MemoryNotFound(memory_id.to_string()))?;

        row_to_memory(&row)
    }

    async fn list_memories(
        &self,
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
            .fetch_one(&self.pool)
            .await
            .map_err(|e| OmsError::Internal(format!("failed to count memories: {e}")))?;
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
            .fetch_all(&self.pool)
            .await
            .map_err(|e| OmsError::Internal(format!("failed to list memories: {e}")))?;

        let items: Result<Vec<MemoryEntry>, OmsError> = rows.iter().map(row_to_memory).collect();

        Ok(Page {
            items: items?,
            total: total as u64,
            limit,
            offset,
        })
    }

    async fn update_memory(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        memory_id: Uuid,
        request: UpdateMemoryRequest,
    ) -> Result<MemoryEntry, OmsError> {
        let existing = self.get_memory(tenant_id, store_id, memory_id).await?;

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

        let mut conn = self
            .pool
            .acquire()
            .await
            .map_err(|e| OmsError::Internal(format!("failed to acquire connection: {e}")))?;

        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut *conn)
            .await
            .map_err(|e| OmsError::Internal(format!("failed to begin transaction: {e}")))?;

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

        if let Err(e) = self
            .log_audit_on_conn(
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
            .map_err(|e| OmsError::Internal(format!("failed to commit transaction: {e}")))?;

        // Re-fetch the updated entry
        self.get_memory(tenant_id, store_id, memory_id).await
    }

    async fn delete_memory(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        memory_id: Uuid,
    ) -> Result<(), OmsError> {
        let existing = self.get_memory(tenant_id, store_id, memory_id).await?;

        let mut conn = self
            .pool
            .acquire()
            .await
            .map_err(|e| OmsError::Internal(format!("failed to acquire connection: {e}")))?;

        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut *conn)
            .await
            .map_err(|e| OmsError::Internal(format!("failed to begin transaction: {e}")))?;

        let result = match sqlx::query(
            "DELETE FROM memories WHERE id = ? AND store_id = ? AND tenant_id = ?",
        )
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

        if let Err(e) = self
            .log_audit_on_conn(
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
            .map_err(|e| OmsError::Internal(format!("failed to commit transaction: {e}")))?;

        Ok(())
    }

    // --- Search ---

    async fn search(
        &self,
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
                .fetch_all(&self.pool)
                .await
                .map_err(|e| OmsError::Internal(format!("failed to search memories: {e}")))?;

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

            let rows = keyword_query.fetch_all(&self.pool).await.map_err(|e| {
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

    // --- Level 2: Audit ---

    async fn audit_log(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        filter: AuditFilter,
    ) -> Result<Page<AuditEntry>, OmsError> {
        self.get_store(tenant_id, store_id).await?;

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
            .fetch_one(&self.pool)
            .await
            .map_err(|e| OmsError::Internal(format!("failed to count audit logs: {e}")))?;
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
            .fetch_all(&self.pool)
            .await
            .map_err(|e| OmsError::Internal(format!("failed to query audit logs: {e}")))?;

        Ok(Page {
            items: rows.iter().map(row_to_audit).collect::<Result<_, _>>()?,
            total: total as u64,
            limit,
            offset,
        })
    }

    // --- Level 2: Lifecycle ---

    async fn purge_expired(&self, tenant_id: &str, store_id: Uuid) -> Result<u64, OmsError> {
        self.get_store(tenant_id, store_id).await?;

        let now_str = Utc::now().to_rfc3339();

        let mut conn = self.pool.acquire().await.map_err(|e| {
            OmsError::Internal(format!("failed to acquire connection: {e}"))
        })?;

        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut *conn)
            .await
            .map_err(|e| OmsError::Internal(format!("failed to begin transaction: {e}")))?;

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

        if let Err(e) = self
            .log_audit_on_conn(
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
            .map_err(|e| OmsError::Internal(format!("failed to commit transaction: {e}")))?;

        Ok(deleted)
    }

    // --- Level 2: Batch ---
    // Note: Batch operations have partial-success semantics. Individual entries
    // are committed independently; if entry N fails, entries 0..N-1 remain.
    // The response reports both successes and failures. A future optimization
    // could use a single transaction for atomicity.

    async fn batch_create_memories(
        &self,
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
        self.get_store(tenant_id, store_id).await?;

        let mut created = Vec::new();
        let mut errors = Vec::new();

        for (index, entry) in request.entries.into_iter().enumerate() {
            match self.create_memory(tenant_id, store_id, entry).await {
                Ok(memory) => created.push(memory),
                Err(error) => errors.push(BatchError {
                    index,
                    error: error.to_string(),
                }),
            }
        }

        Ok(BatchCreateResponse { created, errors })
    }

    async fn batch_delete_memories(
        &self,
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
            match self.delete_memory(tenant_id, store_id, memory_id).await {
                Ok(()) => deleted += 1,
                Err(error) => errors.push(BatchError {
                    index,
                    error: error.to_string(),
                }),
            }
        }

        Ok(BatchDeleteResponse { deleted, errors })
    }

    // --- Level 2: Inheritance ---

    async fn create_inheritance(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        request: CreateInheritanceRequest,
    ) -> Result<InheritanceSpec, OmsError> {
        self.get_store(tenant_id, store_id).await?;

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

        sqlx::query(
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
        .execute(&self.pool)
        .await
        .map_err(|e| OmsError::Internal(format!("failed to create inheritance: {e}")))?;

        Ok(inheritance)
    }

    async fn delete_inheritance(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        inheritance_id: Uuid,
    ) -> Result<(), OmsError> {
        let result =
            sqlx::query("DELETE FROM inheritance WHERE id = ? AND store_id = ? AND tenant_id = ?")
                .bind(inheritance_id.to_string())
                .bind(store_id.to_string())
                .bind(tenant_id)
                .execute(&self.pool)
                .await
                .map_err(|e| OmsError::Internal(format!("failed to delete inheritance: {e}")))?;

        if result.rows_affected() == 0 {
            return Err(OmsError::InvalidInput(format!(
                "inheritance not found: {inheritance_id}"
            )));
        }

        Ok(())
    }

    async fn bubble_up(
        &self,
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
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| OmsError::Internal(format!("failed to query inheritance: {e}")))?
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

        let source_rows = memory_query.fetch_all(&self.pool).await.map_err(|e| {
            OmsError::Internal(format!("failed to query child memories for bubble up: {e}"))
        })?;

        // Check which source memories have already been bubbled up to this parent
        // by looking for existing memories with a source reference pointing back.
        let mut already_bubbled = std::collections::HashSet::new();
        {
            let existing_sql =
                "SELECT source_json FROM memories WHERE store_id = ? AND tenant_id = ? AND scope_agent_id = ? AND source_json IS NOT NULL";
            let rows = sqlx::query(existing_sql)
                .bind(store_id.to_string())
                .bind(tenant_id)
                .bind(&request.parent_agent_id)
                .fetch_all(&self.pool)
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

        let mut created = Vec::new();
        for row in &source_rows {
            let source = row_to_memory(row)?;

            // Skip if already bubbled up
            if already_bubbled.contains(&source.id) {
                continue;
            }

            let mut parent_scope = source.scope.clone();
            parent_scope.agent_id = Some(request.parent_agent_id.clone());
            parent_scope.session_id = None;
            parent_scope.run_id = None;

            let memory = self
                .create_memory(
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
            let summary_entry = self
                .create_memory(
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
                    },
                )
                .await?;
            created.push(summary_entry);
        }

        Ok(created)
    }

    // --- Level 2: Shared Spaces ---

    async fn create_shared_space(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        request: CreateSharedSpaceRequest,
    ) -> Result<SharedSpace, OmsError> {
        self.get_store(tenant_id, store_id).await?;

        let id = Uuid::new_v4();
        let now = Utc::now().to_rfc3339();
        let scope = request.scope.normalize(tenant_id);
        let scope_json = serde_json::to_string(&scope).map_err(|e| {
            OmsError::Internal(format!("failed to serialize shared space scope: {e}"))
        })?;

        sqlx::query(
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
        .execute(&self.pool)
        .await
        .map_err(|e| OmsError::Internal(format!("failed to create shared space: {e}")))?;

        self.get_shared_space(tenant_id, store_id, id).await
    }

    async fn list_shared_spaces(
        &self,
        tenant_id: &str,
        store_id: Uuid,
    ) -> Result<Vec<SharedSpace>, OmsError> {
        self.get_store(tenant_id, store_id).await?;

        let rows = sqlx::query(
            "SELECT * FROM shared_spaces WHERE store_id = ? AND tenant_id = ? ORDER BY created_at DESC",
        )
        .bind(store_id.to_string())
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| OmsError::Internal(format!("failed to list shared spaces: {e}")))?;

        let mut spaces = Vec::with_capacity(rows.len());
        for row in &rows {
            spaces.push(self.hydrate_shared_space(row).await?);
        }
        Ok(spaces)
    }

    async fn get_shared_space(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        space_id: Uuid,
    ) -> Result<SharedSpace, OmsError> {
        let row = sqlx::query(
            "SELECT * FROM shared_spaces WHERE id = ? AND store_id = ? AND tenant_id = ?",
        )
        .bind(space_id.to_string())
        .bind(store_id.to_string())
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| OmsError::Internal(format!("failed to query shared space: {e}")))?
        .ok_or_else(|| OmsError::InvalidInput(format!("shared space not found: {space_id}")))?;

        self.hydrate_shared_space(&row).await
    }

    async fn join_shared_space(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        space_id: Uuid,
        request: JoinSpaceRequest,
    ) -> Result<SharedSpace, OmsError> {
        self.get_shared_space(tenant_id, store_id, space_id).await?;

        sqlx::query(
            "INSERT INTO space_participants (space_id, agent_id, access, joined_at)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(space_id, agent_id) DO UPDATE SET access = excluded.access, joined_at = excluded.joined_at",
        )
        .bind(space_id.to_string())
        .bind(&request.agent_id)
        .bind(participant_access_to_str(request.access))
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| OmsError::Internal(format!("failed to join shared space: {e}")))?;

        self.get_shared_space(tenant_id, store_id, space_id).await
    }

    async fn leave_shared_space(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        space_id: Uuid,
        request: LeaveSpaceRequest,
    ) -> Result<(), OmsError> {
        self.get_shared_space(tenant_id, store_id, space_id).await?;

        sqlx::query("DELETE FROM space_participants WHERE space_id = ? AND agent_id = ?")
            .bind(space_id.to_string())
            .bind(&request.agent_id)
            .execute(&self.pool)
            .await
            .map_err(|e| OmsError::Internal(format!("failed to leave shared space: {e}")))?;

        Ok(())
    }

    async fn delete_shared_space(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        space_id: Uuid,
    ) -> Result<(), OmsError> {
        let result = sqlx::query(
            "DELETE FROM shared_spaces WHERE id = ? AND store_id = ? AND tenant_id = ?",
        )
        .bind(space_id.to_string())
        .bind(store_id.to_string())
        .bind(tenant_id)
        .execute(&self.pool)
        .await
        .map_err(|e| OmsError::Internal(format!("failed to delete shared space: {e}")))?;

        if result.rows_affected() == 0 {
            return Err(OmsError::InvalidInput(format!(
                "shared space not found: {space_id}"
            )));
        }

        Ok(())
    }

    async fn create_edge(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        request: CreateEdgeRequest,
    ) -> Result<GraphEdge, OmsError> {
        self.get_store(tenant_id, store_id).await?;
        self.get_memory(tenant_id, store_id, request.source_memory_id)
            .await?;
        self.get_memory(tenant_id, store_id, request.target_memory_id)
            .await?;

        let id = Uuid::new_v4();
        let now = Utc::now();
        let metadata_json = serde_json::to_string(&request.metadata)
            .map_err(|e| OmsError::Internal(format!("failed to serialize edge metadata: {e}")))?;

        sqlx::query(
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
        .execute(&self.pool)
        .await
        .map_err(|e| OmsError::Internal(format!("failed to insert graph edge: {e}")))?;

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

    async fn delete_edge(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        edge_id: Uuid,
    ) -> Result<(), OmsError> {
        let result =
            sqlx::query("DELETE FROM graph_edges WHERE id = ? AND store_id = ? AND tenant_id = ?")
                .bind(edge_id.to_string())
                .bind(store_id.to_string())
                .bind(tenant_id)
                .execute(&self.pool)
                .await
                .map_err(|e| OmsError::Internal(format!("failed to delete graph edge: {e}")))?;

        if result.rows_affected() == 0 {
            return Err(OmsError::InvalidInput(format!(
                "graph edge not found: {edge_id}"
            )));
        }
        Ok(())
    }

    async fn graph_traverse(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        request: GraphTraversalRequest,
    ) -> Result<GraphTraversalResult, OmsError> {
        use std::collections::{HashSet, VecDeque};

        const MAX_TRAVERSAL_DEPTH: u32 = 10;
        let depth = request.depth.min(MAX_TRAVERSAL_DEPTH);

        let start = self
            .get_memory(tenant_id, store_id, request.start_memory_id)
            .await?;

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
                .fetch_all(&self.pool)
                .await
                .map_err(|e| OmsError::Internal(format!("failed to query graph edges: {e}")))?;

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
                        metadata: serde_json::from_str(&metadata_str).map_err(|e| {
                            OmsError::Internal(format!("invalid edge metadata: {e}"))
                        })?,
                        created_at: DateTime::parse_from_rfc3339(&created_str)
                            .map_err(|e| {
                                OmsError::Internal(format!("invalid edge created_at: {e}"))
                            })?
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

                let node_rows = batch_q.fetch_all(&self.pool).await.map_err(|e| {
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

    async fn gdpr_purge(
        &self,
        tenant_id: &str,
        store_id: Uuid,
        scope: MemoryScope,
    ) -> Result<u64, OmsError> {
        self.get_store(tenant_id, store_id).await?;

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

        let mut conn = self
            .pool
            .acquire()
            .await
            .map_err(|e| OmsError::Internal(format!("failed to acquire connection: {e}")))?;

        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut *conn)
            .await
            .map_err(|e| OmsError::Internal(format!("failed to begin transaction: {e}")))?;

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
        if !purged_ids.is_empty() {
            for chunk in purged_ids.chunks(500) {
                let placeholders: Vec<&str> = chunk.iter().map(|_| "?").collect();
                let anon_sql = format!(
                    "UPDATE audit_log SET agent_id = NULL, details_json = NULL \
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
        if let Err(e) = self
            .log_audit_on_conn(
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
            .map_err(|e| OmsError::Internal(format!("failed to commit transaction: {e}")))?;

        Ok(deleted)
    }

    // --- Health & Capabilities ---

    async fn stats(&self, tenant_id: &str, store_id: Uuid) -> Result<StoreStats, OmsError> {
        // Verify store exists and belongs to tenant
        self.get_store(tenant_id, store_id).await?;

        let row = sqlx::query(
            "SELECT COUNT(*) as cnt FROM memories WHERE store_id = ? AND tenant_id = ?",
        )
        .bind(store_id.to_string())
        .bind(tenant_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| OmsError::Internal(format!("failed to get stats: {e}")))?;

        let total: i64 = row.get("cnt");

        let layer_rows = sqlx::query(
            "SELECT layer, COUNT(*) as cnt FROM memories WHERE store_id = ? AND tenant_id = ? GROUP BY layer",
        )
        .bind(store_id.to_string())
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| OmsError::Internal(format!("failed to get layer stats: {e}")))?;

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

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supported_layers: vec![
                MemoryLayer::Working,
                MemoryLayer::Episodic,
                MemoryLayer::Semantic,
                MemoryLayer::Procedural,
                MemoryLayer::Archival,
            ],
            vector_search: true,
            graph_support: true,
            temporal_queries: true,
            keyword_search: true,
            max_embedding_dimensions: None,
            supported_distance_metrics: vec!["cosine".into()],
            compaction_support: false,
            archival_support: false,
            max_entry_size_bytes: None,
            batch_operations: true,
            max_batch_size: Some(1000),
            pub_sub_notifications: false,
            encryption_at_rest: false,
            audit_log: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use kd6_core::models::{CreateStoreRequest, StoreConfig, UpdateStoreRequest};

    async fn test_provider() -> SqliteProvider {
        SqliteProvider::new("sqlite::memory:").await.unwrap()
    }

    #[tokio::test]
    async fn create_and_get_store() {
        let provider = test_provider().await;
        let store = provider
            .create_store(
                "tenant-1",
                CreateStoreRequest {
                    name: "my-store".into(),
                    region: Some("us-east".into()),
                    config: StoreConfig::default(),
                    metadata: Default::default(),
                },
            )
            .await
            .unwrap();

        assert_eq!(store.name, "my-store");
        assert_eq!(store.tenant_id, "tenant-1");
        assert_eq!(store.region.as_deref(), Some("us-east"));

        let fetched = provider.get_store("tenant-1", store.id).await.unwrap();
        assert_eq!(fetched.id, store.id);
        assert_eq!(fetched.name, "my-store");
    }

    #[tokio::test]
    async fn get_store_wrong_tenant_returns_not_found() {
        let provider = test_provider().await;
        let store = provider
            .create_store(
                "tenant-1",
                CreateStoreRequest {
                    name: "secret-store".into(),
                    region: None,
                    config: StoreConfig::default(),
                    metadata: Default::default(),
                },
            )
            .await
            .unwrap();

        let result = provider.get_store("tenant-2", store.id).await;
        assert!(matches!(result, Err(OmsError::StoreNotFound(_))));
    }

    #[tokio::test]
    async fn list_stores_filters_by_tenant() {
        let provider = test_provider().await;
        for i in 0..3 {
            provider
                .create_store(
                    "tenant-a",
                    CreateStoreRequest {
                        name: format!("store-a-{i}"),
                        region: None,
                        config: StoreConfig::default(),
                        metadata: Default::default(),
                    },
                )
                .await
                .unwrap();
        }
        provider
            .create_store(
                "tenant-b",
                CreateStoreRequest {
                    name: "store-b".into(),
                    region: None,
                    config: StoreConfig::default(),
                    metadata: Default::default(),
                },
            )
            .await
            .unwrap();

        let a_stores = provider.list_stores("tenant-a").await.unwrap();
        assert_eq!(a_stores.len(), 3);

        let b_stores = provider.list_stores("tenant-b").await.unwrap();
        assert_eq!(b_stores.len(), 1);

        let empty = provider.list_stores("tenant-c").await.unwrap();
        assert!(empty.is_empty());
    }

    #[tokio::test]
    async fn update_store_partial() {
        let provider = test_provider().await;
        let store = provider
            .create_store(
                "tenant-1",
                CreateStoreRequest {
                    name: "original".into(),
                    region: None,
                    config: StoreConfig::default(),
                    metadata: Default::default(),
                },
            )
            .await
            .unwrap();

        let updated = provider
            .update_store(
                "tenant-1",
                store.id,
                UpdateStoreRequest {
                    name: Some("renamed".into()),
                    config: None,
                    metadata: None,
                },
            )
            .await
            .unwrap();

        assert_eq!(updated.name, "renamed");
        assert!(updated.updated_at > store.updated_at);
    }

    #[tokio::test]
    async fn update_store_wrong_tenant() {
        let provider = test_provider().await;
        let store = provider
            .create_store(
                "tenant-1",
                CreateStoreRequest {
                    name: "mine".into(),
                    region: None,
                    config: StoreConfig::default(),
                    metadata: Default::default(),
                },
            )
            .await
            .unwrap();

        let result = provider
            .update_store(
                "tenant-2",
                store.id,
                UpdateStoreRequest {
                    name: Some("hijacked".into()),
                    config: None,
                    metadata: None,
                },
            )
            .await;
        assert!(matches!(result, Err(OmsError::StoreNotFound(_))));
    }

    #[tokio::test]
    async fn delete_store_success() {
        let provider = test_provider().await;
        let store = provider
            .create_store(
                "tenant-1",
                CreateStoreRequest {
                    name: "doomed".into(),
                    region: None,
                    config: StoreConfig::default(),
                    metadata: Default::default(),
                },
            )
            .await
            .unwrap();

        provider.delete_store("tenant-1", store.id).await.unwrap();

        let result = provider.get_store("tenant-1", store.id).await;
        assert!(matches!(result, Err(OmsError::StoreNotFound(_))));
    }

    #[tokio::test]
    async fn delete_store_wrong_tenant() {
        let provider = test_provider().await;
        let store = provider
            .create_store(
                "tenant-1",
                CreateStoreRequest {
                    name: "protected".into(),
                    region: None,
                    config: StoreConfig::default(),
                    metadata: Default::default(),
                },
            )
            .await
            .unwrap();

        let result = provider.delete_store("tenant-2", store.id).await;
        assert!(matches!(result, Err(OmsError::StoreNotFound(_))));

        // Verify it still exists for the correct tenant
        provider.get_store("tenant-1", store.id).await.unwrap();
    }

    #[tokio::test]
    async fn delete_nonexistent_store() {
        let provider = test_provider().await;
        let result = provider.delete_store("tenant-1", Uuid::new_v4()).await;
        assert!(matches!(result, Err(OmsError::StoreNotFound(_))));
    }

    #[tokio::test]
    async fn store_config_roundtrip() {
        let provider = test_provider().await;
        let config = StoreConfig {
            default_ttl_seconds: Some(3600),
            default_sharing_policy: Some("private".into()),
            embedding_model: Some("text-embedding-3-small".into()),
        };

        let store = provider
            .create_store(
                "tenant-1",
                CreateStoreRequest {
                    name: "configured".into(),
                    region: None,
                    config: config.clone(),
                    metadata: Default::default(),
                },
            )
            .await
            .unwrap();

        let fetched = provider.get_store("tenant-1", store.id).await.unwrap();
        assert_eq!(fetched.config.default_ttl_seconds, Some(3600));
        assert_eq!(
            fetched.config.embedding_model.as_deref(),
            Some("text-embedding-3-small")
        );
    }

    // --- Helper to create a store + provider for memory tests ---

    async fn setup_with_store() -> (SqliteProvider, kd6_core::models::MemoryStore) {
        let provider = test_provider().await;
        let store = provider
            .create_store(
                "tenant-1",
                CreateStoreRequest {
                    name: "test-store".into(),
                    region: None,
                    config: StoreConfig::default(),
                    metadata: Default::default(),
                },
            )
            .await
            .unwrap();
        (provider, store)
    }

    fn make_memory_request(agent: &str) -> CreateMemoryRequest {
        CreateMemoryRequest {
            layer: kd6_core::models::MemoryLayer::Working,
            content: serde_json::json!({"text": "hello world"}),
            embedding: Some(vec![1.0, 0.0, 0.0]),
            owner_agent_id: agent.into(),
            scope: kd6_core::models::MemoryScope {
                tenant_id: "tenant-1".into(),
                org_id: None,
                team_id: None,
                project_id: None,
                user_id: None,
                agent_id: Some(agent.into()),
                session_id: None,
                run_id: None,
            },
            tags: vec!["test".into()],
            categories: vec!["unit-test".into()],
            source: None,
            access_control: Default::default(),
            expires_at: None,
            immutable: false,
            valid_from: None,
            valid_until: None,
            confidence: None,
            entity_type: None,
        }
    }

    #[tokio::test]
    async fn create_and_get_memory() {
        let (provider, store) = setup_with_store().await;
        let entry = provider
            .create_memory("tenant-1", store.id, make_memory_request("agent-1"))
            .await
            .unwrap();

        assert_eq!(entry.version, 1);
        assert_eq!(entry.owner_agent_id, "agent-1");
        assert_eq!(entry.tags, vec!["test"]);
        assert_eq!(entry.embedding, Some(vec![1.0, 0.0, 0.0]));

        let fetched = provider
            .get_memory("tenant-1", store.id, entry.id)
            .await
            .unwrap();
        assert_eq!(fetched.id, entry.id);
        assert_eq!(fetched.content, serde_json::json!({"text": "hello world"}));
        assert_eq!(fetched.embedding, Some(vec![1.0, 0.0, 0.0]));
    }

    #[tokio::test]
    async fn get_memory_wrong_tenant() {
        let (provider, store) = setup_with_store().await;
        let entry = provider
            .create_memory("tenant-1", store.id, make_memory_request("agent-1"))
            .await
            .unwrap();

        let result = provider.get_memory("tenant-2", store.id, entry.id).await;
        assert!(matches!(result, Err(OmsError::MemoryNotFound(_))));
    }

    #[tokio::test]
    async fn update_memory_increments_version() {
        let (provider, store) = setup_with_store().await;
        let entry = provider
            .create_memory("tenant-1", store.id, make_memory_request("agent-1"))
            .await
            .unwrap();

        let updated = provider
            .update_memory(
                "tenant-1",
                store.id,
                entry.id,
                kd6_core::models::UpdateMemoryRequest {
                    content: Some(serde_json::json!({"text": "updated"})),
                    embedding: None,
                    tags: None,
                    categories: None,
                    access_control: None,
                    expires_at: None,
                },
            )
            .await
            .unwrap();

        assert_eq!(updated.version, 2);
        assert_eq!(updated.content, serde_json::json!({"text": "updated"}));
        assert!(updated.updated_at > entry.updated_at);
    }

    #[tokio::test]
    async fn test_optimistic_concurrency_conflict() {
        let provider = test_provider().await;
        let tenant = "t-occ";
        let store = provider
            .create_store(
                tenant,
                CreateStoreRequest {
                    name: "occ-store".into(),
                    region: None,
                    config: StoreConfig::default(),
                    metadata: Default::default(),
                },
            )
            .await
            .unwrap();

        let mut request = make_memory_request("agent-1");
        request.scope.tenant_id = tenant.into();
        request.content = serde_json::json!({"text": "original"});
        let memory = provider
            .create_memory(tenant, store.id, request)
            .await
            .unwrap();
        assert_eq!(memory.version, 1);

        let updated = provider
            .update_memory(
                tenant,
                store.id,
                memory.id,
                UpdateMemoryRequest {
                    content: Some(serde_json::json!({"text": "updated"})),
                    embedding: None,
                    tags: None,
                    categories: None,
                    access_control: None,
                    expires_at: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(updated.version, 2);

        let stale_content =
            serde_json::to_string(&serde_json::json!({"text": "stale write"})).unwrap();
        let stale_attempt = sqlx::query(
            "UPDATE memories SET content_json = ?, updated_at = ?, version = ?
             WHERE id = ? AND store_id = ? AND tenant_id = ? AND version = ?",
        )
        .bind(&stale_content)
        .bind(Utc::now().to_rfc3339())
        .bind(2)
        .bind(memory.id.to_string())
        .bind(store.id.to_string())
        .bind(tenant)
        .bind(1)
        .execute(&provider.pool)
        .await
        .unwrap();
        assert_eq!(stale_attempt.rows_affected(), 0);

        let updated2 = provider
            .update_memory(
                tenant,
                store.id,
                memory.id,
                UpdateMemoryRequest {
                    content: Some(serde_json::json!({"text": "updated again"})),
                    embedding: None,
                    tags: None,
                    categories: None,
                    access_control: None,
                    expires_at: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(updated2.version, 3);
    }

    #[tokio::test]
    async fn update_immutable_memory_fails() {
        let (provider, store) = setup_with_store().await;
        let mut req = make_memory_request("agent-1");
        req.immutable = true;

        let entry = provider
            .create_memory("tenant-1", store.id, req)
            .await
            .unwrap();

        let result = provider
            .update_memory(
                "tenant-1",
                store.id,
                entry.id,
                kd6_core::models::UpdateMemoryRequest {
                    content: Some(serde_json::json!("nope")),
                    embedding: None,
                    tags: None,
                    categories: None,
                    access_control: None,
                    expires_at: None,
                },
            )
            .await;

        assert!(matches!(result, Err(OmsError::Immutable(_))));
    }

    #[tokio::test]
    async fn delete_memory_success() {
        let (provider, store) = setup_with_store().await;
        let entry = provider
            .create_memory("tenant-1", store.id, make_memory_request("agent-1"))
            .await
            .unwrap();

        provider
            .delete_memory("tenant-1", store.id, entry.id)
            .await
            .unwrap();

        let result = provider.get_memory("tenant-1", store.id, entry.id).await;
        assert!(matches!(result, Err(OmsError::MemoryNotFound(_))));
    }

    #[tokio::test]
    async fn list_memories_pagination() {
        let (provider, store) = setup_with_store().await;

        for i in 0..5 {
            let mut req = make_memory_request("agent-1");
            req.content = serde_json::json!({"index": i});
            provider
                .create_memory("tenant-1", store.id, req)
                .await
                .unwrap();
        }

        let page1 = provider
            .list_memories(
                "tenant-1",
                store.id,
                ListMemoriesFilter {
                    limit: Some(2),
                    offset: Some(0),
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        assert_eq!(page1.items.len(), 2);
        assert_eq!(page1.total, 5);
        assert_eq!(page1.limit, 2);
        assert_eq!(page1.offset, 0);

        let page2 = provider
            .list_memories(
                "tenant-1",
                store.id,
                ListMemoriesFilter {
                    limit: Some(2),
                    offset: Some(2),
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        assert_eq!(page2.items.len(), 2);
        assert_eq!(page2.offset, 2);
    }

    #[tokio::test]
    async fn list_memories_filter_by_owner() {
        let (provider, store) = setup_with_store().await;
        provider
            .create_memory("tenant-1", store.id, make_memory_request("agent-a"))
            .await
            .unwrap();
        provider
            .create_memory("tenant-1", store.id, make_memory_request("agent-b"))
            .await
            .unwrap();
        provider
            .create_memory("tenant-1", store.id, make_memory_request("agent-a"))
            .await
            .unwrap();

        let page = provider
            .list_memories(
                "tenant-1",
                store.id,
                ListMemoriesFilter {
                    owner_agent_id: Some("agent-a".into()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        assert_eq!(page.total, 2);
        assert!(page.items.iter().all(|m| m.owner_agent_id == "agent-a"));
    }

    #[tokio::test]
    async fn embedding_roundtrip() {
        let embedding = vec![0.1, 0.2, 0.3, -0.5, 1.0];
        let bytes = super::embedding_to_bytes(&embedding);
        let recovered = super::bytes_to_embedding(&bytes);
        assert_eq!(embedding, recovered);
    }

    #[tokio::test]
    async fn search_by_vector_similarity() {
        let (provider, store) = setup_with_store().await;

        // Insert memories with different embeddings
        let mut req1 = make_memory_request("agent-1");
        req1.embedding = Some(vec![1.0, 0.0, 0.0]); // points along x-axis
        req1.content = serde_json::json!("x-axis");
        provider
            .create_memory("tenant-1", store.id, req1)
            .await
            .unwrap();

        let mut req2 = make_memory_request("agent-1");
        req2.embedding = Some(vec![0.0, 1.0, 0.0]); // points along y-axis
        req2.content = serde_json::json!("y-axis");
        provider
            .create_memory("tenant-1", store.id, req2)
            .await
            .unwrap();

        let mut req3 = make_memory_request("agent-1");
        req3.embedding = Some(vec![0.9, 0.1, 0.0]); // close to x-axis
        req3.content = serde_json::json!("near-x");
        provider
            .create_memory("tenant-1", store.id, req3)
            .await
            .unwrap();

        // Search for vectors similar to x-axis
        let results = provider
            .search(
                "tenant-1",
                store.id,
                SearchQuery {
                    query: "test".into(),
                    embedding: Some(vec![1.0, 0.0, 0.0]),
                    layers: vec![],
                    scope: None,
                    top_k: 2,
                    threshold: 0.5,
                    filters: Default::default(),
                    keyword: false,
                },
            )
            .await
            .unwrap();

        assert_eq!(results.len(), 2);
        // First result should be exact match (score ~1.0)
        assert!(results[0].score > 0.99);
        assert_eq!(results[0].entry.content, serde_json::json!("x-axis"));
        // Second should be near-x (high similarity)
        assert!(results[1].score > 0.9);
        assert_eq!(results[1].entry.content, serde_json::json!("near-x"));
    }

    #[tokio::test]
    async fn search_with_threshold_filters() {
        let (provider, store) = setup_with_store().await;

        let mut req1 = make_memory_request("agent-1");
        req1.embedding = Some(vec![1.0, 0.0, 0.0]);
        provider
            .create_memory("tenant-1", store.id, req1)
            .await
            .unwrap();

        let mut req2 = make_memory_request("agent-1");
        req2.embedding = Some(vec![0.0, 1.0, 0.0]); // orthogonal, score ~0.0
        provider
            .create_memory("tenant-1", store.id, req2)
            .await
            .unwrap();

        // High threshold should exclude orthogonal vector
        let results = provider
            .search(
                "tenant-1",
                store.id,
                SearchQuery {
                    query: "test".into(),
                    embedding: Some(vec![1.0, 0.0, 0.0]),
                    layers: vec![],
                    scope: None,
                    top_k: 10,
                    threshold: 0.5,
                    filters: Default::default(),
                    keyword: false,
                },
            )
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn search_requires_embedding() {
        let (provider, store) = setup_with_store().await;

        let result = provider
            .search(
                "tenant-1",
                store.id,
                SearchQuery {
                    query: "test".into(),
                    embedding: None,
                    layers: vec![],
                    scope: None,
                    top_k: 10,
                    threshold: 0.0,
                    filters: Default::default(),
                    keyword: false,
                },
            )
            .await;

        assert!(matches!(result, Err(OmsError::InvalidInput(_))));
    }

    #[tokio::test]
    async fn audit_log_returns_memory_mutation_entries() {
        let (provider, store) = setup_with_store().await;
        let memory = provider
            .create_memory("tenant-1", store.id, make_memory_request("agent-a"))
            .await
            .unwrap();
        provider
            .update_memory(
                "tenant-1",
                store.id,
                memory.id,
                UpdateMemoryRequest {
                    content: Some(serde_json::json!({"text": "updated"})),
                    embedding: None,
                    tags: None,
                    categories: None,
                    access_control: None,
                    expires_at: None,
                },
            )
            .await
            .unwrap();
        provider
            .delete_memory("tenant-1", store.id, memory.id)
            .await
            .unwrap();

        let page = provider
            .audit_log(
                "tenant-1",
                store.id,
                kd6_core::models::AuditFilter {
                    memory_id: Some(memory.id),
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        assert_eq!(page.total, 3);
        let actions: Vec<String> = page
            .items
            .iter()
            .map(|entry| entry.action.clone())
            .collect();
        assert!(actions.contains(&"create".to_string()));
        assert!(actions.contains(&"update".to_string()));
        assert!(actions.contains(&"delete".to_string()));
    }

    #[tokio::test]
    async fn test_audit_hash_chain_integrity() {
        let provider = test_provider().await;
        let tenant = "t-audit-hash";
        let store = provider
            .create_store(
                tenant,
                CreateStoreRequest {
                    name: "audit-hash-store".into(),
                    region: None,
                    config: StoreConfig::default(),
                    metadata: Default::default(),
                },
            )
            .await
            .unwrap();

        let mut req1 = make_memory_request("agent-audit-1");
        req1.scope.tenant_id = tenant.into();
        let m1 = provider
            .create_memory(tenant, store.id, req1)
            .await
            .unwrap();

        let mut req2 = make_memory_request("agent-audit-2");
        req2.scope.tenant_id = tenant.into();
        provider
            .create_memory(tenant, store.id, req2)
            .await
            .unwrap();

        provider
            .delete_memory(tenant, store.id, m1.id)
            .await
            .unwrap();

        let audit = provider
            .audit_log(tenant, store.id, kd6_core::models::AuditFilter::default())
            .await
            .unwrap();

        assert!(
            audit.items.len() >= 3,
            "expected at least 3 audit entries, got {}",
            audit.items.len()
        );

        let rows = sqlx::query(
            "SELECT entry_hash, prev_hash FROM audit_log WHERE store_id = ? AND tenant_id = ? ORDER BY rowid ASC",
        )
        .bind(store.id.to_string())
        .bind(tenant)
        .fetch_all(&provider.pool)
        .await
        .unwrap();

        assert!(rows.len() >= 3);

        let first_prev: Option<String> = rows[0].get("prev_hash");
        assert!(
            first_prev.is_none(),
            "first audit entry should have no prev_hash"
        );

        let first_hash: Option<String> = rows[0].get("entry_hash");
        assert!(
            first_hash.is_some(),
            "first audit entry should have entry_hash"
        );

        for i in 1..rows.len() {
            let prev_hash: Option<String> = rows[i].get("prev_hash");
            let prev_entry_hash: Option<String> = rows[i - 1].get("entry_hash");
            assert_eq!(prev_hash, prev_entry_hash, "hash chain broken at entry {i}");
        }
    }

    #[tokio::test]
    async fn purge_expired_removes_only_expired_entries() {
        let (provider, store) = setup_with_store().await;

        let mut expired = make_memory_request("agent-a");
        expired.expires_at = Some(Utc::now() - Duration::minutes(5));
        provider
            .create_memory("tenant-1", store.id, expired)
            .await
            .unwrap();

        let mut active = make_memory_request("agent-a");
        active.expires_at = Some(Utc::now() + Duration::minutes(5));
        provider
            .create_memory("tenant-1", store.id, active)
            .await
            .unwrap();

        let purged = provider.purge_expired("tenant-1", store.id).await.unwrap();
        assert_eq!(purged, 1);

        let page = provider
            .list_memories("tenant-1", store.id, ListMemoriesFilter::default())
            .await
            .unwrap();
        assert_eq!(page.total, 1);
    }

    #[tokio::test]
    async fn batch_create_memories_handles_partial_failures() {
        let (provider, store) = setup_with_store().await;
        sqlx::query(
            "CREATE TRIGGER block_bad_batch_insert BEFORE INSERT ON memories
             WHEN new.owner_agent_id = 'bad-agent' BEGIN
                 SELECT RAISE(ABORT, 'blocked owner');
             END;",
        )
        .execute(&provider.pool)
        .await
        .unwrap();

        let response = provider
            .batch_create_memories(
                "tenant-1",
                store.id,
                kd6_core::models::BatchCreateRequest {
                    entries: vec![
                        make_memory_request("good-agent"),
                        make_memory_request("bad-agent"),
                        make_memory_request("good-agent-2"),
                    ],
                },
            )
            .await
            .unwrap();

        assert_eq!(response.created.len(), 2);
        assert_eq!(response.errors.len(), 1);
        assert_eq!(response.errors[0].index, 1);
    }

    #[tokio::test]
    async fn inheritance_crud_roundtrip() {
        let (provider, store) = setup_with_store().await;
        let inheritance = provider
            .create_inheritance(
                "tenant-1",
                store.id,
                kd6_core::models::CreateInheritanceRequest {
                    parent_agent_id: "parent".into(),
                    child_agent_id: "child".into(),
                    inherit_layers: vec![MemoryLayer::Semantic],
                    filter: Default::default(),
                    access: kd6_core::models::InheritanceAccess::ReadOnly,
                    bubble_up: kd6_core::models::BubbleUpConfig {
                        enabled: true,
                        auto_summarize: false,
                        layers: vec![MemoryLayer::Semantic],
                    },
                },
            )
            .await
            .unwrap();

        let row = sqlx::query(
            "SELECT COUNT(*) AS cnt FROM inheritance WHERE id = ? AND store_id = ? AND tenant_id = ?",
        )
        .bind(inheritance.id.to_string())
        .bind(store.id.to_string())
        .bind("tenant-1")
        .fetch_one(&provider.pool)
        .await
        .unwrap();
        assert_eq!(row.get::<i64, _>("cnt"), 1);

        provider
            .delete_inheritance("tenant-1", store.id, inheritance.id)
            .await
            .unwrap();

        let row = sqlx::query(
            "SELECT COUNT(*) AS cnt FROM inheritance WHERE id = ? AND store_id = ? AND tenant_id = ?",
        )
        .bind(inheritance.id.to_string())
        .bind(store.id.to_string())
        .bind("tenant-1")
        .fetch_one(&provider.pool)
        .await
        .unwrap();
        assert_eq!(row.get::<i64, _>("cnt"), 0);
    }

    #[tokio::test]
    async fn shared_space_lifecycle() {
        let (provider, store) = setup_with_store().await;
        let space = provider
            .create_shared_space(
                "tenant-1",
                store.id,
                kd6_core::models::CreateSharedSpaceRequest {
                    name: "blackboard".into(),
                    scope: MemoryScope {
                        tenant_id: "tenant-1".into(),
                        org_id: None,
                        team_id: None,
                        project_id: None,
                        user_id: None,
                        agent_id: None,
                        session_id: None,
                        run_id: None,
                    },
                    layer: MemoryLayer::Working,
                    conflict_resolution: kd6_core::models::ConflictResolution::LastWriteWins,
                    notify_on_write: true,
                    notify_on_delete: false,
                },
            )
            .await
            .unwrap();
        assert!(space.participants.is_empty());

        let joined = provider
            .join_shared_space(
                "tenant-1",
                store.id,
                space.id,
                kd6_core::models::JoinSpaceRequest {
                    agent_id: "agent-1".into(),
                    access: kd6_core::models::ParticipantAccess::ReadWrite,
                },
            )
            .await
            .unwrap();
        assert_eq!(joined.participants.len(), 1);
        assert_eq!(joined.participants[0].agent_id, "agent-1");

        let listed = provider
            .list_shared_spaces("tenant-1", store.id)
            .await
            .unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].participants.len(), 1);

        provider
            .leave_shared_space(
                "tenant-1",
                store.id,
                space.id,
                kd6_core::models::LeaveSpaceRequest {
                    agent_id: "agent-1".into(),
                },
            )
            .await
            .unwrap();
        let fetched = provider
            .get_shared_space("tenant-1", store.id, space.id)
            .await
            .unwrap();
        assert!(fetched.participants.is_empty());
    }

    #[tokio::test]
    async fn search_supports_keyword_fts() {
        let (provider, store) = setup_with_store().await;

        let mut target = make_memory_request("agent-1");
        target.embedding = None;
        target.content = serde_json::json!({"text": "alpha keyword match"});
        provider
            .create_memory("tenant-1", store.id, target)
            .await
            .unwrap();

        let mut other = make_memory_request("agent-2");
        other.embedding = None;
        other.content = serde_json::json!({"text": "completely different"});
        provider
            .create_memory("tenant-1", store.id, other)
            .await
            .unwrap();

        let results = provider
            .search(
                "tenant-1",
                store.id,
                SearchQuery {
                    query: "alpha".into(),
                    embedding: None,
                    layers: vec![],
                    scope: None,
                    top_k: 10,
                    threshold: 0.0,
                    filters: Default::default(),
                    keyword: true,
                },
            )
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].entry.content,
            serde_json::json!({"text": "alpha keyword match"})
        );
    }

    #[tokio::test]
    async fn stats_returns_counts() {
        let (provider, store) = setup_with_store().await;

        for _ in 0..3 {
            provider
                .create_memory("tenant-1", store.id, make_memory_request("agent-1"))
                .await
                .unwrap();
        }

        let stats = provider.stats("tenant-1", store.id).await.unwrap();
        assert_eq!(stats.total_entries, 3);
        assert_eq!(stats.entries_by_layer.get(&MemoryLayer::Working), Some(&3));
    }

    #[test]
    fn cosine_similarity_unit() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((super::cosine_similarity(&a, &b) - 1.0).abs() < 1e-6);

        let c = vec![0.0, 1.0, 0.0];
        assert!(super::cosine_similarity(&a, &c).abs() < 1e-6);

        let d = vec![-1.0, 0.0, 0.0];
        assert!((super::cosine_similarity(&a, &d) + 1.0).abs() < 1e-6);
    }

    #[tokio::test]
    async fn test_graph_create_and_traverse() {
        let provider = test_provider().await;
        let tenant = "t-graph";
        let store = provider
            .create_store(
                tenant,
                CreateStoreRequest {
                    name: "graph-store".into(),
                    region: None,
                    config: StoreConfig::default(),
                    metadata: Default::default(),
                },
            )
            .await
            .unwrap();

        let mut m1_req = make_memory_request("agent-1");
        m1_req.scope.tenant_id = tenant.into();
        m1_req.content = serde_json::json!({"text": "Node A"});
        let m1 = provider
            .create_memory(tenant, store.id, m1_req)
            .await
            .unwrap();

        let mut m2_req = make_memory_request("agent-1");
        m2_req.scope.tenant_id = tenant.into();
        m2_req.content = serde_json::json!({"text": "Node B"});
        let m2 = provider
            .create_memory(tenant, store.id, m2_req)
            .await
            .unwrap();

        let mut m3_req = make_memory_request("agent-1");
        m3_req.scope.tenant_id = tenant.into();
        m3_req.content = serde_json::json!({"text": "Node C"});
        let m3 = provider
            .create_memory(tenant, store.id, m3_req)
            .await
            .unwrap();

        let edge1 = provider
            .create_edge(
                tenant,
                store.id,
                kd6_core::models::CreateEdgeRequest {
                    source_memory_id: m1.id,
                    target_memory_id: m2.id,
                    relation_type: "related_to".into(),
                    weight: 1.0,
                    metadata: serde_json::json!({}),
                },
            )
            .await
            .unwrap();
        assert_eq!(edge1.source_memory_id, m1.id);
        assert_eq!(edge1.target_memory_id, m2.id);

        let _edge2 = provider
            .create_edge(
                tenant,
                store.id,
                kd6_core::models::CreateEdgeRequest {
                    source_memory_id: m2.id,
                    target_memory_id: m3.id,
                    relation_type: "depends_on".into(),
                    weight: 0.8,
                    metadata: serde_json::json!({}),
                },
            )
            .await
            .unwrap();

        let result = provider
            .graph_traverse(
                tenant,
                store.id,
                kd6_core::models::GraphTraversalRequest {
                    start_memory_id: m1.id,
                    depth: 2,
                    relation_types: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(result.nodes.len(), 3);
        assert_eq!(result.edges.len(), 2);

        let result = provider
            .graph_traverse(
                tenant,
                store.id,
                kd6_core::models::GraphTraversalRequest {
                    start_memory_id: m1.id,
                    depth: 1,
                    relation_types: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(result.nodes.len(), 2);
        assert_eq!(result.edges.len(), 1);

        let result = provider
            .graph_traverse(
                tenant,
                store.id,
                kd6_core::models::GraphTraversalRequest {
                    start_memory_id: m1.id,
                    depth: 2,
                    relation_types: Some(vec!["related_to".into()]),
                },
            )
            .await
            .unwrap();
        assert_eq!(result.nodes.len(), 2);
        assert_eq!(result.edges.len(), 1);
    }

    #[tokio::test]
    async fn test_graph_delete_edge() {
        let provider = test_provider().await;
        let tenant = "t-graph-del";
        let store = provider
            .create_store(
                tenant,
                CreateStoreRequest {
                    name: "graph-del-store".into(),
                    region: None,
                    config: StoreConfig::default(),
                    metadata: Default::default(),
                },
            )
            .await
            .unwrap();

        let mut m1_req = make_memory_request("agent-1");
        m1_req.scope.tenant_id = tenant.into();
        m1_req.content = serde_json::json!({"text": "X"});
        let m1 = provider
            .create_memory(tenant, store.id, m1_req)
            .await
            .unwrap();

        let mut m2_req = make_memory_request("agent-1");
        m2_req.scope.tenant_id = tenant.into();
        m2_req.content = serde_json::json!({"text": "Y"});
        let m2 = provider
            .create_memory(tenant, store.id, m2_req)
            .await
            .unwrap();

        let edge = provider
            .create_edge(
                tenant,
                store.id,
                kd6_core::models::CreateEdgeRequest {
                    source_memory_id: m1.id,
                    target_memory_id: m2.id,
                    relation_type: "test".into(),
                    weight: 1.0,
                    metadata: serde_json::json!({}),
                },
            )
            .await
            .unwrap();

        provider
            .delete_edge(tenant, store.id, edge.id)
            .await
            .unwrap();
        let err = provider.delete_edge(tenant, store.id, edge.id).await;
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn test_gdpr_purge() {
        let provider = test_provider().await;
        let tenant = "t-gdpr";
        let store = provider
            .create_store(
                tenant,
                CreateStoreRequest {
                    name: "gdpr-store".into(),
                    region: None,
                    config: StoreConfig::default(),
                    metadata: Default::default(),
                },
            )
            .await
            .unwrap();

        let mut req1 = make_memory_request("agent-1");
        req1.scope.tenant_id = tenant.into();
        req1.scope.user_id = Some("user-a".into());
        req1.content = serde_json::json!({"text": "User A data"});
        let mut req2 = make_memory_request("agent-1");
        req2.scope.tenant_id = tenant.into();
        req2.scope.user_id = Some("user-a".into());
        req2.content = serde_json::json!({"text": "User A more data"});
        let mut req3 = make_memory_request("agent-1");
        req3.scope.tenant_id = tenant.into();
        req3.scope.user_id = Some("user-b".into());
        req3.content = serde_json::json!({"text": "User B data"});

        provider
            .create_memory(tenant, store.id, req1)
            .await
            .unwrap();
        provider
            .create_memory(tenant, store.id, req2)
            .await
            .unwrap();
        provider
            .create_memory(tenant, store.id, req3)
            .await
            .unwrap();

        let deleted = provider
            .gdpr_purge(
                tenant,
                store.id,
                kd6_core::models::MemoryScope {
                    tenant_id: tenant.into(),
                    org_id: None,
                    team_id: None,
                    project_id: None,
                    user_id: Some("user-a".into()),
                    agent_id: None,
                    session_id: None,
                    run_id: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(deleted, 2);

        let remaining = provider
            .list_memories(tenant, store.id, ListMemoriesFilter::default())
            .await
            .unwrap();
        assert_eq!(remaining.items.len(), 1);
    }

    #[tokio::test]
    async fn test_gdpr_purge_rejects_empty_scope() {
        let provider = test_provider().await;
        let tenant = "t-gdpr-empty";
        let store = provider
            .create_store(
                tenant,
                CreateStoreRequest {
                    name: "gdpr-guard-store".into(),
                    region: None,
                    config: StoreConfig::default(),
                    metadata: Default::default(),
                },
            )
            .await
            .unwrap();

        let result = provider
            .gdpr_purge(tenant, store.id, MemoryScope::default())
            .await;
        assert!(result.is_err(), "empty scope should be rejected");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("at least one scope field"),
            "error should mention scope requirement: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_temporal_metadata() {
        let provider = test_provider().await;
        let tenant = "t-temporal";
        let store = provider
            .create_store(
                tenant,
                CreateStoreRequest {
                    name: "temporal-store".into(),
                    region: None,
                    config: StoreConfig::default(),
                    metadata: Default::default(),
                },
            )
            .await
            .unwrap();

        let now = chrono::Utc::now();
        let future = now + chrono::Duration::hours(24);

        let mut req = make_memory_request("agent-1");
        req.scope.tenant_id = tenant.into();
        req.content = serde_json::json!({"text": "temporal fact"});
        req.valid_from = Some(now);
        req.valid_until = Some(future);
        req.confidence = Some(0.95);
        req.entity_type = Some("fact".into());

        let entry = provider.create_memory(tenant, store.id, req).await.unwrap();
        assert!(entry.valid_from.is_some());
        assert!(entry.valid_until.is_some());
        assert_eq!(entry.confidence, Some(0.95));
        assert_eq!(entry.entity_type.as_deref(), Some("fact"));

        let fetched = provider
            .get_memory(tenant, store.id, entry.id)
            .await
            .unwrap();
        assert!(fetched.valid_from.is_some());
        assert!(fetched.valid_until.is_some());
        assert_eq!(fetched.confidence, Some(0.95));
        assert_eq!(fetched.entity_type.as_deref(), Some("fact"));
    }

    #[tokio::test]
    async fn test_sovereignty_config() {
        let provider = test_provider().await;
        let tenant = "t-sovereignty";
        let store = provider
            .create_store(
                tenant,
                CreateStoreRequest {
                    name: "sovereign-store".into(),
                    region: None,
                    config: StoreConfig::default(),
                    metadata: Default::default(),
                },
            )
            .await
            .unwrap();

        assert_eq!(
            store.sovereignty.mode,
            kd6_core::models::sovereignty::SovereigntyMode::Any
        );
    }
}
