-- Temporal metadata on memories
ALTER TABLE memories ADD COLUMN valid_from TEXT;
ALTER TABLE memories ADD COLUMN valid_until TEXT;
ALTER TABLE memories ADD COLUMN confidence REAL;

-- Graph relationships
ALTER TABLE memories ADD COLUMN entity_type TEXT;

CREATE TABLE IF NOT EXISTS graph_edges (
    id TEXT PRIMARY KEY NOT NULL,
    store_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    source_memory_id TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    target_memory_id TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    relation_type TEXT NOT NULL,  -- e.g., 'related_to', 'part_of', 'depends_on'
    weight REAL DEFAULT 1.0,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_graph_edges_source ON graph_edges(source_memory_id);
CREATE INDEX IF NOT EXISTS idx_graph_edges_target ON graph_edges(target_memory_id);
CREATE INDEX IF NOT EXISTS idx_graph_edges_store ON graph_edges(store_id, tenant_id);

-- Cryptographic audit trail: add prev_hash column for hash chaining
ALTER TABLE audit_log ADD COLUMN entry_hash TEXT;
ALTER TABLE audit_log ADD COLUMN prev_hash TEXT;

-- Data sovereignty config on stores
ALTER TABLE stores ADD COLUMN sovereignty_json TEXT NOT NULL DEFAULT '{"mode":"any"}';
