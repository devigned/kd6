# Usage Guide

## Prerequisites

- Rust 1.75 or later (2021 edition)
- SQLite 3.35+ (for FTS5 and other features; included in most OS distributions)

## Building from Source

```bash
git clone https://github.com/devigned/kd6.git
cd kd6
cargo build --release
```

This produces two binaries in `target/release/`:

- `kd6-server` -- HTTP API server
- `kd6-mcp` -- MCP server (Streamable HTTP by default, stdio optional)

## Running the HTTP Server

```bash
# Start with defaults (SQLite file at ./kd6.db, listen on 0.0.0.0:8080)
cargo run --release --bin kd6-server

# Or with custom configuration
KD6_DATABASE_URL="sqlite:data/memories.db?mode=rwc" \
LISTEN_ADDR="127.0.0.1:3000" \
RUST_LOG="info" \
cargo run --release --bin kd6-server
```

### Environment Variables

| Variable | Default | Description |
|---|---|---|
| `KD6_DATABASE_URL` | `sqlite:kd6.db?mode=rwc` | SQLite connection URL. The `?mode=rwc` flag creates the file if it does not exist. |
| `LISTEN_ADDR` | `0.0.0.0:8080` | Address and port for the HTTP server. |
| `KD6_MCP_TRANSPORT` | `http` | MCP transport mode: `http` (Streamable HTTP) or `stdio`. |
| `KD6_MCP_ADDR` | `0.0.0.0:8081` | Address and port for the MCP HTTP server. |
| `RUST_LOG` | `info` | Log level filter. Supports `trace`, `debug`, `info`, `warn`, `error`. |

Database migrations run automatically on startup. No manual setup is needed.

## Running the MCP Server

The MCP server supports two transport modes: HTTP (default) and stdio.

### HTTP mode (default)

```bash
cargo run --release --bin kd6-mcp
```

This starts a Streamable HTTP MCP server on port 8081. Clients connect to
`http://localhost:8081/mcp`.

```bash
# Custom address
KD6_MCP_ADDR="127.0.0.1:9090" cargo run --release --bin kd6-mcp
```

### Stdio mode

For local agent frameworks that launch MCP servers as child processes:

```bash
KD6_MCP_TRANSPORT=stdio cargo run --release --bin kd6-mcp
```

Logs go to stderr to keep stdout clean for MCP protocol messages.

### MCP Client Configuration

To register KD6 as an MCP server in Claude Desktop using HTTP mode, add this to
your `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "kd6": {
      "url": "http://localhost:8081/mcp"
    }
  }
}
```

For stdio mode:

```json
{
  "mcpServers": {
    "kd6": {
      "command": "/path/to/kd6-mcp",
      "env": {
        "KD6_DATABASE_URL": "sqlite:/path/to/kd6.db?mode=rwc",
        "KD6_MCP_TRANSPORT": "stdio"
      }
    }
  }
}
```

## HTTP API Walkthrough

All API requests require a `X-Tenant-Id` header for tenant isolation. Many
endpoints also accept an `X-Agent-Id` header for audit attribution.

### Health Check

```bash
curl http://localhost:8080/health
```

```json
{ "status": "ok" }
```

### Provider Capabilities

```bash
curl http://localhost:8080/capabilities
```

Returns the feature set of the running backend, including supported layers,
search modes, and batch limits.

### Create a Store

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

### Create a Memory

```bash
curl -X POST http://localhost:8080/v1/stores/{store_id}/memories \
  -H "Content-Type: application/json" \
  -H "X-Tenant-Id: acme-corp" \
  -H "X-Agent-Id: code-reviewer" \
  -d '{
    "layer": "semantic",
    "content": {"text": "The auth module uses bcrypt with cost factor 12"},
    "owner_agent_id": "code-reviewer",
    "scope": {},
    "tags": ["auth", "security"]
  }'
```

### Search Memories

KD6 supports keyword search (FTS5) and vector search (cosine similarity).

**Keyword search:**

