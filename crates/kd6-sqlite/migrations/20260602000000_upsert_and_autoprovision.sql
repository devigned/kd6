-- Add upsert_key column to memories (OMS spec 4.3.2)
ALTER TABLE memories ADD COLUMN upsert_key TEXT;

-- Index for efficient upsert lookups: store + layer + scope + upsert_key
CREATE INDEX IF NOT EXISTS idx_memories_upsert
    ON memories(store_id, layer, scope_tenant_id, upsert_key)
    WHERE upsert_key IS NOT NULL;
