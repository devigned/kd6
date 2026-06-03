# Features

KD6 implements all three conformance levels of the Open Memory Service (OMS)
specification. This document describes the shipped behavior of the reference
implementation, including the HTTP API, store-name routing, automatic
embeddings, and advanced memory management features.

## Level 1: Core

The foundation of KD6 — stores, memories, search, isolation, and capability
discovery.

### Store Management

Memory stores are named containers for a tenant's memories. Each store has its
own configuration, metadata, and optional sovereignty settings.

**Operations:** create, get, list, update, delete

**API routes:**
- `POST /v1/stores`
- `GET /v1/stores`
- `GET /v1/stores/{store_name}`
- `PATCH /v1/stores/{store_name}`
- `DELETE /v1/stores/{store_name}`

**Store-name routing:**
- All store-scoped API routes use `{store_name}`, not a UUID.
- Store names are immutable after creation.
- Store names are unique per tenant.
- KD6 resolves the store name to an internal UUID at the server layer.
- `UpdateStoreRequest` no longer includes `name`; store updates only accept
  `config` and `metadata`.

### Auto-Provisioning

KD6 provides a zero-config path for getting started quickly:

- The `_default` store alias creates a store on first use when auto-provisioning
  is enabled.
- If the `X-Tenant-ID` header is absent and default-tenant resolution is
  enabled, KD6 uses `_default` as the tenant.
- This allows first-write and search-before-write flows without an explicit
  setup call.

For explicitly named stores, clients can still create stores ahead of time with
`POST /v1/stores`.

### Memory CRUD

Memories are the core data unit. Each memory stores JSON content, a memory
layer, owner, hierarchical scope, tags, categories, access control, lifecycle
fields, optional temporal metadata, optional graph metadata, optional
embeddings, and an optional `upsert_key`.

**Operations:** create, get, list, update, delete

**API routes:**
- `POST /v1/stores/{store_name}/memories`
- `GET /v1/stores/{store_name}/memories`
- `GET /v1/stores/{store_name}/memories/{memory_id}`
- `PATCH /v1/stores/{store_name}/memories/{memory_id}`
- `DELETE /v1/stores/{store_name}/memories/{memory_id}`

**Listing filters:**
- `layer` — filter by memory layer
- `tags` — filter by tag values
- `categories` — filter by category values
- `owner_agent_id` — filter by owning agent
- `scope` — filter by scope fields
- `limit` / `offset` — pagination

### Upsert Support

KD6 supports idempotent create-or-replace writes through `upsert_key`.

- Callers may set `upsert_key` in `CreateMemoryRequest`.
- If a memory already exists with the same `upsert_key`, store, layer, and
  scope, KD6 replaces that memory instead of creating a duplicate.
- If no match exists, KD6 creates a new memory normally.
- This is useful for singleton-style facts such as “latest status of task X” or
  “current preference for agent Y”.

The operation is atomic and participates in normal audit logging.

### Server-Side Embedding

KD6 can compute embeddings automatically for both writes and queries.

- On create and batch create, KD6 computes an embedding when the request omits
  `embedding`.
- On update, KD6 recomputes the embedding when `content` changes and the request
  does not explicitly set or clear `embedding`.
- On search, KD6 computes the query embedding when the request omits
  `embedding`.
- If the caller provides an embedding, KD6 preserves it and does not overwrite
  it.
- If an embedding is provided with the wrong dimensionality for the configured
  provider, KD6 rejects the request.

**Configuration:**
- `KD6_EMBEDDING_PROVIDER=local` — local fastembed provider using
  `all-MiniLM-L6-v2`
- `KD6_EMBEDDING_PROVIDER=openai-compatible` — OpenAI-compatible HTTP provider
- `KD6_EMBEDDING_PROVIDER=none` — disable server-side embedding generation

Related environment variables include `KD6_EMBEDDING_ENDPOINT`,
`KD6_EMBEDDING_MODEL`, and `KD6_EMBEDDING_DIMENSIONS` for the
`openai-compatible` provider.

### Search

KD6 supports both vector and keyword search through a single endpoint:

- `POST /v1/stores/{store_name}/search`

**Vector search:**
- Uses cosine similarity over stored embeddings
- Returns ranked results
- In SQLite, vector comparison is brute-force and suitable for small to
  medium-sized datasets

**Keyword search:**
- Uses SQLite FTS5 over memory content
- Sanitizes input to strip FTS5 operators before execution
- Returns relevance-ranked matches

### Tenant Isolation

Every operation is tenant-scoped.

- KD6 filters all reads by tenant.
- KD6 normalizes write scopes to the authenticated tenant.
- Cross-tenant access is not allowed.
- Store names are only unique within a tenant, not globally.

### Capabilities Endpoint

`GET /capabilities` returns the declared feature set of the active provider,
including supported layers, search features, graph support, audit support,
maximum embedding dimensions, and batch limits.

## Level 2: Standard

Production-oriented features for lifecycle management, auditing, scoping, and
multi-agent collaboration.

### Audit Logging

Every mutation records an audit event in the same transaction as the underlying
data change.

Audit entries capture:
- action performed
- tenant, store, and optional memory identifiers
- acting agent when available
- timestamp
- structured details JSON
- the previous entry hash for tamper-evident chaining

