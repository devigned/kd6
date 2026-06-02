-- Audit log for all write operations
CREATE TABLE IF NOT EXISTS audit_log (
    id TEXT PRIMARY KEY NOT NULL,
    store_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    memory_id TEXT,
    action TEXT NOT NULL,  -- 'create', 'update', 'delete', 'purge', 'batch_create', 'batch_delete'
    agent_id TEXT,
    details_json TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_audit_store_tenant ON audit_log(store_id, tenant_id);
CREATE INDEX IF NOT EXISTS idx_audit_memory ON audit_log(memory_id);
CREATE INDEX IF NOT EXISTS idx_audit_created ON audit_log(store_id, created_at);

-- FTS5 virtual table for keyword/BM25 search on memory content
CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
    memory_id UNINDEXED,
    content,
    content=memories,
    content_rowid=rowid
);

-- Triggers to keep FTS in sync with memories table
CREATE TRIGGER IF NOT EXISTS memories_fts_insert AFTER INSERT ON memories BEGIN
    INSERT INTO memories_fts(rowid, memory_id, content)
    VALUES (new.rowid, new.id, new.content_json);
END;

CREATE TRIGGER IF NOT EXISTS memories_fts_delete AFTER DELETE ON memories BEGIN
    INSERT INTO memories_fts(memories_fts, rowid, memory_id, content)
    VALUES ('delete', old.rowid, old.id, old.content_json);
END;

CREATE TRIGGER IF NOT EXISTS memories_fts_update AFTER UPDATE ON memories BEGIN
    INSERT INTO memories_fts(memories_fts, rowid, memory_id, content)
    VALUES ('delete', old.rowid, old.id, old.content_json);
    INSERT INTO memories_fts(rowid, memory_id, content)
    VALUES (new.rowid, new.id, new.content_json);
END;

-- Inheritance relationships
CREATE TABLE IF NOT EXISTS inheritance (
    id TEXT PRIMARY KEY NOT NULL,
    store_id TEXT NOT NULL REFERENCES stores(id) ON DELETE CASCADE,
    tenant_id TEXT NOT NULL,
    parent_agent_id TEXT NOT NULL,
    child_agent_id TEXT NOT NULL,
    inherit_layers_json TEXT NOT NULL DEFAULT '[]',
    filter_json TEXT NOT NULL DEFAULT '{}',
    access TEXT NOT NULL DEFAULT 'read_only',  -- 'read_only' or 'read_write'
    bubble_up_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_inheritance_store ON inheritance(store_id, tenant_id);
CREATE INDEX IF NOT EXISTS idx_inheritance_parent ON inheritance(parent_agent_id);
CREATE INDEX IF NOT EXISTS idx_inheritance_child ON inheritance(child_agent_id);

-- Shared memory spaces (blackboard pattern)
CREATE TABLE IF NOT EXISTS shared_spaces (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    store_id TEXT NOT NULL REFERENCES stores(id) ON DELETE CASCADE,
    tenant_id TEXT NOT NULL,
    scope_json TEXT NOT NULL,
    layer TEXT NOT NULL DEFAULT 'working',
    conflict_resolution TEXT NOT NULL DEFAULT 'last_write_wins',
    notify_on_write INTEGER NOT NULL DEFAULT 0,
    notify_on_delete INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_shared_spaces_store ON shared_spaces(store_id, tenant_id);

CREATE TABLE IF NOT EXISTS space_participants (
    space_id TEXT NOT NULL REFERENCES shared_spaces(id) ON DELETE CASCADE,
    agent_id TEXT NOT NULL,
    access TEXT NOT NULL DEFAULT 'read_write',  -- 'read_only', 'read_write', 'admin'
    joined_at TEXT NOT NULL,
    PRIMARY KEY (space_id, agent_id)
);
