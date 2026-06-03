# Usage Guide

## Prerequisites

- Rust 1.75 or later
- SQLite 3.35+ with FTS5 support available on your system
- `curl` for the HTTP examples
- An MCP-capable client if you want to use KD6 through MCP

## Building

```bash
git clone https://github.com/devigned/kd6.git
cd kd6

# Primary validation command: format check, clippy, build, and tests
make ci

# Build release binaries
cargo build --release
```

This produces the server binaries in `target/release/`:

- `kd6-server` — HTTP API server
- `kd6-mcp` — MCP server

## Running HTTP Server

```bash
# Start with defaults
cargo run --release -p kd6-server

# Custom database and listen address
KD6_DATABASE_URL="sqlite:data/kd6.db?mode=rwc" \
LISTEN_ADDR="127.0.0.1:3000" \
RUST_LOG="info" \
cargo run --release -p kd6-server
```

Database migrations run automatically at startup.

### Environment variables

| Variable | Default | Description |
|---|---|---|
| `KD6_DATABASE_URL` | `sqlite:kd6.db?mode=rwc` | SQLite connection URL |
| `LISTEN_ADDR` | `0.0.0.0:8080` | HTTP server address |
| `KD6_MCP_TRANSPORT` | `http` | MCP transport: `http` or `stdio` |
| `KD6_MCP_ADDR` | `0.0.0.0:8081` | MCP HTTP server address |
| `KD6_EMBEDDING_PROVIDER` | `local` | Embedding provider |
| `KD6_EMBEDDING_ENDPOINT` | — | OpenAI-compatible endpoint |
| `KD6_EMBEDDING_MODEL` | — | Model name |
| `KD6_EMBEDDING_API_KEY` | — | API key |
| `KD6_EMBEDDING_DIMENSIONS` | — | Dimension override |
| `RUST_LOG` | `info` | Log level |

### Embedding providers

KD6 can embed memory content and search queries automatically.

- `KD6_EMBEDDING_PROVIDER=local` — default. Uses `fastembed-rs` with `all-MiniLM-L6-v2` and 384-dimensional vectors.
- `KD6_EMBEDDING_PROVIDER=openai-compatible` — requires `KD6_EMBEDDING_ENDPOINT` and `KD6_EMBEDDING_MODEL`; optionally accepts `KD6_EMBEDDING_API_KEY` and `KD6_EMBEDDING_DIMENSIONS`.
- `KD6_EMBEDDING_PROVIDER=none` — disables auto-embedding. Callers must provide `embedding` values on writes and vector searches.

Examples:

```bash
# Local embeddings (default)
KD6_EMBEDDING_PROVIDER=local cargo run --release -p kd6-server

# OpenAI-compatible embeddings
KD6_EMBEDDING_PROVIDER=openai-compatible \
KD6_EMBEDDING_ENDPOINT="https://api.openai.com/v1/embeddings" \
KD6_EMBEDDING_MODEL="text-embedding-3-small" \
KD6_EMBEDDING_API_KEY="your-api-key" \
cargo run --release -p kd6-server

# No auto-embedding
KD6_EMBEDDING_PROVIDER=none cargo run --release -p kd6-server
```

### Auto-provisioning and defaults

KD6 supports a zero-setup path for development and simple local use:

- If `X-Tenant-Id` is absent, KD6 uses the `_default` tenant.
- If you write to `/v1/stores/_default/...`, KD6 auto-creates the `_default` store on first write.
- Store names are chosen when a store is created, are immutable, and are the primary identifier in all store-scoped URLs.

If you want strict behavior, you can disable these conveniences with `KD6_DEFAULT_TENANT=false` and `KD6_AUTO_PROVISION=false`.

## Running MCP Server

KD6 MCP supports Streamable HTTP and stdio transports.

### HTTP mode

```bash
cargo run --release -p kd6-mcp
```

This starts the MCP endpoint at `http://localhost:8081/mcp`.

