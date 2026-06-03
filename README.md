# KD6

*We can remember it for your agents.*

KD6 is an open memory service for agentic AI workloads. It gives agents a
structured, searchable, multi-tenant place to store and retrieve knowledge
across sessions, teams, and projects. Memories are organized into five layers
(working, episodic, semantic, procedural, archival) and linked through a
knowledge graph, with full-text search, vector similarity, scoped visibility,
audit logging, and GDPR compliance built in.

KD6 implements all three conformance levels of the
[Open Memory Service (OMS) specification](spec/oms-spec.md), a standard
interface for agent memory that decouples AI applications from their storage
backend.

## Why?

AI agents are stateless by default. Every conversation starts from zero. The
workarounds — stuffing context windows, appending to markdown files, writing
ad-hoc JSON — break down as agent systems grow. An agent team working on a
codebase for weeks needs real memory: searchable, scoped, layered, and shared
where appropriate.

KD6 provides that. It is a database purpose-built for agent knowledge, exposed
through both a REST API and the Model Context Protocol (MCP).

## Quick Start

```bash
# Build
cargo build --release

# Run the HTTP server (creates kd6.db automatically)
cargo run --release -p kd6-server

# Or run the MCP server (for agent frameworks that speak MCP)
cargo run --release -p kd6-mcp
```

Store a memory:

```bash
# Create a store (names are the primary identifier, not UUIDs)
curl -X POST http://localhost:8080/v1/stores \
  -H "Content-Type: application/json" \
  -H "X-Tenant-Id: my-team" \
  -d '{"name": "project-notes"}'

# Store a memory (use the store name in the URL)
curl -X POST http://localhost:8080/v1/stores/project-notes/memories \
  -H "Content-Type: application/json" \
  -H "X-Tenant-Id: my-team" \
  -d '{
    "layer": "semantic",
    "content": {"text": "Auth service uses bcrypt with cost 12"},
    "owner_agent_id": "code-reviewer",
    "scope": {},
    "tags": ["auth", "security"]
  }'
```

Search it back:

```bash
curl -X POST http://localhost:8080/v1/stores/project-notes/search \
  -H "Content-Type: application/json" \
  -H "X-Tenant-Id: my-team" \
  -d '{"query": "authentication", "keyword": true}'
```

## What It Does

### Memory Layers

Memories live in one of five layers, each with different retention
characteristics:

| Layer | Purpose | Example |
|---|---|---|
| **Working** | Scratch space for in-flight tasks | "Currently refactoring the auth module" |
| **Episodic** | Records of specific events | "PR #42 was merged with 3 approvals" |
| **Semantic** | Distilled facts and knowledge | "The API uses JWT with RS256 signing" |
| **Procedural** | Learned processes and patterns | "Deploy sequence: build, test, stage, promote" |
| **Archival** | Historical record | "Q1 2025 architecture review decisions" |

### Search

Two search modes, both available through a single endpoint:

- **Keyword search** powered by SQLite FTS5 with relevance ranking
- **Vector search** using cosine similarity over stored embeddings

Embeddings are computed automatically on write and query when an embedding
provider is configured (local or OpenAI-compatible).

### Knowledge Graph

Link memories with typed, weighted edges and traverse relationships using
breadth-first search. Build graphs like "this decision depends on that
requirement, which relates to this design doc."

### Multi-Tenancy and Scoping

Hard tenant isolation on every query. Within a tenant, memories are scoped
across eight hierarchical levels:

```
tenant > org > team > project > user > agent > session > run
```

An agent only sees memories at its scope level or broader.

### Compliance

- **Audit logging** with SHA-256 hash chain for tamper detection
- **GDPR purge** removes data and anonymizes audit entries in one transaction
- **Data sovereignty** configuration per store
- **Optimistic concurrency** prevents lost updates
- **Immutable memories** for records that must never change

### MCP Integration

Ten tools exposed over the Model Context Protocol for direct agent use:

`create_store`, `list_stores`, `create_memory`, `get_memory`,
`search_memories`, `delete_memory`, `create_edge`, `traverse_graph`,
`store_stats`, `gdpr_purge`

