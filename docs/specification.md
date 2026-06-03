# OMS Specification

KD6 is a reference implementation of the **Open Memory Service (OMS)**
specification. The full specification lives in [`spec/oms-spec.md`](../spec/oms-spec.md)
at the root of this repository.

## What is OMS?

The Open Memory Service specification defines a standard interface for agent
memory systems. It addresses a gap in the agentic AI ecosystem: while there
are many proprietary memory solutions, there is no common interface that lets
agents store, retrieve, and manage knowledge independently of the underlying
storage engine.

OMS defines:

- A **memory model** with five layers (working, episodic, semantic, procedural,
  archival)
- A **scope hierarchy** for multi-tenant, multi-agent isolation
- A **REST API** for HTTP-based access (using human-readable store names as
  the primary identifier)
- A **Service Provider Interface (SPI)** that backends implement
- Three **conformance levels** with increasing capability requirements

## Conformance Levels

### Level 1: Core

The minimum viable memory service. Required capabilities:

- Store CRUD (create, read, update, delete) with name-based routing
- Memory CRUD with partial updates and upsert support
- Vector search (semantic similarity)
- Keyword search (full-text)
- Tenant isolation
- Capabilities discovery

Any system that implements Level 1 can participate in the OMS ecosystem as a
basic memory store.

### Level 2: Standard

Production-grade features. Adds:

- Audit logging with hash chain integrity
- TTL and lifecycle management (expiration, purge)
- Batch operations (create and delete)
- Hierarchical scoping (8-level scope tree)
- Memory inheritance and layer promotion (bubble-up)
- Shared spaces for cross-agent collaboration

Level 2 is where most production deployments should target. It provides the
operational features needed to run memory at scale: audit trails for compliance,
TTL for cost control, batching for throughput, and shared spaces for multi-agent
coordination.

### Level 3: Advanced

Capabilities for knowledge graphs, compliance, and temporal reasoning:

- Graph memory with typed edges and BFS traversal
- Temporal metadata (valid_from, valid_until, confidence)
- GDPR purge with audit anonymization
- Cryptographic audit chain (SHA-256)
- Data sovereignty configuration

Level 3 is for deployments that need to model relationships between knowledge
(graph), reason about time-sensitive facts (temporal), or operate under
regulatory constraints (GDPR, data residency).

## KD6's Conformance

KD6 implements all three levels using SQLite as the storage backend. The
conformance matrix:

| Feature | Level | Status |
|---|---|---|
| Store CRUD (name-based routing) | 1 | ✅ Implemented |
| Memory CRUD (with upsert) | 1 | ✅ Implemented |
| Vector search (cosine) | 1 | ✅ Implemented |
| Keyword search (FTS5) | 1 | ✅ Implemented |
| Tenant isolation | 1 | ✅ Implemented |
| Capabilities | 1 | ✅ Implemented |
| Server-side embedding | 1 | ✅ Implemented |
| Audit logging | 2 | ✅ Implemented |
| TTL / lifecycle | 2 | ✅ Implemented |
| Batch operations | 2 | ✅ Implemented |
| Hierarchical scoping | 2 | ✅ Implemented |
| Inheritance / bubble-up | 2 | ✅ Implemented |
| Shared spaces | 2 | ✅ Implemented |
| Graph memory | 3 | ✅ Implemented |
| Temporal metadata | 3 | ✅ Implemented |
| GDPR purge | 3 | ✅ Implemented |
| Cryptographic audit | 3 | ✅ Implemented |
| Data sovereignty | 3 | ✅ Implemented |

## The Memory Model

### Five Layers

```
           +-----------+
           |  Working  |   scratch pad, short-lived
           +-----------+
                |
           +-----------+
           | Episodic  |   records of events
           +-----------+
                |
           +-----------+
           | Semantic  |   distilled knowledge
           +-----------+
                |
           +-----------+
           | Procedural|   processes, workflows
           +-----------+
                |
           +-----------+
           |  Archival |   long-term storage
           +-----------+
```

Memories flow downward through inheritance rules. An agent might store raw
observations in the working layer, then use bubble-up to promote distilled
insights to the semantic layer. The inheritance system handles deduplication
automatically.

### Memory Entry Structure

Each memory entry contains:

| Field | Type | Description |
|---|---|---|
| `id` | UUID | Unique identifier |
| `store_id` | UUID | Parent store |
| `layer` | enum | One of the five memory layers |
| `content` | JSON | Arbitrary structured content |
| `embedding` | float[] | Optional vector embedding (auto-computed if provider configured) |
| `owner_agent_id` | string | Agent that created this memory |
| `scope` | object | Hierarchical visibility scope |
| `tags` | string[] | Freeform labels |
| `categories` | string[] | Classification labels |
| `source` | object | Provenance (conversation_id, document_id, uri) |
| `access_control` | object | Policy + allowed agents/scopes |
| `expires_at` | timestamp | Optional TTL |
| `immutable` | bool | Whether the entry can be modified |
| `version` | int | Optimistic concurrency counter |
| `valid_from` | timestamp | Temporal: when this became true |
| `valid_until` | timestamp | Temporal: when this stops being true |
| `confidence` | float | Temporal: certainty score (0.0–1.0) |
| `entity_type` | string | Graph: type for graph node classification |
| `upsert_key` | string | Upsert: unique key for create-or-replace semantics |

## The SPI

The Service Provider Interface is defined as the `OmsProvider` Rust trait in
`kd6-core`. Backend implementors provide a struct that implements this trait.
The platform (HTTP server, MCP server, or any other consumer) interacts with
the backend exclusively through this interface.

```rust
#[async_trait]
pub trait OmsProvider: Send + Sync {
    // Store management (7 methods)
    async fn create_store(...) -> Result<MemoryStore, OmsError>;
    async fn get_store(...) -> Result<MemoryStore, OmsError>;
    async fn get_store_by_name(...) -> Result<MemoryStore, OmsError>;
    async fn list_stores(...) -> Result<Vec<MemoryStore>, OmsError>;
    async fn get_or_create_store(...) -> Result<MemoryStore, OmsError>;
    async fn update_store(...) -> Result<MemoryStore, OmsError>;
    async fn delete_store(...) -> Result<(), OmsError>;

    // Memory CRUD (5 methods)
    async fn create_memory(...) -> Result<MemoryEntry, OmsError>;
    async fn get_memory(...) -> Result<MemoryEntry, OmsError>;
    async fn list_memories(...) -> Result<Page<MemoryEntry>, OmsError>;
    async fn update_memory(...) -> Result<MemoryEntry, OmsError>;
    async fn delete_memory(...) -> Result<(), OmsError>;

    // Search, health, capabilities (3 methods)
    async fn search(...) -> Result<Vec<SearchResult>, OmsError>;
    async fn stats(...) -> Result<StoreStats, OmsError>;
    fn capabilities(&self) -> ProviderCapabilities;

    // Level 2: audit, lifecycle, batch, inheritance, shared spaces (~12 methods)
    // Level 3: graph, GDPR purge (~4 methods)
}
```

Adding a new backend (Postgres, DynamoDB, in-memory) means implementing this
trait. The HTTP and MCP servers work unchanged.

## Specification Evolution

The OMS specification is designed to grow. New conformance levels or optional
extensions can be added without breaking existing implementations. Level 1
remains the stable baseline that all providers must support.

KD6 tracks the specification as it evolves. The version in `spec/oms-spec.md`
is the one this implementation targets.
