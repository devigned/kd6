-- Fix upsert index to match full scope (not just scope_tenant_id).
-- Two memories with the same upsert_key but different scopes are distinct entries.
DROP INDEX IF EXISTS idx_memories_upsert;
CREATE INDEX IF NOT EXISTS idx_memories_upsert
    ON memories(store_id, layer, scope_tenant_id, scope_org_id, scope_team_id,
                scope_project_id, scope_user_id, scope_agent_id, scope_session_id,
                scope_run_id, upsert_key)
    WHERE upsert_key IS NOT NULL;

-- Add composite index for memory pagination (list_memories ORDER BY created_at DESC).
CREATE INDEX IF NOT EXISTS idx_memories_store_tenant_created
    ON memories(store_id, tenant_id, created_at DESC);

-- Add redacted flag to audit_log for GDPR anonymization.
-- When TRUE, the entry's content has been anonymized but its hash chain
-- (entry_hash, prev_hash) remains intact for cryptographic verification.
ALTER TABLE audit_log ADD COLUMN redacted INTEGER NOT NULL DEFAULT 0;

-- Add unique constraint for store names within a tenant (prevents _default race).
CREATE UNIQUE INDEX IF NOT EXISTS idx_stores_tenant_name
    ON stores(tenant_id, name);