## Architecture

Five Rust crates in a Cargo workspace:

```
kd6-server (HTTP API)     kd6-mcp (MCP server)
         \                      /
          \                    /
           kd6-sqlite (backend)
                  |
             kd6-core (domain types + SPI trait)
                  |
             kd6-embed (embedding providers)
```

**kd6-core** defines the `OmsProvider` trait (the SPI) and all domain types.
No I/O, no dependencies on storage or transport. Any backend implements this
trait.

**kd6-embed** provides pluggable embedding providers: a local provider using
fastembed-rs (all-MiniLM-L6-v2, 384 dimensions) and an OpenAI-compatible HTTP
provider for hosted models.

**kd6-sqlite** implements `OmsProvider` using sqlx with async connection
pooling, WAL mode, FTS5, and six migration files. The implementation is split
across ten focused modules.

**kd6-server** wraps the provider in an axum HTTP API with 30 routes,
custom extractors for JSON error responses, and tenant header validation.

**kd6-mcp** wraps the provider in an MCP server using the rmcp crate,
exposing ten tools over Streamable HTTP (default) or stdio.

Swapping the backend (to Postgres, DynamoDB, or anything else) means
implementing the `OmsProvider` trait. The HTTP and MCP servers work unchanged.

## Configuration

| Variable | Default | Description |
|---|---|---|
| `KD6_DATABASE_URL` | `sqlite:kd6.db?mode=rwc` | SQLite connection string |
| `LISTEN_ADDR` | `0.0.0.0:8080` | HTTP server bind address |
| `KD6_MCP_TRANSPORT` | `http` | MCP transport: `http` or `stdio` |
| `KD6_MCP_ADDR` | `0.0.0.0:8081` | MCP HTTP server bind address |
| `KD6_EMBEDDING_PROVIDER` | `local` | Embedding provider: `local`, `openai-compatible`, or `none` |
| `KD6_EMBEDDING_ENDPOINT` | — | API endpoint (required for `openai-compatible`) |
| `KD6_EMBEDDING_MODEL` | — | Model name (required for `openai-compatible`) |
| `KD6_EMBEDDING_API_KEY` | — | API key (optional, for authenticated endpoints) |
| `KD6_EMBEDDING_DIMENSIONS` | — | Override embedding dimensions (optional) |
| `RUST_LOG` | `info` | Tracing log level |

## OMS Conformance

KD6 implements all three OMS specification levels:

| Level | Name | Features | Status |
|---|---|---|---|
| 1 | Core | Stores, memories, search, tenant isolation | ✅ Complete |
| 2 | Standard | Audit, TTL, batching, scoping, inheritance, shared spaces | ✅ Complete |
| 3 | Advanced | Graph memory, temporal metadata, GDPR, crypto audit, sovereignty | ✅ Complete |

166 tests across five crates. Zero clippy warnings.

## Documentation

Detailed documentation lives in the [`docs/`](docs/) directory:

- **[Architecture](docs/architecture.md)** — crate structure, data flow,
  design decisions
- **[Usage Guide](docs/usage.md)** — building, running, API walkthrough with
  curl examples
- **[Features](docs/features.md)** — complete reference for all capabilities
- **[OMS Specification](docs/specification.md)** — the spec, conformance
  levels, and memory model
- **[Contributing](docs/contributing.md)** — development workflow, testing,
  and coding conventions

## Examples

The [`examples/`](examples/) directory contains integration examples:

- **[squad-memory](examples/squad-memory/)** — Multi-agent sprint simulation
  using [Squad](https://github.com/bradygaster/squad) with KD6 as the memory
  provider. Demonstrates memory layers, scoped visibility, knowledge graph,
  and cross-session recall across three simulated development sessions.

## Development

```bash
# Full CI pipeline (same as GitHub Actions)
make ci

# Individual steps
cargo fmt -- --check                 # Format check
cargo clippy --all-targets -- -D warnings  # Lint (zero warnings policy)
cargo build --all-targets            # Build
cargo test                           # Run all 166 tests
```

Tests use in-memory SQLite and need no external setup.

## License

MIT
