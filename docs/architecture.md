# Architecture

KD6 is a Cargo workspace with five crates. The public entry points are an Axum HTTP API and an rmcp-based MCP server. Both frontends share the same domain model, the same embedding abstractions, and the same SQLite-backed `OmsProvider` implementation.

A useful mental model is:

```
                      kd6-server (HTTP API)
                    /    |               \
                   /     |                \
                  /      |                 \
        kd6-mcp (MCP)  kd6-sqlite       kd6-embed
               \          |               /
                \         |              /
                 \--------+-------------/
                          |
                       kd6-core
```

Actual Cargo dependencies are:

- `kd6-core` — no internal KD6 dependencies
- `kd6-embed` → `kd6-core`
- `kd6-sqlite` → `kd6-core`
- `kd6-server` → `kd6-core`, `kd6-embed`, `kd6-sqlite`
- `kd6-mcp` → `kd6-core`, `kd6-embed`, `kd6-sqlite`

## Crate Overview

### kd6-core

`kd6-core` is the shared contract layer. It is pure Rust domain logic — no database, HTTP, or MCP transport code.

It contains:

- **`OmsProvider`** — the backend SPI. It defines **31 async methods** plus a synchronous `capabilities()` method.
- **Domain types** — `MemoryEntry`, `MemoryStore`, `MemoryScope`, `GraphEdge`, `SharedSpace`, audit models, batch request/response types, and the OMS request/response surface.
- **`OmsError`** — currently **14 variants**, including `ConstraintViolation` for database-level uniqueness and constraint failures.
- **Embedding abstractions** — the `EmbeddingProvider` trait, `NoopEmbedder`, and shared `auto_embed_content`, `auto_embed_query`, and `auto_embed_update` helpers used by both the HTTP and MCP layers.

Because `kd6-core` stays transport- and storage-agnostic, new backends can implement `OmsProvider` without pulling in Axum, rmcp, or sqlx.

### kd6-embed

`kd6-embed` provides concrete embedding implementations for the `EmbeddingProvider` trait defined in `kd6-core`.

It currently ships two providers:

- **`LocalEmbedder`** — in-process embeddings via `fastembed`/ONNX. The default model is **all-MiniLM-L6-v2** with **384 dimensions**.
- **`OpenAiCompatibleEmbedder`** — an HTTP client for any OpenAI-compatible `/embeddings` endpoint. It validates response ordering and dimensionality before returning vectors.

This crate depends on `kd6-core`, not the other way around.

### kd6-sqlite

`kd6-sqlite` is the only crate that talks to SQLite. It implements `OmsProvider` with `sqlx`, configures SQLite in WAL mode with foreign keys enabled, and applies **6 migration files** at startup.

Implementation is split into focused modules:

- `helpers.rs`
- `stores.rs`
- `memories.rs`
- `search.rs`
- `audit.rs`
- `graph.rs`
- `shared_spaces.rs`
- `inheritance.rs`
- `lifecycle.rs`
- `gdpr.rs`

`provider.rs` is intentionally thin — it delegates `OmsProvider` calls into those modules and also houses the crate’s test suite. `lib.rs` only wires modules together and re-exports `SqliteProvider`.

### kd6-server

`kd6-server` is the Axum REST API. It exposes **30 `/v1` route registrations**, plus root-level `/health` and `/capabilities` endpoints.

Key properties:

- **Store-scoped routing uses names, not UUIDs** — for example, `/v1/stores/my-store/memories`.
- **Custom extractors** — `TenantId`, `JsonBody<T>`, and `PathId<T>` return JSON-shaped errors instead of Axum’s default plain-text rejections.
- **No `AgentId` extractor** — that code was removed; `TenantId` is the only custom header extractor.
- **Store resolution** — the server parses a `StoreRef` from the path, then resolves it to the provider’s internal UUID with `get_store_by_name()` or `_default` auto-provisioning.
- **Shared state** — `AppState` holds `Arc<dyn OmsProvider>`, `Arc<dyn EmbeddingProvider>`, and the server config flags that control default tenant and default store behavior.

### kd6-mcp

`kd6-mcp` is the MCP server built on `rmcp`. It resolves store names to UUIDs before calling the provider, just like the HTTP layer.

