-- Memory stores
CREATE TABLE IF NOT EXISTS stores (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    region TEXT,
    config_json TEXT NOT NULL DEFAULT '{}',
    metadata_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_stores_tenant ON stores(tenant_id);

-- Memory entries
CREATE TABLE IF NOT EXISTS memories (
    id TEXT PRIMARY KEY NOT NULL,
    store_id TEXT NOT NULL REFERENCES stores(id) ON DELETE CASCADE,
    tenant_id TEXT NOT NULL,
    layer TEXT NOT NULL DEFAULT 'working',
    content_json TEXT NOT NULL,
    embedding BLOB,
    owner_agent_id TEXT NOT NULL,
    -- Scope columns (flattened for efficient filtering)
    scope_tenant_id TEXT NOT NULL,
    scope_org_id TEXT,
    scope_team_id TEXT,
    scope_project_id TEXT,
    scope_user_id TEXT,
    scope_agent_id TEXT,
    scope_session_id TEXT,
    scope_run_id TEXT,
    -- Metadata
    tags_json TEXT NOT NULL DEFAULT '[]',
    categories_json TEXT NOT NULL DEFAULT '[]',
    source_json TEXT,
    -- Access control
    access_policy TEXT NOT NULL DEFAULT 'private',
    allowed_agents_json TEXT,
    allowed_scopes_json TEXT,
    -- Lifecycle
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    expires_at TEXT,
    immutable INTEGER NOT NULL DEFAULT 0,
    version INTEGER NOT NULL DEFAULT 1
);

CREATE INDEX IF NOT EXISTS idx_memories_store ON memories(store_id);
CREATE INDEX IF NOT EXISTS idx_memories_tenant ON memories(tenant_id);
CREATE INDEX IF NOT EXISTS idx_memories_store_tenant ON memories(store_id, tenant_id);
CREATE INDEX IF NOT EXISTS idx_memories_layer ON memories(store_id, layer);
CREATE INDEX IF NOT EXISTS idx_memories_owner ON memories(store_id, owner_agent_id);
CREATE INDEX IF NOT EXISTS idx_memories_scope ON memories(scope_tenant_id, scope_org_id, scope_team_id, scope_project_id);
