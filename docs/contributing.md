# Contributing

## Development Setup

### Prerequisites

- Rust 1.75+ (2021 edition)
- SQLite 3.35+ (included in most OS distributions)
- Make (for the CI pipeline)

### Building

```bash
git clone https://github.com/devigned/kd6.git
cd kd6
cargo build
```

### Running the CI Pipeline

The Makefile replicates the GitHub Actions CI pipeline locally:

```bash
make ci
```

This runs, in order:

1. `cargo fmt -- --check` — format check
2. `cargo clippy --all-targets -- -D warnings` — lint (zero warnings)
3. `cargo build --all-targets` — build all crates and tests
4. `cargo test` — run all 166 tests

Always run `make ci` before pushing. If CI passes locally, it will pass in
GitHub Actions.

### Running Individual Steps

```bash
cargo test                           # All tests
cargo test -p kd6-sqlite             # Tests for one crate
cargo test test_create_memory        # Single test by name
cargo clippy --all-targets -- -D warnings  # Lint
cargo fmt                            # Auto-format
```

### Running Servers Locally

```bash
# HTTP API server
cargo run -p kd6-server

# MCP server (Streamable HTTP)
cargo run -p kd6-mcp

# MCP server (stdio mode)
KD6_MCP_TRANSPORT=stdio cargo run -p kd6-mcp
```

## Coding Conventions

### Zero Warnings Policy

Every commit must pass `cargo clippy -- -D warnings` with no warnings. Clippy
is the first gate in CI — if it fails, nothing else runs.

### Transaction Discipline

All write operations must wrap data mutation **and** audit log entry in a
single `BEGIN IMMEDIATE` transaction. If any step fails, the entire operation
rolls back. This is enforced throughout kd6-sqlite — see any module
(stores.rs, memories.rs, graph.rs, etc.) for the pattern:

```rust
let mut tx = pool.begin().await.map_err(|e| map_db_error("begin transaction", e))?;
// ... do work on &mut *tx ...
crate::audit::log_audit_on_conn(&mut *tx, store_id, tenant_id, ...).await?;
tx.commit().await.map_err(|e| map_db_error("commit transaction", e))?;
```

### Tenant Isolation

Never allow cross-tenant data access. Every query must include a
`WHERE tenant_id = ?` clause. All write paths must call
`MemoryScope::normalize(tenant_id)` to override the scope's tenant field with
the authenticated value.

### Error Handling

- Use `OmsError` variants from kd6-core for all error returns
- Use `map_db_error()` from kd6-sqlite helpers to convert sqlx errors —
  it detects constraint violations automatically
- Row-parsing errors (corrupted data) should use `OmsError::Internal`
- Never expose raw database errors to API callers

### Store Names, Not UUIDs

All store-scoped API paths use human-readable store names, not UUIDs. Store
names are immutable after creation and unique per tenant. The `OmsProvider`
trait internally uses UUIDs — the server/MCP layer resolves names via
`get_store_by_name()`.

### Testing

- Tests use in-memory SQLite (`sqlite::memory:`) — no external setup needed
- Integration tests live alongside their crate in `tests/integration.rs`
- Unit tests live in the module they test (e.g., `provider.rs` has a `tests` mod)
- When adding a new feature, add tests that cover both the happy path and
  error cases (not found, wrong tenant, constraint violations)

## Crate Structure

```
crates/
├── kd6-core/      # Domain types, OmsError, OmsProvider trait, EmbeddingProvider trait
├── kd6-embed/     # Embedding providers (local fastembed, OpenAI-compatible HTTP)
├── kd6-sqlite/    # SQLite backend (10 focused modules + provider.rs)
├── kd6-server/    # Axum HTTP server with 30 routes
└── kd6-mcp/       # MCP server (rmcp) with 10 tools
```

### Adding a New Backend

To add a new storage backend (e.g., Postgres):

1. Create a new crate (e.g., `kd6-postgres`)
2. Implement the `OmsProvider` trait from kd6-core
3. Wire it into kd6-server and kd6-mcp as an alternative to SqliteProvider

The `OmsProvider` trait has 31 methods. Start with Level 1 (store CRUD, memory
CRUD, search, capabilities) and expand from there. The trait provides default
implementations that return `NotImplemented` for Level 2 and Level 3 methods.

### Provider Module Organization

The kd6-sqlite provider is split across ten focused modules:

| Module | Responsibility |
|---|---|
| `helpers.rs` | Parse helpers, embedding conversion, cosine similarity, map_db_error |
| `stores.rs` | Store CRUD, get_store_by_name, get_or_create_store |
| `memories.rs` | Memory CRUD, row parsing, upsert logic |
| `search.rs` | FTS5 keyword search, vector similarity search |
| `audit.rs` | Audit log writing and SHA-256 hash chain |
| `graph.rs` | Graph edge CRUD and BFS traversal |
| `shared_spaces.rs` | Shared space lifecycle and participant management |
| `inheritance.rs` | Memory inheritance rules and bubble-up |
| `lifecycle.rs` | TTL purge, batch operations, store stats |
| `gdpr.rs` | GDPR purge with audit anonymization |

Each module exports `pub(crate)` free functions that accept `&SqlitePool`.
The `provider.rs` file contains the `SqliteProvider` struct, constructor,
thin `OmsProvider` trait delegation, and all tests.

## Commit Guidelines

- Write clear commit messages that tell a progressive story
- No "fix" commits — squash or rewrite history to maintain a clean narrative
- Each commit should be a logical, self-contained change
- Run `make ci` before every push to ensure nothing is broken