```bash
curl -X POST http://localhost:8080/v1/stores/{store_id}/search \
  -H "Content-Type: application/json" \
  -H "X-Tenant-Id: acme-corp" \
  -d '{
    "text": "authentication bcrypt",
    "keyword": true,
    "limit": 10
  }'
```

**Vector search** (requires embeddings to be stored on memories):

```bash
curl -X POST http://localhost:8080/v1/stores/{store_id}/search \
  -H "Content-Type: application/json" \
  -H "X-Tenant-Id: acme-corp" \
  -d '{
    "embedding": [0.1, 0.2, 0.3, ...],
    "keyword": false,
    "limit": 5
  }'
```

### Update a Memory

Partial updates are supported. Only include the fields you want to change.
To clear a nullable field (like `expires_at`), set it to `null` explicitly.

```bash
curl -X PATCH http://localhost:8080/v1/stores/{store_id}/memories/{memory_id} \
  -H "Content-Type: application/json" \
  -H "X-Tenant-Id: acme-corp" \
  -d '{
    "tags": ["auth", "security", "password-hashing"],
    "expires_at": null
  }'
```

### Graph Operations

Create relationships between memories and traverse the graph:

```bash
# Create an edge
curl -X POST http://localhost:8080/v1/stores/{store_id}/graph/edges \
  -H "Content-Type: application/json" \
  -H "X-Tenant-Id: acme-corp" \
  -d '{
    "source_memory_id": "...",
    "target_memory_id": "...",
    "relation_type": "depends_on",
    "weight": 1.0
  }'

# Traverse from a starting memory
curl -X POST http://localhost:8080/v1/stores/{store_id}/graph/traverse \
  -H "Content-Type: application/json" \
  -H "X-Tenant-Id: acme-corp" \
  -d '{
    "start_memory_id": "...",
    "depth": 3,
    "relation_types": ["depends_on", "related_to"]
  }'
```

### Batch Operations

Create or delete multiple memories in a single request:

```bash
curl -X POST http://localhost:8080/v1/stores/{store_id}/memories/batch \
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

### Audit Log

View the audit trail for a store or a specific memory:

```bash
# Store-level audit
curl "http://localhost:8080/v1/stores/{store_id}/audit?limit=20" \
  -H "X-Tenant-Id: acme-corp"

# Memory-level audit
curl "http://localhost:8080/v1/stores/{store_id}/memories/{memory_id}/audit" \
  -H "X-Tenant-Id: acme-corp"
```

### GDPR Purge

Remove all memories matching a scope and anonymize associated audit entries:

```bash
curl -X POST http://localhost:8080/v1/stores/{store_id}/gdpr/purge \
  -H "Content-Type: application/json" \
  -H "X-Tenant-Id: acme-corp" \
  -d '{
    "user_id": "user-42"
  }'
```

The purge requires at least one scope field beyond `tenant_id` to prevent
accidental deletion of all tenant data.

## MCP Tools Reference

When using KD6 through MCP, the following tools are available:

| Tool | Description |
|---|---|
| `create_store` | Create a new memory store for a tenant |
| `list_stores` | List all stores for a tenant |
| `create_memory` | Store a new memory entry |
| `get_memory` | Retrieve a specific memory by ID |
| `search_memories` | Search with keywords or vectors |
| `delete_memory` | Delete a memory entry |
| `create_edge` | Create a graph edge between memories |
| `traverse_graph` | Walk the knowledge graph from a starting node |
| `gdpr_purge` | Purge memories by scope and anonymize audit data |

Each tool accepts a `tenant_id` parameter for isolation. The MCP server
defaults to keyword search when the `keyword` parameter is not specified, since
MCP clients typically do not have access to embedding models.

## Development

```bash
# Run all tests
cargo test

# Run tests for a specific crate
cargo test -p kd6-sqlite

# Run a single test by name
cargo test test_create_memory

# Lint
cargo clippy -- -D warnings

# Format
cargo fmt
```

Tests use in-memory SQLite (`sqlite::memory:`) and require no external setup.
