# Copilot Instructions -- KD6

## Project Overview

KD6 is the reference implementation of the **Open Memory Service (OMS)**
specification, a standard interface for multilayered, multi-tenant agent memory
services. It enables agentic AI workloads to persist, search, share, and manage
memory across sessions, agents, and tenants.

The project is written in Rust with a SQLite backend. The OMS spec lives in
`spec/oms-spec.md`. KD6 implements all three OMS conformance levels (Core,
Standard, Advanced).

## Build & Test

```bash
cargo build                          # Build
cargo test                           # Run all tests (77 across 4 crates)
cargo test <test_name>               # Run a single test by name
cargo test -p kd6-sqlite             # Run tests for a specific crate
cargo clippy -- -D warnings          # Lint (zero warnings required)
cargo fmt -- --check                 # Format check
```

```bash
# Run the HTTP API server
KD6_DATABASE_URL="sqlite:kd6.db?mode=rwc" cargo run -p kd6-server

# Run the MCP server (Streamable HTTP on port 8081)
KD6_DATABASE_URL="sqlite:kd6.db?mode=rwc" cargo run -p kd6-mcp

# Run the MCP server in stdio mode
KD6_MCP_TRANSPORT=stdio cargo run -p kd6-mcp
```

## Crate Structure

```
crates/
├── kd6-core/      # Domain types, OmsError, OmsProvider trait (pure -- no I/O deps)
├── kd6-sqlite/    # SQLite SPI implementation (sqlx), migrations in migrations/
├── kd6-server/    # Axum HTTP server, route handlers, custom extractors
└── kd6-mcp/       # MCP server (rmcp), 10 tools, Streamable HTTP + stdio transports
```

- **kd6-core** is the shared contract. All other crates depend on it.
- The `OmsProvider` trait (async, object-safe via `async_trait`) is the backend
  SPI. New backends implement this trait.
- **kd6-server** depends on both `kd6-core` and `kd6-sqlite`.
- **kd6-mcp** depends on both `kd6-core` and `kd6-sqlite`.
- SQLite migrations live in `crates/kd6-sqlite/migrations/`.

## Architecture

### OMS Conformance Levels

The spec defines three conformance levels. All three are implemented:

- **Level 1 (Core):** Store CRUD, memory entry CRUD, vector search (cosine
  similarity), keyword search (FTS5), tenant isolation, capabilities discovery.
- **Level 2 (Standard):** Multi-layer memory, audit logging with hash chain,
  TTL/lifecycle management, batch operations, hierarchical scoping (8 levels),
  inheritance/bubble-up, shared memory spaces.
- **Level 3 (Advanced):** Graph memory with typed edges and BFS traversal,
  temporal metadata (valid_from, valid_until, confidence), GDPR purge with
  audit anonymization, cryptographic audit trails (SHA-256), data sovereignty
  configuration.

### Memory Layers

Every memory store supports five layers:

| Layer | Purpose | Typical TTL |
|-------|---------|-------------|
| `working` | Active task scratchpad, current reasoning context | Minutes to hours |
| `episodic` | Specific interaction records, event logs | Days to months |
| `semantic` | Extracted facts, entities, relationships | Months to years |
| `procedural` | Learned patterns, skills, preferences | Long-lived |
| `archival` | Compressed, summarized historical context | Indefinite |

### Memory Scope Hierarchy

Scopes control visibility via a strict hierarchy:

```
tenant > org > team > project > user > agent > session > run
```

A memory entry is visible to any agent whose scope is equal to or more specific
than the entry's scope. `tenant_id` is the hard isolation boundary.

### Access Control Policies

- `private` -- visible only to the owning agent
- `inherit` -- visible to child agents spawned by the owner
- `shared` -- visible to agents matching `allowed_agents` or `allowed_scopes`
- `public_read` -- readable by all agents within the scope

### Key Implementation Patterns

- **Transaction discipline:** All write operations wrap data mutation and audit
  log entry in a single `BEGIN IMMEDIATE` transaction.
- **Optimistic concurrency:** Memory entries carry a `version` counter;
  updates use `WHERE version = ?` and return `Conflict` on mismatch.
- **Scope normalization:** `MemoryScope.normalize(tenant_id)` forces the
  authenticated tenant on all write paths.
- **Custom extractors:** `JsonBody<T>` and `PathId<T>` return JSON error
  responses on parse failures (not plain text).
- **FTS5 sanitization:** Keyword search inputs are stripped of FTS5 operators
  before reaching the search engine.

### API Surface

REST API rooted under `/v1/stores/{store_id}/`:

- **Health/capabilities:** `GET /health`, `GET /capabilities`
- **Store management:** CRUD on `/v1/stores`
- **Memory CRUD:** `/memories` and `/memories/{memory_id}`
- **Search:** `POST /search` (keyword via FTS5, vector via cosine similarity)
- **Lifecycle:** `DELETE /expired`, `GET /lifecycle/stats`
- **Batch:** `POST /memories/batch`, `POST /memories/batch/delete`
- **Inheritance:** `POST /inherit`, `DELETE /inherit/{id}`, `POST /bubble-up`
- **Shared spaces:** CRUD + join/leave on `/shared-spaces`
- **Graph:** `POST /graph/edges`, `DELETE /graph/edges/{id}`, `POST /graph/traverse`
- **Audit:** `GET /audit`, `GET /memories/{id}/audit`
- **GDPR:** `POST /gdpr/purge`

### MCP Server

The MCP server (kd6-mcp) exposes 10 tools via the Model Context Protocol using
rmcp. It supports Streamable HTTP transport (default, port 8081) and stdio
transport. Tools: `create_store`, `list_stores`, `create_memory`, `get_memory`,
`search_memories`, `delete_memory`, `create_edge`, `traverse_graph`,
`gdpr_purge`, `store_stats`.

### Environment Variables

| Variable | Default | Used by |
|---|---|---|
| `KD6_DATABASE_URL` | `sqlite:kd6.db?mode=rwc` | Both servers |
| `LISTEN_ADDR` | `0.0.0.0:8080` | kd6-server |
| `KD6_MCP_TRANSPORT` | `http` | kd6-mcp |
| `KD6_MCP_ADDR` | `0.0.0.0:8081` | kd6-mcp |
| `RUST_LOG` | `info` | Both servers |

## Conventions

- Use `cargo clippy -- -D warnings` to ensure zero warnings before committing.
- Use `cargo fmt` for formatting; do not override rustfmt defaults unless
  configured in `rustfmt.toml`.
- Tenant isolation must be enforced at the data layer -- never allow cross-tenant
  access.
- All write operations must produce audit events within the same transaction.
- Memory entries are versioned; the `version` field increments on each update.
- Backend capabilities are declared, not assumed -- always check
  `ProviderCapabilities` before using optional features.
- Tests use in-memory SQLite (`sqlite::memory:`) and require no external setup.