```bash
KD6_MCP_ADDR="127.0.0.1:9090" cargo run --release -p kd6-mcp
```

### Stdio mode

```bash
KD6_MCP_TRANSPORT=stdio cargo run --release -p kd6-mcp
```

Logs are written to stderr so stdout remains clean for MCP traffic.

## MCP Client Config

### Claude Desktop over HTTP

```json
{
  "mcpServers": {
    "kd6": {
      "url": "http://localhost:8081/mcp"
    }
  }
}
```

### Claude Desktop over stdio

```json
{
  "mcpServers": {
    "kd6": {
      "command": "/absolute/path/to/kd6-mcp",
      "env": {
        "KD6_DATABASE_URL": "sqlite:/absolute/path/to/kd6.db?mode=rwc",
        "KD6_MCP_TRANSPORT": "stdio",
        "KD6_EMBEDDING_PROVIDER": "local"
      }
    }
  }
}
```

## HTTP API Walkthrough

### Routing and headers

All store-scoped endpoints use a store name in the path, not a UUID.

- Old style: `/v1/stores/{store_id}/memories`
- Current style: `/v1/stores/my-store/memories`

Request headers:

- `X-Tenant-Id` — optional. If omitted, KD6 uses `_default`.
- `X-Agent-Id` — optional, used for audit attribution.

### Health check

```bash
curl http://localhost:8080/health
```

### Capabilities

```bash
curl http://localhost:8080/capabilities
```

### Zero-setup write with `_default`

This works even with no prior store creation when auto-provisioning is enabled:

```bash
curl -X POST http://localhost:8080/v1/stores/_default/memories \
  -H "Content-Type: application/json" \
  -d '{
    "layer": "working",
    "content": {"text": "Local scratch memory"},
    "owner_agent_id": "demo-agent",
    "scope": {}
  }'
```

### Create a named store

```bash
curl -X POST http://localhost:8080/v1/stores \
  -H "Content-Type: application/json" \
  -H "X-Tenant-Id: acme-corp" \
  -d '{
    "name": "project-alpha",
    "config": {
      "default_ttl_seconds": 86400
    }
  }'
```

Use `project-alpha` directly in all subsequent URLs.

### Create a memory

```bash
curl -X POST http://localhost:8080/v1/stores/project-alpha/memories \
  -H "Content-Type: application/json" \
  -H "X-Tenant-Id: acme-corp" \
  -H "X-Agent-Id: code-reviewer" \
  -d '{
    "layer": "semantic",
    "content": {"text": "The auth module uses bcrypt with cost factor 12"},
    "owner_agent_id": "code-reviewer",
    "scope": {},
    "tags": ["auth", "security"],
    "upsert_key": "fact:auth:bcrypt"
  }'
```

`upsert_key` enables atomic create-or-replace within the same store, layer, and scope.

When auto-embedding is enabled, KD6 computes embeddings from `content` if `embedding` is omitted.

### Get a memory

```bash
curl http://localhost:8080/v1/stores/project-alpha/memories/{memory_id} \
  -H "X-Tenant-Id: acme-corp"
```

### Search memories

The request field is `query`, not `text`. Use `top_k` to limit results.

#### Keyword search

```bash
curl -X POST http://localhost:8080/v1/stores/project-alpha/search \
  -H "Content-Type: application/json" \
  -H "X-Tenant-Id: acme-corp" \
  -d '{
    "query": "authentication bcrypt",
    "keyword": true,
    "top_k": 10
  }'
```

#### Vector search

```bash
curl -X POST http://localhost:8080/v1/stores/project-alpha/search \
  -H "Content-Type: application/json" \
  -H "X-Tenant-Id: acme-corp" \
  -d '{
    "query": "password hashing approach",
    "embedding": [0.1, 0.2, 0.3],
    "keyword": false,
    "top_k": 5
  }'
```

If `embedding` is omitted and the embedding provider is `local` or `openai-compatible`, KD6 embeds the `query` automatically. If `KD6_EMBEDDING_PROVIDER=none`, callers must provide the query embedding themselves.

