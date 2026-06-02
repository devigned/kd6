# Features

KD6 implements all three conformance levels of the Open Memory Service (OMS)
specification. This document covers each feature, how it works, and where it
lives in the codebase.

## Level 1: Core

The foundation. Any OMS-compliant system must support these.

### Store Management

Memory stores are named containers that hold memories for a tenant. Each store
has its own configuration, metadata, and optional sovereignty settings.

**Operations:** create, get, list, update, delete

**API routes:**
- `POST /v1/stores`
- `GET /v1/stores`
- `GET /v1/stores/{store_id}`
- `PATCH /v1/stores/{store_id}`
- `DELETE /v1/stores/{store_id}`

### Memory CRUD

Memories are the core data unit. Each memory has structured JSON content, a
layer classification, ownership, scope, tags, categories, access control, and
optional embeddings.

**Operations:** create, get, list (with filters), update (partial), delete

**API routes:**
- `POST /v1/stores/{store_id}/memories`
- `GET /v1/stores/{store_id}/memories`
- `GET /v1/stores/{store_id}/memories/{memory_id}`
- `PATCH /v1/stores/{store_id}/memories/{memory_id}`
- `DELETE /v1/stores/{store_id}/memories/{memory_id}`

**Listing filters:**
- `layer` -- filter by memory layer
- `tags` -- filter by tag values
- `categories` -- filter by category values
- `owner_agent_id` -- filter by owning agent
- `scope` -- filter by scope fields
- `limit` / `offset` -- pagination

### Vector Search

Semantic search using cosine similarity over stored embeddings. Callers provide
an embedding vector and receive ranked results.

The SQLite implementation uses brute-force comparison, scanning up to 10,000
candidate rows. This is practical for small to medium datasets. For larger
deployments, a backend with native vector indexing (such as Postgres with
pgvector) is recommended.

### Keyword Search

Full-text search powered by SQLite FTS5. The `content` field of each memory is
indexed for fast text matching with relevance ranking.

Input queries are sanitized to prevent FTS5 syntax injection. Special characters
and operators are stripped before the query reaches the FTS5 engine.

### Tenant Isolation

Every operation requires a `tenant_id`. All queries include a tenant filter,
and all writes normalize the scope to use the authenticated tenant ID. There is
no mechanism to access another tenant's data.

### Capabilities Endpoint

`GET /capabilities` returns the feature set of the running backend:

```json
{
  "supported_layers": ["working", "episodic", "semantic", "procedural", "archival"],
  "vector_search": true,
  "keyword_search": true,
  "graph_support": true,
  "temporal_queries": true,
  "batch_operations": true,
  "audit_log": true,
  "max_batch_size": 100,
  "max_embedding_dimensions": 4096,
  "supported_distance_metrics": ["cosine"]
}
```

## Level 2: Standard

Extended features for production workloads.

### Audit Logging

Every mutation generates an audit entry in the same transaction as the data
change. Audit entries record:

- Action performed (create, update, delete, purge)
- Tenant, store, and memory IDs
- Agent ID of the actor
- Timestamp
- JSON details of the change
- Hash linking to the previous entry (tamper detection)

**API routes:**
- `GET /v1/stores/{store_id}/audit` -- store-level audit log
- `GET /v1/stores/{store_id}/memories/{memory_id}/audit` -- memory-level audit

**Query parameters:** `limit`, `offset`, `action` (filter by action type)

### TTL and Lifecycle

Memories can have an `expires_at` timestamp. The lifecycle system provides:

- **TTL purge** -- `DELETE /v1/stores/{store_id}/expired` removes all expired
  memories and logs audit entries for each deletion.
- **Lifecycle stats** -- `GET /v1/stores/{store_id}/lifecycle/stats` returns
  store statistics.
- **Default TTL** -- Stores can set `config.default_ttl_seconds` to
  automatically assign expiration to new memories.

### Batch Operations

Create or delete multiple memories in a single request with partial failure
handling. Entries that fail validation are reported individually while
successful entries are committed.

- `POST /v1/stores/{store_id}/memories/batch` -- batch create (up to 100
  entries per request)
- `POST /v1/stores/{store_id}/memories/batch/delete` -- batch delete

Response includes both `created`/`deleted` counts and an `errors` array for
any entries that failed.

### Hierarchical Scoping

The `MemoryScope` structure provides eight levels of hierarchy:

```
tenant_id > org_id > team_id > project_id > user_id > agent_id > session_id > run_id
```

Memories are visible to agents whose scope is equal to or more specific than the
memory's scope. This enables natural patterns like:

- Org-wide knowledge visible to all teams
- Team-specific decisions hidden from other teams
- Agent-private working memory invisible to other agents

### Inheritance and Bubble-Up