**API routes:**
- `GET /v1/stores/{store_name}/audit`
- `GET /v1/stores/{store_name}/memories/{memory_id}/audit`

### TTL and Lifecycle

Memories may include `expires_at`, and stores may define
`config.default_ttl_seconds`.

**API routes:**
- `DELETE /v1/stores/{store_name}/expired` — purge expired memories
- `GET /v1/stores/{store_name}/lifecycle/stats` — return lifecycle statistics

Expired-memory deletion is audited like any other write operation.

### Batch Operations

KD6 supports bulk creation and deletion of memories.

**API routes:**
- `POST /v1/stores/{store_name}/memories/batch`
- `POST /v1/stores/{store_name}/memories/batch/delete`

Batch create applies the same server-side embedding behavior as single-memory
create. Responses include success counts plus per-entry errors for partial
failures.

### Hierarchical Scoping

`MemoryScope` defines eight visibility levels:

```
tenant_id > org_id > team_id > project_id > user_id > agent_id > session_id > run_id
```

A memory is visible to agents whose scope is equal to or more specific than the
memory's scope. This enables tenant-wide, org-wide, team-wide, project-wide,
agent-private, session-specific, and run-specific knowledge boundaries.

### Inheritance and Bubble-Up

Inheritance defines parent-child agent relationships for controlled sharing.
Each inheritance rule specifies:

- `parent_agent_id` and `child_agent_id`
- `inherit_layers` — which layers the child may inherit
- `filter.tags`
- `filter.categories`
- `filter.time_from` / `filter.time_to`
- `filter.max_entries`
- `access` — `read_only` or `read_write`
- `bubble_up` configuration, including enabled layers

**API routes:**
- `POST /v1/stores/{store_name}/inherit`
- `DELETE /v1/stores/{store_name}/inherit/{inheritance_id}`
- `POST /v1/stores/{store_name}/bubble-up`

Bubble-up copies matching memories from child scope into parent scope. KD6
prevents duplicate bubble-up copies by tracking source references with
`bubble_up:{source_memory_id}` provenance.

### Shared Spaces

Shared spaces provide blackboard-style collaboration for multiple agents inside
a store.

Each shared space has:
- a name
- a target layer
- a scope
- a conflict resolution policy
- a participant list with per-agent access

**API routes:**
- `POST /v1/stores/{store_name}/shared-spaces`
- `GET /v1/stores/{store_name}/shared-spaces`
- `GET /v1/stores/{store_name}/shared-spaces/{space_id}`
- `POST /v1/stores/{store_name}/shared-spaces/{space_id}/join`
- `POST /v1/stores/{store_name}/shared-spaces/{space_id}/leave`
- `DELETE /v1/stores/{store_name}/shared-spaces/{space_id}`

List queries are optimized: KD6 hydrates participants in batches rather than
issuing an N+1 query per shared space.

## Level 3: Advanced

Advanced features for graph reasoning, temporal knowledge, compliance, and
sovereignty-aware deployments.

### Graph Memory

KD6 supports typed graph edges between memories.

**API routes:**
- `POST /v1/stores/{store_name}/graph/edges`
- `DELETE /v1/stores/{store_name}/graph/edges/{edge_id}`
- `POST /v1/stores/{store_name}/graph/traverse`

Traversal uses breadth-first search with configurable depth and relation-type
filters, plus implementation limits to protect the server from unbounded graph
expansion.

### Temporal Metadata

Memories may include:
- `valid_from`
- `valid_until`
- `confidence`

These fields let agents reason about when a fact became true, when it stopped
being true, and how reliable it is.

### GDPR Purge

KD6 supports right-to-be-forgotten workflows.

**API route:**
- `POST /v1/stores/{store_name}/gdpr/purge`

A purge request deletes memories matching the requested scope and anonymizes the
associated audit payload while preserving audit-trail continuity.

### Cryptographic Audit Trail

Audit entries form a SHA-256 hash chain. Each entry includes the previous hash,
so tampering with historical audit data breaks the chain from that point
forward.

### Data Sovereignty

Stores may carry sovereignty configuration such as:
- `mode` — `unrestricted`, `region_locked`, or `tenant_controlled`
- `allowed_regions` — permitted residency or processing regions

This configuration is stored with the store and can be enforced by deployments
that need geographic control over memory data.

## Optimistic Concurrency

Memory updates use version-based optimistic concurrency.

- Each memory has a `version` field.
- Updates increment `version`.
- Conflicting writes return `409 Conflict` rather than silently overwriting
  another writer's change.

## Access Control

Each memory carries an `AccessControl` structure:

- `policy` — `private`, `inherit`, `shared`, or `public_read`
- `allowed_agents` — optional explicit agent allowlist
- `allowed_scopes` — optional explicit scope allowlist

This metadata is stored with the memory and can be combined with scoping,
inheritance, and shared-space workflows.

## Immutable Memories

A memory may be created with `immutable: true`.

- Immutable memories cannot be modified.
- Immutable memories cannot be deleted.
- Attempts to update or delete them return `409 Conflict`.

## Error Handling

KD6 returns structured JSON errors.

Important conflict cases include:
- optimistic concurrency failures
- immutable-memory violations
- constraint violations such as duplicate store names within a tenant

Constraint violations return HTTP `409 Conflict` with a specific error message,
which is especially relevant for store creation because store names are unique
per tenant.