It exposes **10 tools**:

- `create_store`
- `list_stores`
- `create_memory`
- `get_memory`
- `search_memories`
- `delete_memory`
- `create_edge`
- `traverse_graph`
- `gdpr_purge`
- `store_stats`

Supported transports:

- **Streamable HTTP** — default, bound to port `8081`, served at `/mcp`
- **stdio** — for local MCP clients that launch the server as a subprocess

## Provider Module Split

The SQLite provider is deliberately organized by feature area rather than as one monolith.

- `stores.rs` — store CRUD, lookup by name, `_default` store provisioning helpers
- `memories.rs` — memory CRUD, batch create/delete, optimistic concurrency, upsert behavior
- `search.rs` — vector and keyword search orchestration
- `audit.rs` — audit append/query logic and hash-chain support
- `graph.rs` — edge creation, deletion, traversal
- `shared_spaces.rs` — shared-space CRUD and participant membership
- `inheritance.rs` — inheritance specs and bubble-up
- `lifecycle.rs` — expiry purge and lifecycle statistics
- `gdpr.rs` — scoped hard-delete and audit anonymization
- `helpers.rs` — SQLite error mapping, embedding blob encoding, FTS5 sanitization, cosine math, and shared query helpers

This split keeps SQL close to the OMS feature it implements, while `provider.rs` stays a narrow façade over the module set.

## Embedding Architecture

Embedding support spans three crates:

- **`kd6-core`** defines the interface and the shared auto-embedding helpers.
- **`kd6-embed`** implements the concrete providers.
- **`kd6-server` and `kd6-mcp`** select a provider at startup and call the same helper functions on create, update, batch-create, and search paths.

Runtime selection is controlled by `KD6_EMBEDDING_PROVIDER`:

- `local` — `LocalEmbedder`
- `openai-compatible` — `OpenAiCompatibleEmbedder`
- `none` — `NoopEmbedder`, which disables automatic embedding and requires callers to provide embeddings explicitly for vector search

Embeddings are stored in SQLite as raw `f32` vectors encoded into little-endian byte blobs, not JSON arrays.

## Multi-Tenancy and Store Routing

Tenant isolation is the outermost boundary. Every data query is scoped by `tenant_id`, and `MemoryScope.normalize(tenant_id)` overwrites any caller-supplied tenant on write paths.

The scope hierarchy is:

```
tenant > org > team > project > user > agent > session > run
```

Access policy is represented directly in the domain model:

- `private`
- `inherit`
- `shared`
- `public_read`

### Store name routing

All store-scoped API paths use **human-readable store names**, not UUIDs.

- REST example — `/v1/stores/my-store/memories`
- MCP input — `store_name: "my-store"`

Internally, `OmsProvider` still uses store UUIDs. The HTTP and MCP layers resolve names through `get_store_by_name()` before invoking provider methods.

Store names are:

- **immutable after creation**
- **unique per tenant**
- enforced by a SQLite unique index on **`stores(tenant_id, name)`**

### Default tenant and default store

The server has two zero-setup conveniences, both configurable:

- **Default tenant fallback** — when the `X-Tenant-ID` header is absent and default-tenant mode is enabled, `TenantId` resolves to `_default`.
- **Default store alias** — when the store name is `_default` and auto-provisioning is enabled, the server creates or reuses that store on first use.

The feature flags live in `ServerConfig` and are controlled at runtime by `KD6_DEFAULT_TENANT` and `KD6_AUTO_PROVISION`.

## Memory Layers

KD6 implements the OMS five-layer memory model:

| Layer | Purpose | Typical TTL |
|---|---|---|
| `working` | Active scratchpad and short-lived task context | Minutes to hours |
| `episodic` | Interaction records, observations, event history | Days to months |
| `semantic` | Extracted facts, entities, and relationships | Months to years |
| `procedural` | Learned patterns, workflows, and preferences | Long-lived |
| `archival` | Compressed or long-retention historical context | Indefinite |

These layers are first-class enum values in `MemoryLayer` and flow through CRUD, search, inheritance, and lifecycle APIs.

## Concurrency and Consistency

KD6 relies on a small number of explicit consistency mechanisms:

