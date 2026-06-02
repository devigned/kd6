# Architecture

KD6 is structured as a Cargo workspace with four crates. Each crate has a
single responsibility, and dependencies flow in one direction: from the server
and MCP binaries down through the SQLite backend to the core domain types.

```
kd6-server (HTTP API)     kd6-mcp (MCP server)
         \                      /
          \                    /
           kd6-sqlite (backend)
                  |
             kd6-core (domain)
```

## Crate Overview

### kd6-core

Pure domain crate with no I/O. Contains:

- **`OmsProvider` trait** -- the Service Provider Interface (SPI) that all
  backends must implement. Roughly 25 async methods spanning stores, memories,
  search, audit, lifecycle, batch operations, inheritance, shared spaces, graph
  traversal, and GDPR purge.
- **Domain models** -- `MemoryEntry`, `MemoryStore`, `MemoryScope`,
  `GraphEdge`, `SharedSpace`, `AuditEntry`, and all request/response types.
- **Error types** -- `OmsError` with variants for not-found, conflict,
  immutable, unauthorized, invalid input, and internal errors.

This crate has zero runtime dependencies beyond serde, uuid, chrono, and
thiserror. It compiles in seconds and can be depended on by any future backend
without pulling in SQLite, HTTP, or MCP machinery.

### kd6-sqlite

The SQLite backend. Implements `OmsProvider` using sqlx with async connection
pooling. This is the only crate that touches the database.

Key internals:

- **Connection pool** -- `SqlitePool` configured with WAL mode and foreign keys
  enabled on every connection via `SqliteConnectOptions` pragmas.
- **Migrations** -- Three SQL migration files applied at startup via
  `sqlx::migrate!()`:
  - `20260601000000_initial.sql` -- stores and memories tables (Level 1)
  - `20260601000001_level2.sql` -- audit log, FTS5, inheritance, shared spaces
  - `20260601000002_level3.sql` -- temporal columns, graph edges, audit hash
    chain, sovereignty
- **Vector search** -- Brute-force cosine similarity over f32 embeddings stored
  as little-endian byte blobs. Capped at 10,000 candidate rows per query.
- **Keyword search** -- SQLite FTS5 with input sanitization against injection.
- **Transaction discipline** -- All write operations (create, update, delete,
  GDPR purge, TTL purge) run inside `BEGIN IMMEDIATE` transactions that include
  both the data mutation and the audit log entry. If any step fails, the entire
  operation rolls back.

### kd6-server

An axum HTTP server that wraps the `OmsProvider` trait with a REST API. All
routes live under `/v1/` with a root-level `/health` and `/capabilities`
endpoint.

Design choices:

- **Custom extractors** -- `TenantId` and `AgentId` are pulled from headers
  (`X-Tenant-Id`, `X-Agent-Id`). `JsonBody<T>` and `PathId<T>` wrap axum's
  built-in extractors to return JSON error responses instead of plain text on
  parse failures.
- **Body limit** -- 10 MB default via `DefaultBodyLimit`.
- **Shared state** -- `AppState` holds an `Arc<dyn OmsProvider>`, making the
  server backend-agnostic. Swap in a Postgres provider and nothing else changes.

### kd6-mcp

An MCP (Model Context Protocol) server that exposes KD6 operations as tools.
Built with the rmcp crate. Supports two transport modes:

- **Streamable HTTP** (default) -- runs as a standalone HTTP server on port
  8081, accessible at `/mcp`. Uses `StreamableHttpService` with per-session
  state management.
- **Stdio** -- launched as a child process by MCP clients, communicating over
  stdin/stdout.

Agents that speak MCP can call nine tools directly without going through the
REST API.

Tools exposed: `create_store`, `list_stores`, `create_memory`, `get_memory`,
`search_memories`, `create_edge`, `traverse_graph`, `gdpr_purge`,
`delete_memory`.

## Multi-Tenancy Model

Tenant isolation is the outermost boundary. Every API call requires a
`tenant_id`, and every query includes a `WHERE tenant_id = ?` clause. There is
no cross-tenant data access at any level.

Within a tenant, memories are organized hierarchically using `MemoryScope`:

```
tenant_id
  org_id
    team_id
      project_id
        user_id
          agent_id
            session_id
              run_id
```

The `MemoryScope.normalize(tenant_id)` method forces the tenant field to the
authenticated value on every write path, preventing scope escalation.

## Memory Layers

KD6 implements the five-layer memory model from the OMS specification:

| Layer | Purpose | Typical TTL |
|---|---|---|
| **Working** | Scratch space for in-flight agent tasks | Minutes to hours |
| **Episodic** | Records of specific events and interactions | Days to weeks |
| **Semantic** | Distilled facts and knowledge | Weeks to months |
| **Procedural** | Learned processes and patterns | Long-lived |
| **Archival** | Historical record, rarely accessed | Indefinite |

Memories can be promoted between layers using the inheritance and bubble-up
mechanisms.

## Concurrency and Consistency

- **Optimistic concurrency** -- Every `MemoryEntry` carries a `version` counter.
  Updates require `WHERE version = ?` to succeed; a mismatch returns
  `OmsError::Conflict`.
- **WAL mode** -- SQLite is configured in Write-Ahead Logging mode for
  concurrent readers alongside a single writer.
- **Immediate transactions** -- Write operations use `BEGIN IMMEDIATE` to
  acquire the write lock up front, avoiding deadlocks under contention.

## Audit Trail

Every mutation (create, update, delete, GDPR purge, TTL purge) generates an
audit log entry within the same transaction as the data change. The audit system
supports:

- **Hash chain** -- Each audit entry includes a SHA-256 hash linking it to the
  previous entry, enabling tamper detection.
- **GDPR anonymization** -- When a GDPR purge runs, the associated audit entries
  have their `agent_id` and `details_json` fields nulled out. The structural
  record of "something happened" remains, but personal data is removed.

## Graph Memory

Level 3 adds a knowledge graph layer on top of flat memory entries. Edges
connect memories with typed relationships (`related_to`, `depends_on`,
`derived_from`, or any custom string) and carry weights and metadata.

Traversal uses breadth-first search with three safety caps:

- Maximum depth: 10
- Maximum total nodes visited: 1,000
- Maximum edges loaded per node: 500

These bounds prevent runaway traversals on densely connected graphs.

## Design Decisions

**Why sqlx over rusqlite?** sqlx is async-native and uses the same API surface
for SQLite, Postgres, and MySQL. Migrating to Postgres later means changing
connection strings and adjusting SQL dialect, not rewriting the data layer.

**Why brute-force vector search?** SQLite has no native vector index. For the
reference implementation, scanning up to 10,000 rows with cosine similarity is
fast enough (single-digit milliseconds on modern hardware). A production
deployment targeting millions of memories should use a dedicated vector database
or Postgres with pgvector.

**Why FTS5 for keyword search?** FTS5 is built into SQLite, requires no
external dependencies, and handles tokenization, stemming, and ranking out of
the box. Query inputs are sanitized to prevent FTS5 syntax injection.

**Why separate HTTP and MCP servers?** Not all consumers speak HTTP. Agents
running locally often prefer MCP over stdio. Keeping them as separate binaries
with the same underlying provider means both interfaces stay in sync
automatically.
