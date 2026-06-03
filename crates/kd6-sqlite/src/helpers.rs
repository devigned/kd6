use kd6_core::error::OmsError;
use kd6_core::models::{
    AccessPolicy, ConflictResolution, InheritanceAccess, MemoryLayer, ParticipantAccess,
    SearchQuery,
};
use uuid::Uuid;

pub(crate) fn embedding_to_bytes(embedding: &[f32]) -> Vec<u8> {
    embedding.iter().flat_map(|f| f.to_le_bytes()).collect()
}

pub(crate) fn bytes_to_embedding(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

pub(crate) fn parse_layer(s: &str) -> Result<MemoryLayer, OmsError> {
    match s {
        "working" => Ok(MemoryLayer::Working),
        "episodic" => Ok(MemoryLayer::Episodic),
        "semantic" => Ok(MemoryLayer::Semantic),
        "procedural" => Ok(MemoryLayer::Procedural),
        "archival" => Ok(MemoryLayer::Archival),
        other => Err(OmsError::Internal(format!("unknown layer: {other}"))),
    }
}

pub(crate) fn access_policy_to_str(policy: &AccessPolicy) -> &'static str {
    match policy {
        AccessPolicy::Private => "private",
        AccessPolicy::Inherit => "inherit",
        AccessPolicy::Shared => "shared",
        AccessPolicy::PublicRead => "public_read",
    }
}

pub(crate) fn parse_access_policy(s: &str) -> Result<AccessPolicy, OmsError> {
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

pub(crate) fn parse_inheritance_access(s: &str) -> Result<InheritanceAccess, OmsError> {
    match s {
        "read_only" => Ok(InheritanceAccess::ReadOnly),
        "read_write" => Ok(InheritanceAccess::ReadWrite),
        other => Err(OmsError::Internal(format!(
            "unknown inheritance access: {other}"
        ))),
    }
}

pub(crate) fn inheritance_access_to_str(access: InheritanceAccess) -> &'static str {
    match access {
        InheritanceAccess::ReadOnly => "read_only",
        InheritanceAccess::ReadWrite => "read_write",
    }
}

pub(crate) fn parse_conflict_resolution(s: &str) -> Result<ConflictResolution, OmsError> {
    match s {
        "last_write_wins" => Ok(ConflictResolution::LastWriteWins),
        "orchestrator_merge" => Ok(ConflictResolution::OrchestratorMerge),
        "crdt" => Ok(ConflictResolution::Crdt),
        other => Err(OmsError::Internal(format!(
            "unknown conflict resolution: {other}"
        ))),
    }
}

pub(crate) fn conflict_resolution_to_str(conflict_resolution: ConflictResolution) -> &'static str {
    match conflict_resolution {
        ConflictResolution::LastWriteWins => "last_write_wins",
        ConflictResolution::OrchestratorMerge => "orchestrator_merge",
        ConflictResolution::Crdt => "crdt",
    }
}

pub(crate) fn parse_participant_access(s: &str) -> Result<ParticipantAccess, OmsError> {
    match s {
        "read_only" => Ok(ParticipantAccess::ReadOnly),
        "read_write" => Ok(ParticipantAccess::ReadWrite),
        "admin" => Ok(ParticipantAccess::Admin),
        other => Err(OmsError::Internal(format!(
            "unknown participant access: {other}"
        ))),
    }
}

pub(crate) fn participant_access_to_str(access: ParticipantAccess) -> &'static str {
    match access {
        ParticipantAccess::ReadOnly => "read_only",
        ParticipantAccess::ReadWrite => "read_write",
        ParticipantAccess::Admin => "admin",
    }
}

pub(crate) fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// Escape user input for safe use with FTS5 MATCH queries.
/// Wraps each token in double quotes to prevent FTS5 operator injection.
pub(crate) fn sanitize_fts5_query(input: &str) -> String {
    input
        .split_whitespace()
        .map(|token| format!("\"{}\"", token.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Map a sqlx::Error to an appropriate OmsError.
/// Detects unique constraint violations and maps them to ConstraintViolation;
/// all other database errors become Internal.
pub(crate) fn map_db_error(context: &str, err: sqlx::Error) -> OmsError {
    if let sqlx::Error::Database(ref db_err) = err {
        if let Some(code) = db_err.code() {
            if code == "2067" || code == "1555" || code == "19" {
                return OmsError::ConstraintViolation(format!("{context}: {db_err}"));
            }
        }
        let msg = db_err.message();
        if msg.contains("UNIQUE constraint failed") {
            return OmsError::ConstraintViolation(format!("{context}: {msg}"));
        }
    }
    OmsError::Internal(format!("{context}: {err}"))
}

pub(crate) fn build_search_conditions(
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

pub(crate) fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
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