### Update a memory

```bash
curl -X PATCH http://localhost:8080/v1/stores/project-alpha/memories/{memory_id} \
  -H "Content-Type: application/json" \
  -H "X-Tenant-Id: acme-corp" \
  -d '{
    "tags": ["auth", "security", "password-hashing"],
    "expires_at": null
  }'
```

### Delete a memory

```bash
curl -X DELETE http://localhost:8080/v1/stores/project-alpha/memories/{memory_id} \
  -H "X-Tenant-Id: acme-corp"
```

### Batch create

```bash
curl -X POST http://localhost:8080/v1/stores/project-alpha/memories/batch \
  -H "Content-Type: application/json" \
  -H "X-Tenant-Id: acme-corp" \
  -d '{
    "entries": [
      {
        "layer": "episodic",
        "content": {"text": "Meeting discussed API redesign"},
        "owner_agent_id": "note-taker",
        "scope": {}
      },
      {
        "layer": "episodic",
        "content": {"text": "Decision: switch to REST from GraphQL"},
        "owner_agent_id": "note-taker",
        "scope": {}
      }
    ]
  }'
```

### Graph operations

```bash
# Create an edge
curl -X POST http://localhost:8080/v1/stores/project-alpha/graph/edges \
  -H "Content-Type: application/json" \
  -H "X-Tenant-Id: acme-corp" \
  -d '{
    "source_memory_id": "11111111-1111-1111-1111-111111111111",
    "target_memory_id": "22222222-2222-2222-2222-222222222222",
    "relation_type": "depends_on",
    "weight": 1.0
  }'

# Traverse the graph
curl -X POST http://localhost:8080/v1/stores/project-alpha/graph/traverse \
  -H "Content-Type: application/json" \
  -H "X-Tenant-Id: acme-corp" \
  -d '{
    "start_memory_id": "11111111-1111-1111-1111-111111111111",
    "depth": 3,
    "relation_types": ["depends_on", "related_to"]
  }'
```

### Store statistics

```bash
curl http://localhost:8080/v1/stores/project-alpha/lifecycle/stats \
  -H "X-Tenant-Id: acme-corp"
```

### Audit log

```bash
curl "http://localhost:8080/v1/stores/project-alpha/audit?limit=20" \
  -H "X-Tenant-Id: acme-corp"

curl "http://localhost:8080/v1/stores/project-alpha/memories/{memory_id}/audit" \
  -H "X-Tenant-Id: acme-corp"
```

### GDPR purge

```bash
curl -X POST http://localhost:8080/v1/stores/project-alpha/gdpr/purge \
  -H "Content-Type: application/json" \
  -H "X-Tenant-Id: acme-corp" \
  -d '{
    "user_id": "user-42"
  }'
```

The purge request must include at least one scope field beyond `tenant_id`.

## MCP Tools Reference

KD6 MCP exposes 10 tools:

| Tool | Description |
|---|---|
| `create_store` | Create a store by name for a tenant |
| `list_stores` | List stores visible to a tenant |
| `create_memory` | Create a memory in a named store, with optional `upsert_key` |
| `get_memory` | Fetch a memory by ID from a named store |
| `search_memories` | Search a named store with `query`, optional `embedding`, and `top_k` |
| `delete_memory` | Delete a memory by ID from a named store |
| `create_edge` | Create a graph edge between two memories |
| `traverse_graph` | Traverse the memory graph from a starting memory |
| `store_stats` | Return lifecycle and storage statistics for a named store |
| `gdpr_purge` | Purge scoped data and anonymize related audit entries |

MCP requests identify stores by `store_name`, not UUID. Tool calls include `tenant_id` for isolation.

## Development

KD6 currently has 166 tests across 5 crates.

```bash
# Primary local validation command
make ci

# Individual commands
cargo fmt -- --check
cargo clippy --all-targets -- -D warnings
cargo build --all-targets
cargo test
```

Tests use in-memory SQLite and require no external setup.