Inheritance rules define how memories flow between layers. A rule specifies:

- Source layer and target layer
- Which tags to match
- Whether to include content, embeddings, or both

The **bubble-up** operation executes an inheritance rule, copying matching
memories from the source layer to the target layer. Deduplication uses source
reference URIs (`bubble_up:{source_memory_id}`) to prevent duplicates on
repeated runs.

**API routes:**
- `POST /v1/stores/{store_id}/inherit` -- create a rule
- `DELETE /v1/stores/{store_id}/inherit/{inheritance_id}` -- delete a rule
- `POST /v1/stores/{store_id}/bubble-up` -- execute bubble-up

### Shared Spaces

Shared spaces allow multiple agents to read and write memories in a designated
layer within a store. Each space has:

- A name and target layer
- A list of member agent IDs
- Join/leave operations

**API routes:**
- `POST /v1/stores/{store_id}/shared-spaces`
- `GET /v1/stores/{store_id}/shared-spaces`
- `GET /v1/stores/{store_id}/shared-spaces/{space_id}`
- `POST /v1/stores/{store_id}/shared-spaces/{space_id}/join`
- `POST /v1/stores/{store_id}/shared-spaces/{space_id}/leave`
- `DELETE /v1/stores/{store_id}/shared-spaces/{space_id}`

## Level 3: Advanced

Features for compliance, knowledge graphs, and temporal reasoning.

### Graph Memory

A knowledge graph layer built on top of memory entries. Edges connect memories
with typed relationships and carry weights and arbitrary metadata.

**Edge types** are freeform strings. Common examples: `related_to`,
`depends_on`, `derived_from`, `contradicts`, `supersedes`.

**Traversal** uses breadth-first search from a starting node with configurable:
- Depth limit (capped at 10)
- Relation type filter
- Breadth limits (1,000 total nodes, 500 edges per node)

**API routes:**
- `POST /v1/stores/{store_id}/graph/edges` -- create edge
- `DELETE /v1/stores/{store_id}/graph/edges/{edge_id}` -- delete edge
- `POST /v1/stores/{store_id}/graph/traverse` -- BFS traversal

### Temporal Metadata

Memories can carry temporal validity information:

- `valid_from` -- when this fact became true
- `valid_until` -- when this fact stopped being true (or will stop)
- `confidence` -- a 0.0 to 1.0 score indicating certainty

This enables agents to reason about time-sensitive knowledge. A fact about
"current API rate limits" might have `valid_from: 2025-01-01` and
`valid_until: 2025-07-01`, letting agents know whether the information is still
applicable.

### GDPR Purge

Right-to-be-forgotten support. Given a scope (which must include at least one
field beyond `tenant_id`), the purge:

1. Finds all memories matching the scope
2. Deletes them
3. Anonymizes associated audit log entries (nulls `agent_id` and
   `details_json`)
4. Runs everything in a single transaction

The audit skeleton remains (timestamps, action types) to satisfy regulatory
requirements for audit trail continuity, but personal data is removed.

**API route:** `POST /v1/stores/{store_id}/gdpr/purge`

### Cryptographic Audit Trail

Audit entries form a hash chain using SHA-256. Each entry's hash includes the
previous entry's hash, creating an append-only log where tampering with any
entry breaks the chain from that point forward.

The chain is maintained per-store and uses `BEGIN IMMEDIATE` transactions with
`rowid` ordering to guarantee consistency under concurrent access.

### Data Sovereignty

Stores can be configured with sovereignty settings:

- `mode` -- `unrestricted`, `region_locked`, or `tenant_controlled`
- `allowed_regions` -- list of permitted regions

This metadata is stored and enforced at the store level, enabling deployments
where data must not leave specific geographic boundaries.

## Optimistic Concurrency

All memory updates use version-based optimistic concurrency control. The
`version` field on each `MemoryEntry` is incremented on every update, and
the update query includes `WHERE version = ?`. If another writer modified the
entry since the caller last read it, the update fails with a `409 Conflict`
response.

This approach avoids locking while preventing lost updates in concurrent
environments.

## Access Control

Each memory entry has an `AccessControl` structure with three fields:

- `policy` -- one of `private`, `inherit`, `shared`, or `public_read`
- `allowed_agents` -- optional list of agent IDs with access
- `allowed_scopes` -- optional list of scope identifiers with access

The access control metadata is stored alongside the memory. Enforcement
semantics depend on the deployment context.

## Immutable Memories

Memories can be marked as `immutable: true` at creation time. Once set,
the content cannot be modified or deleted. Attempts to update or delete an
immutable memory return a `409 Conflict` with an explanation.

This is useful for compliance records, signed decisions, and other data that
must be preserved exactly as written.
