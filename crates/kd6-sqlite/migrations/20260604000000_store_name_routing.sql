-- Add unique index on (tenant_id, name) to enforce name uniqueness per tenant
-- and enable efficient name-based store lookups.
CREATE UNIQUE INDEX IF NOT EXISTS idx_stores_tenant_name ON stores(tenant_id, name);