- **WAL mode** — readers can proceed concurrently while SQLite serializes writers.
- **Optimistic concurrency for updates** — `MemoryEntry.version` increments on update, and the explicit update path uses `WHERE ... version = ?`; if no row matches, the provider returns `OmsError::Conflict`.
- **Tenant normalization** — write paths normalize `scope.tenant_id` to the authenticated tenant before persistence.
- **Immediate write locking** — the major audited mutation paths begin with `BEGIN IMMEDIATE` so the write lock is acquired up front rather than halfway through the operation.

Most mutating modules follow the same pattern: mutate data, write the audit row, then `COMMIT`. The notable special case is `_default` store auto-provisioning, which uses `INSERT OR IGNORE` against the unique `(tenant_id, name)` index to stay race-safe.

## Search Architecture

KD6 supports two search modes that can be combined in one request.

### Vector search

Vector search is a brute-force scan over candidate memories with embeddings:

- embeddings are stored as little-endian `f32` byte blobs
- query-time similarity is **cosine similarity**
- candidate rows are capped at **10,000** per query
- scores are merged with keyword results by memory ID, keeping the higher score

This is intentionally simple and portable — good enough for a reference implementation, but not a specialized ANN index.

### Keyword search

Keyword search uses SQLite **FTS5**.

Before the query reaches `MATCH`, KD6 sanitizes user input in `sanitize_fts5_query()` by tokenizing whitespace and quoting each token. That strips FTS5 operators from user-controlled input and avoids syntax injection.

## Error Handling

`OmsError` is the single error vocabulary shared across the workspace. The SQLite adapter converts low-level `sqlx::Error` values with `map_db_error()`.

Important details:

- SQLite constraint codes **`2067`**, **`1555`**, and **`19`** are recognized as constraint violations.
- Those map to **`OmsError::ConstraintViolation`**.
- The HTTP layer converts `ConstraintViolation` into **HTTP 409 Conflict**.
- Non-constraint database failures fall back to `OmsError::Internal`.

This keeps uniqueness failures — especially duplicate store names and other indexed conflicts — distinct from generic internal errors.

## Audit Trail

Audit logging is part of the architecture, not an optional afterthought.

For the main audited mutation paths, KD6 writes the domain change and the audit entry in the same transaction. That pattern is implemented across `stores.rs`, `memories.rs`, `inheritance.rs`, `shared_spaces.rs`, `lifecycle.rs`, `gdpr.rs`, and the audited graph write path.

The audit subsystem also supports:

- **hash chaining** — each audit row records a SHA-256 link to the previous row for tamper evidence
- **query APIs** — store-wide and per-memory audit history
- **GDPR anonymization** — purge keeps structural audit history while clearing identifying details from affected audit rows

## Shared Spaces

Shared spaces are multi-agent collaboration surfaces layered on top of a store.

A notable implementation detail is the `list_shared_spaces()` hydration strategy: it avoids an N+1 participant query pattern by loading all spaces first, then fetching participants in a single batch query and grouping them with a `HashMap` keyed by space ID.

## Graph Memory

Graph memory is Level 3 functionality layered on top of flat memories.

- edges are stored separately from memory rows
- edges are typed with arbitrary `relation_type` strings and weighted with `weight`
- traversal is breadth-first search from a starting memory

Traversal guards are explicit:

- maximum depth — `10`
- maximum visited nodes — `1,000`
- maximum edges loaded per node — `500`

Neighbor nodes discovered during traversal are batch-fetched rather than loaded one-by-one.

## Design Decisions

**Why store names in URLs instead of UUIDs?** Human-readable store names make the API easier to use directly while preserving UUID-based internal references in the provider layer.

**Why keep embedding logic out of `kd6-sqlite`?** Embedding generation is an application concern, not a storage concern. The provider stores vectors; the server and MCP layers decide when and how to compute them.

**Why brute-force vector search?** SQLite has no built-in ANN index. A 10,000-row cosine scan keeps the reference implementation portable and easy to reason about.

**Why FTS5?** It is built into SQLite, fast enough for the reference implementation, and integrates cleanly with the rest of the schema.

**Why separate HTTP and MCP binaries?** They serve different clients, but both stay behaviorally aligned because they share the same domain types, embedding helpers, and provider implementation.
