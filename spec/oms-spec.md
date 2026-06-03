# Open Memory Service (OMS) Specification

> **Version:** 0.1.0 (Draft)
> **Date:** May 24, 2026
> **Status:** Draft — For internal review and stakeholder feedback
> **Companion Document:** [Research Report](./research.md)

---

## Table of Contents

1. [Purpose](#1-purpose)
2. [Design Principles](#2-design-principles)
3. [Terminology](#3-terminology)
4. [Core Concepts](#4-core-concepts)
   - 4.1 [Memory Store](#41-memory-store)
   - 4.2 [Memory Layers](#42-memory-layers)
   - 4.3 [Memory Entry](#43-memory-entry)
   - 4.4 [Memory Scope (Hierarchical)](#44-memory-scope-hierarchical)
   - 4.5 [Parent-Child Memory Inheritance](#45-parent-child-memory-inheritance)
   - 4.6 [Shared Memory Spaces (Blackboard)](#46-shared-memory-spaces-blackboard)
5. [API Surface](#5-api-surface)
   - 5.1 [Memory Store Management](#51-memory-store-management)
   - 5.2 [Memory CRUD](#52-memory-crud)
   - 5.3 [Memory Search](#53-memory-search)
   - 5.4 [Memory Lifecycle](#54-memory-lifecycle)
   - 5.5 [Memory Inheritance & Sharing](#55-memory-inheritance--sharing)
   - 5.6 [Audit & Compliance](#56-audit--compliance)
6. [Authentication & Multi-Tenancy](#6-authentication--multi-tenancy)
7. [Protocol Integration](#7-protocol-integration)
8. [Embedding Provider Interface](#8-embedding-provider-interface)
   - 8.1 [Motivation](#81-motivation)
   - 8.2 [Embedding Provider SPI](#82-embedding-provider-spi)
   - 8.3 [Store-Level Embedding Configuration](#83-store-level-embedding-configuration)
   - 8.4 [Automatic Embedding Behavior](#84-automatic-embedding-behavior)
   - 8.5 [Embedding Model Lifecycle](#85-embedding-model-lifecycle)
   - 8.6 [Built-in Provider Requirements](#86-built-in-provider-requirements)
   - 8.7 [Capability Advertisement](#87-capability-advertisement)
9. [Backend Provider Interface (SPI)](#9-backend-provider-interface-spi)
10. [Data Sovereignty](#10-data-sovereignty)
11. [Conformance Levels](#11-conformance-levels)
12. [Reference Implementation Guidance](#12-reference-implementation-guidance)
13. [Relationship to Existing Work](#13-relationship-to-existing-work)
14. [References](#14-references)

---

## 1. Purpose

The **Open Memory Service (OMS)** specification defines a standard interface for multilayered, multi-tenant agent memory services. It enables:

- **Platform providers** to offer memory as a managed service with standard APIs
- **Customers** to implement their own OMS-compliant memory backend ("bring your own memory")
- **Agent frameworks** to consume memory through a uniform API regardless of backend
- **Open source community** to build interoperable implementations that can be composed and swapped

The companion [Research Report](./research.md) provides detailed background on existing frameworks, protocols, and a gap analysis motivating this specification. In summary: no existing protocol defines the **memory service contract** — the CRUD, search, lifecycle, sharing, and sovereignty API that a pluggable memory backend must implement. MCP [18] defines agent↔tool connectivity; A2A [19] defines agent↔agent communication; ACP defines REST-native agent messaging. OMS fills the remaining gap: the memory service itself.

---

## 2. Design Principles

1. **Layered by default** — Memory is organized into tiers with different lifecycles and access patterns
2. **Tenant-isolated** — Every memory operation is scoped to a tenant boundary; cross-tenant access is impossible by default
3. **Hierarchical sharing** — Parent agents can grant scoped memory access to children; peers can share via explicit workspaces
4. **Backend-agnostic** — The spec defines the API contract, not the storage engine; conformant backends declare their capabilities
5. **Protocol-compatible** — Designed to complement MCP (agent↔memory), A2A (agent↔agent), and ACP (REST)
6. **Sovereignty-aware** — Memory can be pinned to geographic regions with configurable cross-region policies
7. **Auditable** — All mutations produce audit events with non-repudiable provenance
8. **Lifecycle-managed** — Memory has explicit TTL, compaction, summarization, and archival policies
9. **Incrementally adoptable** — Conformance levels allow implementors to start simple and add features over time

---

## 3. Terminology

| Term | Definition |
|------|-----------|
| **Memory Store** | A named, configured container for memories. The primary unit of configuration, isolation, and lifecycle management. |
| **Memory Entry** | A single unit of stored memory within a store, belonging to a specific layer. |
| **Memory Layer** | A tier in the memory hierarchy (working, episodic, semantic, procedural, archival) with distinct lifecycle and access characteristics. |
| **Memory Scope** | A hierarchical identifier (tenant → org → team → project → user → agent → session → run) that controls visibility. |
| **Inheritance** | A parent-child relationship where a child agent receives filtered, read-only (or read-write) access to a parent's memories. |
| **Shared Memory Space** | A blackboard-style workspace where multiple peer agents can read and write collaboratively. |
| **Provider (SPI)** | A backend implementation that fulfills the OMS service provider interface. |
| **Conformance Level** | One of three tiers (Core, Standard, Advanced) indicating which spec features an implementation supports. |

---

## 4. Core Concepts

### 4.1 Memory Store

A **Memory Store** is a named, configured container for memories. Each tenant has one or more stores. A store is the primary unit of configuration, isolation, and lifecycle management.

```yaml
MemoryStore:
  id: string (UUID)
  name: string
  tenant_id: string
  region: string                          # data sovereignty
  config:
    layers: LayerConfig[]                 # which memory layers are enabled
    backend: BackendConfig                # pluggable backend configuration
    default_ttl: duration | null          # default TTL for new entries
    default_sharing_policy: SharingPolicy # default access policy
    embedding:                            # embedding provider config (see section 8)
      provider: string | null             #   provider identifier (e.g., "openai", "ollama")
      model: string | null                #   model name (e.g., "text-embedding-3-small")
      dimensions: int | null              #   override dimensions (if supported by model)
    compaction_policy: CompactionPolicy | null
  metadata: map<string, string>           # arbitrary user metadata
  created_at: timestamp
  updated_at: timestamp
```

#### 4.1.1 Default Store and Auto-Provisioning

Implementations MAY support a **default store** identified by the well-known alias `_default`. When enabled, any API call that would normally require a `store_id` path parameter MAY use `_default` as the store identifier.

When auto-provisioning is enabled and a write operation (memory creation, upsert) targets a store or tenant that does not yet exist, the service MUST lazily create the missing entities before completing the write:

1. If the resolved tenant does not exist, create it with implementation-defined defaults.
2. If the target store does not exist within the resolved tenant (whether `_default` or an explicitly named store), create it with implementation-defined default configuration.
3. Return the write result as normal. The caller does not need to distinguish between a pre-existing store and a newly provisioned one.

This lazy provisioning means an agent's very first memory write is self-sufficient. No setup calls are required. Read operations against non-existent stores or tenants MUST return empty results (not errors), so that search-before-write patterns also work without prior provisioning.

This feature is intended for development, single-agent, and evaluation scenarios where explicit store and tenant management adds unnecessary ceremony. Implementations that enable it SHOULD document the default configuration applied to auto-provisioned stores.

> **Security consideration:** In multi-tenant production deployments, auto-provisioning may allow any authenticated agent to implicitly create tenants or provision storage. Implementations MUST provide a configuration flag to disable auto-provisioning and MUST disable it by default when the deployment is configured for multi-tenant operation. When disabled, writes targeting non-existent stores MUST be rejected with `404 Not Found` and requests with unrecognized tenant context MUST be rejected with `403 Forbidden`.

### 4.2 Memory Layers

Every store supports up to five memory layers, each with independent configuration. The five-layer model is grounded in cognitive science research on human memory systems [1][2][3][4]:

| Layer | Purpose | Typical TTL | Storage Pattern | Cognitive Basis |
|-------|---------|-------------|----------------|----------------|
| `working` | Active task scratchpad, current reasoning context | Minutes to hours | Key-value / in-memory | Working Memory (Baddeley & Hitch, 1974) [2] |
| `episodic` | Specific interaction records, event logs | Days to months | Vector + metadata | Episodic Memory (Tulving, 1972) [1] |
| `semantic` | Extracted facts, entities, relationships | Months to years | Vector + knowledge graph | Semantic Memory (Tulving, 1972) [1] |
| `procedural` | Learned patterns, skills, user preferences | Long-lived | Structured store | Procedural Memory (Squire, 2004) [3] |
| `archival` | Compressed, summarized historical context | Indefinite | Blob + metadata index | Long-term Store (Atkinson & Shiffrin, 1968) [4] |

Each layer has independent:
- TTL and retention policies
- Compaction / summarization rules (e.g., "after 30 days, summarize episodic → archival")
- Access control defaults
- Backend requirements (declared via capabilities)

### 4.3 Memory Entry

```yaml
MemoryEntry:
  id: string (UUID)
  store_id: string
  layer: "working" | "episodic" | "semantic" | "procedural" | "archival"
  content: string | object                # the memory content (see 4.3.1)
  embedding: float[] | null               # vector representation (server-computed or caller-provided; see section 8)
  upsert_key: string | null               # optional idempotency key for upsert semantics (see 4.3.2)
  metadata:
    owner_agent_id: string                # which agent created this entry
    scope: MemoryScope                    # visibility and sharing scope
    tags: string[]                        # user-defined tags for filtering
    categories: string[]                  # system-defined categories
    source: SourceReference | null        # provenance (e.g., conversation ID, document ID)
    temporal:                             # optional temporal metadata (inspired by Zep [10])
      valid_from: timestamp | null        # when this fact became true
      valid_until: timestamp | null       # when this fact became false (null = still valid)
      confidence: float | null            # confidence score (0.0-1.0)
  access_control:
    policy: "private" | "inherit" | "shared" | "public_read"
    allowed_agents: string[] | null       # specific agents granted access
    allowed_scopes: string[] | null       # scopes granted access
  lifecycle:
    created_at: timestamp
    updated_at: timestamp
    expires_at: timestamp | null          # null = no expiration
    immutable: boolean                    # if true, content cannot be modified
    version: integer                      # incremented on each update
  graph:                                  # optional graph relationships
    entity_type: string | null            # e.g., "person", "concept", "preference"
    relationships: Relationship[] | null  # edges to other entities
```

#### 4.3.1 Content Format

The `content` field accepts both plain strings and structured JSON objects. When a client provides a plain string, the service MUST accept it as the full memory content and return it as a string in subsequent reads. Implementations MUST NOT require clients to wrap text in a JSON envelope (e.g., `{"text": "..."}`) when the content is unstructured text.

Structured content is appropriate when the memory carries typed fields, key-value metadata, or nested data that the client wants to preserve with schema. The service MUST round-trip structured content without modification.

#### 4.3.2 Upsert Semantics

The `upsert_key` field enables atomic create-or-replace behavior. When a `POST` to the memory creation endpoint includes a non-null `upsert_key`, the service MUST apply the following logic:

1. Search for an existing memory entry within the same `store_id`, `layer`, and `scope` that has a matching `upsert_key`.
2. If a match exists: replace its `content`, increment its `version`, update `updated_at`, and return the updated entry. The previous content MUST be recorded in the audit trail.
3. If no match exists: create a new entry as normal.

This operation MUST be atomic. Concurrent upserts with the same key MUST serialize and produce a consistent result (last writer wins). The `upsert_key` is scoped to the combination of store, layer, and scope to prevent unintended collisions across organizational boundaries.

**Use cases:**
- Storing agent preferences that should have exactly one current value (e.g., `upsert_key: "preference:theme"`)
- Maintaining singleton facts that are periodically refreshed (e.g., `upsert_key: "agent-status:summarizer"`)
- Avoiding search-then-delete-then-create race conditions in concurrent agent systems

### 4.4 Memory Scope (Hierarchical)

Scopes define visibility boundaries and enable parent-child memory inheritance. The design follows a hierarchical model where more specific scopes inherit from broader ones:

```yaml
MemoryScope:
  tenant_id: string               # REQUIRED -- the hard isolation boundary
  org_id: string | null           # organizational unit within tenant
  team_id: string | null          # team within org
  project_id: string | null       # project within team
  user_id: string | null          # end-user
  agent_id: string | null         # specific agent
  session_id: string | null       # specific conversation session
  run_id: string | null           # specific execution run
```

#### 4.4.1 Default Tenant

Implementations MAY support a **default tenant** identified by the well-known value `_default`. When enabled, requests that do not provide explicit tenant context (no `X-Tenant-ID` header, no `tenant_id` JWT claim) are resolved to the default tenant.

When combined with auto-provisioning (see 4.1.1), the default tenant is lazily created on the first write operation that resolves to it. This allows agents and frameworks that have no concept of multi-tenancy to issue their first memory write with zero prior setup.

This feature is intended for local development, single-user tools, and evaluation environments where tenant management is unnecessary overhead.

> **Security consideration:** Enabling default tenant resolution in a multi-tenant production deployment constitutes a security risk. A request that omits tenant context would silently resolve to the default tenant rather than being rejected, potentially causing data to land in the wrong isolation boundary. Implementations MUST provide a configuration flag to disable default tenant support and MUST disable it by default when the deployment is configured for multi-tenant operation.

When default tenant support is disabled, requests that omit tenant context MUST be rejected with `400 Bad Request`.

**Scope resolution rules:**

A memory entry is visible to any agent whose scope is **equal to or more specific than** the entry's scope:

```
tenant-123                           ← tenant-scoped: visible to all in tenant
├── org-456                          ← org-scoped: visible to all in org
│   ├── team-789                     ← team-scoped: visible to all in team
│   │   ├── project-abc              ← project-scoped: visible to all in project
│   │   │   ├── agent-parent         ← agent-scoped: visible only to this agent
│   │   │   │   └── agent-child-1    ← sees parent's "inherit" memories
│   │   │   │   └── agent-child-2    ← sees parent's "inherit" memories
│   │   │   │                          (cannot see sibling's "private" memories)
```

Memories with `policy: "inherit"` are automatically visible to child agents spawned by the owner. Memories with `policy: "private"` are visible only to the owning agent. Memories with `policy: "shared"` are visible to all agents matching the `allowed_agents` or `allowed_scopes` lists.

### 4.5 Parent-Child Memory Inheritance

When a parent agent spawns a child, the platform creates an inheritance relationship:

```yaml
InheritanceSpec:
  parent_agent_id: string
  child_agent_id: string
  inherit_layers: string[]          # which layers to inherit (e.g., ["semantic", "procedural"])
  filter:
    tags: string[] | null           # only inherit memories with these tags
    categories: string[] | null
    max_entries: integer | null     # limit inherited entries
    time_range:                     # only inherit recent memories
      from: timestamp | null
      to: timestamp | null
  access: "read_only" | "read_write"
  bubble_up:                        # what the child can return to parent
    enabled: boolean
    auto_summarize: boolean         # summarize child's working memory on completion
    layers: string[]                # which of child's layers to bubble up
```

This design is informed by CrewAI's delegation model [14] and LangGraph's state propagation [15], but formalized as a first-class API rather than an implicit framework behavior.

### 4.6 Shared Memory Spaces (Blackboard)

For peer agents that need shared workspace (inspired by classical blackboard architectures [16][17] and memX [13]):

```yaml
SharedMemorySpace:
  id: string
  name: string
  store_id: string
  scope: MemoryScope                      # visibility boundary
  participants: AgentParticipant[]
  layer: string                           # typically "working" or "episodic"
  conflict_resolution: "last_write_wins" | "orchestrator_merge" | "crdt"
  notifications:
    on_write: boolean                     # pub/sub notifications to participants
    on_delete: boolean

AgentParticipant:
  agent_id: string
  access: "read_only" | "read_write" | "admin"
  joined_at: timestamp
```

---

## 5. API Surface

### 5.1 Memory Store Management

```
POST   /v1/stores                           # Create a memory store
GET    /v1/stores                           # List stores (filtered by tenant)
GET    /v1/stores/{store_id}                # Get store details + capabilities
PATCH  /v1/stores/{store_id}                # Update store configuration
DELETE /v1/stores/{store_id}                # Delete store (with policy enforcement)
```

### 5.2 Memory CRUD

```
POST   /v1/stores/{store_id}/memories                # Create memory entry (supports upsert, see 4.3.2)
GET    /v1/stores/{store_id}/memories/{memory_id}     # Get by ID
PATCH  /v1/stores/{store_id}/memories/{memory_id}     # Update entry (versioned)
DELETE /v1/stores/{store_id}/memories/{memory_id}     # Delete entry (audited)
GET    /v1/stores/{store_id}/memories                 # List with filters + pagination
```

The `store_id` path parameter accepts either a concrete store UUID or the well-known alias `_default` when default store support is enabled (see 4.1.1). This alias resolution applies to all store-scoped endpoints across the API surface.

### 5.3 Memory Search

```http
POST /v1/stores/{store_id}/search
Content-Type: application/json

{
  "query": "string",                    // natural language or structured query
  "embedding": [0.1, 0.2, ...] | null, // optional pre-computed query embedding
  "layers": ["episodic", "semantic"],   // which layers to search
  "scope": { ... },                     // scope filter
  "top_k": 10,
  "threshold": 0.3,                     // minimum similarity score
  "filters": {                          // metadata filters
    "tags": ["preference"],
    "categories": ["dietary"],
    "temporal": {                       // Zep-inspired temporal filtering [10]
      "valid_at": "2026-05-24T00:00:00Z"
    }
  },
  "rerank": true,                       // apply reranking model
  "keyword": false,                     // include BM25 keyword search
  "include_graph": false                // traverse graph relationships
}
```

When the store has a configured embedding provider (see section 8), the `embedding` field is optional — the service computes a query embedding from `query` automatically. When no embedding provider is configured, callers must supply `embedding` for vector search or set `keyword: true` for text-only search.

### 5.4 Memory Lifecycle

```
POST   /v1/stores/{store_id}/compact         # Trigger compaction (merge similar memories)
POST   /v1/stores/{store_id}/archive         # Move old memories to archival layer
POST   /v1/stores/{store_id}/summarize       # Summarize layer(s) into higher-level memory
POST   /v1/stores/{store_id}/migrate         # Migrate memories between layers
GET    /v1/stores/{store_id}/lifecycle/stats  # Memory health, usage stats, layer sizes
DELETE /v1/stores/{store_id}/expired          # Purge expired entries
```

### 5.5 Memory Inheritance & Sharing

```
POST   /v1/stores/{store_id}/inherit                    # Create inheritance relationship
DELETE /v1/stores/{store_id}/inherit/{inheritance_id}    # Revoke inheritance
POST   /v1/stores/{store_id}/bubble-up                  # Child returns results to parent

POST   /v1/stores/{store_id}/shared-spaces              # Create shared memory space
GET    /v1/stores/{store_id}/shared-spaces               # List shared spaces
GET    /v1/stores/{store_id}/shared-spaces/{space_id}    # Get space details
POST   /v1/stores/{store_id}/shared-spaces/{space_id}/join    # Add agent to space
DELETE /v1/stores/{store_id}/shared-spaces/{space_id}/leave   # Remove agent from space
DELETE /v1/stores/{store_id}/shared-spaces/{space_id}         # Delete shared space
```

### 5.6 Audit & Compliance

```
GET    /v1/stores/{store_id}/audit                           # Query audit log (paginated)
GET    /v1/stores/{store_id}/memories/{memory_id}/audit      # Entry-level audit trail
POST   /v1/stores/{store_id}/export                          # Export for compliance review
POST   /v1/stores/{store_id}/purge                           # GDPR right-to-erasure
```

---

## 6. Authentication & Multi-Tenancy

Every request MUST include:
- **Tenant context** -- via `X-Tenant-ID` header or `tenant_id` claim in JWT. Implementations that support default tenant resolution (see 4.4.1) MAY omit this requirement for requests targeting the default tenant.
- **Agent identity** -- via `X-Agent-ID` header or `agent_id` claim in JWT
- **Authentication** -- OAuth 2.1 bearer token or mTLS

The service MUST enforce:
- **Tenant isolation** at the data layer -- requests MUST NOT cross tenant boundaries under any circumstances
- **Agent-level authorization** per the entry's `access_control` policy
- **Rate limiting** per tenant and per agent
- **Audit logging** for all write operations

---

## 7. Protocol Integration

### 7.1 MCP Integration

An OMS-compliant memory service SHOULD expose itself as an MCP Server [18]:

```json
{
  "name": "oms-memory",
  "version": "1.0.0",
  "capabilities": {
    "resources": [
      { "uri": "memory://stores/{store_id}/memories", "name": "Agent Memories" }
    ],
    "tools": [
      { "name": "memory_search", "description": "Search agent memory" },
      { "name": "memory_store",  "description": "Store a new memory" },
      { "name": "memory_recall", "description": "Retrieve specific memory by ID" }
    ]
  }
}
```

### 7.2 A2A Integration

When agents delegate tasks across boundaries [19]:
1. Delegating agent creates an `InheritanceSpec` for the child agent
2. Includes the `store_id` and OMS inherit endpoint in the A2A Task artifact
3. Child agent calls the OMS inherit endpoint to receive scoped memory access
4. On task completion, child calls the bubble-up endpoint to return results
5. Delegating agent revokes the inheritance relationship

---

## 8. Embedding Provider Interface

Server-side embedding is a core capability that allows OMS implementations to automatically compute vector representations for memory content. This eliminates the requirement for callers to supply pre-computed embeddings and enables seamless integration with agent frameworks that treat the memory backend as a "text in, relevance out" service.

### 8.1 Motivation

Most agent frameworks (LangChain, CrewAI, Google ADK, Squad) send plain text to their memory backend and expect the backend to handle vectorization. Without server-side embedding, every client adapter must independently manage an embedding model — introducing latency (extra network hop), cost (duplicate model loading), and complexity (version skew between embedding models across clients). This was identified as the P0 integration gap blocking LangChain and other major frameworks from using OMS backends.

By defining embedding as a first-class concern in the OMS spec, implementations gain:
- **Zero-config integration** — agents write text, the service handles the rest
- **Consistency** — all memories in a store use the same embedding model and dimensions
- **Upgradeability** — the store owner can upgrade the embedding model without changing clients
- **Hybrid search** — keyword (BM25) and vector search merge naturally when embeddings are always present

### 8.1.1 Client-Side vs. Server-Side Embedding

OMS supports two embedding strategies. The choice is made **per-request**, not per-store, and the two strategies can coexist within the same store:

| Strategy | How it works | When to use |
|---|---|---|
| **Server-side (recommended)** | Caller sends plain text. The OMS service computes the embedding using its configured provider before storing or searching. | Default path. Simplest for callers. Guarantees model consistency across all entries. Required for framework integrations (LangChain, CrewAI, etc.) that send text only. |
| **Client-side** | Caller computes the embedding externally and includes it in the `embedding` field of the request. The service validates dimensionality but does **not** re-embed. | Advanced use cases where the caller controls the model (e.g., fine-tuned domain embeddings, multi-modal embeddings, or embeddings computed during an upstream pipeline step). |

**Design principles:**

1. **Server-side is the default.** When the `embedding` field is absent from a write or search request, the service MUST compute it from content/query text. This makes the simplest possible API call — `{"content": "some text"}` — fully functional with vector search.

2. **Client-side is an opt-in override.** When the caller provides an `embedding` field, the service uses it as-is. This respects caller expertise while still enforcing dimensionality constraints.

3. **Consistency within a store.** All embeddings in a store MUST share the same dimensionality, regardless of whether they were computed server-side or client-side. A client-provided embedding with the wrong dimensionality MUST be rejected. Callers who provide their own embeddings are responsible for using a model that produces vectors in the same semantic space as the store's configured provider — mixing incompatible models degrades search quality.

4. **No embedding function required in client adapters.** Framework integrations (e.g., LangChain VectorStore, CrewAI RAGStorage) SHOULD default to server-side embedding, making the embedding parameter optional in client constructors. This is the key usability win: `store = KD6VectorStore(base_url="http://localhost:8080")` works with no embedding model configuration on the client side.

### 8.2 Embedding Provider SPI

Implementations MUST support a pluggable embedding provider interface. The interface is intentionally minimal to accommodate local models, remote APIs, and managed services:

```python
class EmbeddingProvider(ABC):
    """Computes vector embeddings from text content.
    
    Implementations may call a local model (e.g., ONNX, Sentence Transformers),
    a remote API (e.g., OpenAI, Azure OpenAI, Cohere, Voyager), or a managed
    service (e.g., Vertex AI, Amazon Bedrock).
    """

    @abstractmethod
    def embed_texts(self, texts: list[str]) -> list[list[float]]:
        """Compute embeddings for one or more text strings.
        
        Args:
            texts: The input strings to embed. Each string may be a plain
                   memory content string or a query string.
        
        Returns:
            A list of embedding vectors, one per input text.
            All vectors MUST have the same dimensionality.
        
        Raises:
            EmbeddingError: If the provider is unavailable or the input
                            exceeds provider-specific limits.
        """
        ...

    @abstractmethod
    def embed_query(self, query: str) -> list[float]:
        """Compute a single embedding for a search query.
        
        Some embedding models use different prefixes or instructions for
        queries vs. documents (e.g., "query: " vs. "passage: " in E5 models).
        This method allows the provider to apply query-specific preprocessing.
        
        Default implementations MAY delegate to embed_texts([query])[0].
        """
        ...

    @abstractmethod
    def dimensions(self) -> int:
        """Return the dimensionality of embeddings produced by this provider.
        
        This value MUST be constant for the lifetime of the provider instance
        and MUST match the length of all vectors returned by embed_texts
        and embed_query.
        """
        ...

    @abstractmethod
    def model_id(self) -> str:
        """Return a stable identifier for the embedding model.
        
        Used for tracking which model produced stored embeddings, enabling
        migration detection when a store's model is upgraded.
        
        Examples: "text-embedding-3-small", "all-MiniLM-L6-v2",
                  "voyage-3", "text-embedding-004"
        """
        ...
```

### 8.3 Store-Level Embedding Configuration

Each memory store MAY be configured with an embedding provider. The `embedding` field in `StoreConfig` specifies the provider and its parameters:

```yaml
StoreConfig:
  embedding:
    provider: string           # provider identifier (e.g., "openai", "ollama", "sentence-transformers")
    model: string              # model name within the provider (e.g., "text-embedding-3-small")
    dimensions: int | null     # override dimensions (for models that support variable dimensions)
    options: map<string, string>  # provider-specific options (e.g., base_url, api_version)
```

When `embedding` is `null` or omitted, the store operates in **pass-through mode**: callers MUST supply pre-computed embeddings for vector search, and keyword search remains available without embeddings. This preserves backward compatibility and supports use cases where the caller controls the embedding model.

### 8.4 Automatic Embedding Behavior

When a store has a configured embedding provider, the service MUST apply the following rules. These rules implement the client-side/server-side coexistence described in section 8.1.1: server-side embedding is the default, and client-provided embeddings are accepted as an opt-in override.

#### 8.4.1 On Memory Write (`POST /v1/stores/{store_id}/memories`)

1. **Client-side override:** If the request includes a non-null `embedding` field, use the caller-provided embedding as-is. The service MUST validate that the dimensionality matches the store's configured model and reject mismatches with `422 Unprocessable Entity`.
2. **Server-side default:** If the request omits `embedding` (or sets it to `null`) and a provider is configured, the service MUST extract text from `content` and compute an embedding before storing:
   - For string content: embed the string directly.
   - For structured (JSON object) content: serialize to a canonical text representation. Implementations SHOULD concatenate string-typed leaf values. Implementations MAY accept a `content_text_field` store config option to specify which field(s) to embed (e.g., `"text"`, `"body"`).
3. **Pass-through mode:** If the request omits `embedding` and no provider is configured (pass-through mode), the memory is stored without an embedding. Vector search will not return this entry, but keyword search will.
4. The computed or provided embedding MUST be stored alongside the memory entry and returned in subsequent reads (when the `embedding` field is requested).
5. Batch write operations (`POST /v1/stores/{store_id}/batch`) MUST apply the same rules per entry. Implementations SHOULD batch embedding calls to the provider for efficiency.

#### 8.4.2 On Memory Update (`PATCH /v1/stores/{store_id}/memories/{memory_id}`)

1. If the update modifies `content` and includes a new `embedding`, use the caller-provided embedding (client-side override). Validate dimensionality.
2. If the update modifies `content` but omits `embedding`, the service MUST recompute the embedding from the new content using the configured provider (server-side default).
3. If the update does not modify `content`, the existing embedding MUST be preserved regardless of whether `embedding` is present in the request.

#### 8.4.3 On Search (`POST /v1/stores/{store_id}/search`)

1. **Client-side override:** If the request includes a non-null `embedding` field, use it as the query vector for similarity search. The service does not embed the `query` string.
2. **Server-side default:** If the request omits `embedding`, the service MUST compute a query embedding from the `query` string using the provider's `embed_query` method before performing similarity search.
3. **Pass-through mode:** If the request omits `embedding` and no provider is configured, vector similarity search is not available. The service MUST fall back to keyword-only search if a `query` string is present, or reject the request with `400 Bad Request` if keyword search is also not applicable.
4. When `keyword: true` is also set, the service performs both keyword and vector search and merges results using the existing merge strategy (see section 5.3).

This means a minimal search request — `{"query": "user preferences", "top_k": 10}` — performs full hybrid search (keyword + vector) when the store has an embedding provider and keyword search is available. No embedding knowledge is required on the caller side.

### 8.5 Embedding Model Lifecycle

#### 8.5.1 Model Versioning

Each stored embedding SHOULD be tagged with the `model_id` that produced it. Implementations MAY store this as metadata on the memory entry or as a store-level field in the database schema.

When a store's embedding model is changed (via `PATCH /v1/stores/{store_id}`), the service MUST NOT silently mix embeddings from different models in search results, as this produces meaningless similarity scores. Implementations MUST choose one of the following strategies:

1. **Lazy re-embedding (recommended):** Mark all existing entries as needing re-embedding. Recompute embeddings in the background or on next read. Search results during the migration period may exclude stale entries or return them with degraded scores.
2. **Eager re-embedding:** Immediately recompute all embeddings in the store. This may be expensive for large stores but ensures instant consistency.
3. **Reject model change:** Refuse to change the embedding model on a store that contains entries. Require the caller to create a new store and migrate entries explicitly.

Implementations MUST document which strategy they use.

#### 8.5.2 Dimensionality Constraints

All embeddings within a single store MUST have the same dimensionality. The dimensionality is determined by the store's configured embedding provider (or by the first caller-provided embedding if no provider is configured). Subsequent writes with mismatched dimensionality MUST be rejected with `422 Unprocessable Entity`.

### 8.6 Built-in Provider Requirements

Implementations at Level 1 conformance are NOT required to include any embedding provider — pass-through mode with keyword search is sufficient.

Implementations at Level 2 and above SHOULD provide at least one built-in embedding provider or document how to configure an external one. Recommended provider categories:

| Category | Examples | Trade-offs |
|---|---|---|
| **Local model** | ONNX Runtime, Sentence Transformers, FastEmbed | No network dependency, no API costs; requires CPU/GPU on the OMS host |
| **Remote API** | OpenAI, Azure OpenAI, Cohere, Voyager, Google Vertex AI | High quality, no local resources; adds latency and API costs |
| **Sidecar** | Ollama, vLLM, TEI (Text Embeddings Inference) | Decoupled scaling, GPU isolation; requires separate deployment |

### 8.7 Capability Advertisement

The `ProviderCapabilities` object (see section 9) MUST include embedding-related fields:

```python
@dataclass
class EmbeddingCapabilities:
    server_side_embedding: bool        # true if the provider has a configured embedding provider
    model_id: str | None               # the active embedding model identifier
    dimensions: int | None             # dimensionality of embeddings produced
    max_batch_size: int | None         # maximum texts per embed_texts call (null = unlimited)
    supports_query_prefix: bool        # true if embed_query applies distinct preprocessing
```

This allows clients to discover which embedding strategy to use:

```http
GET /v1/stores/{store_id}
→ { "capabilities": { "embedding": { "server_side_embedding": true, "model_id": "text-embedding-3-small", "dimensions": 1536 } } }
```

**Client behavior based on capabilities:**

- `server_side_embedding: true` — Clients SHOULD omit the `embedding` field in requests and let the server handle vectorization. This is the simplest integration path and is required for framework adapters (LangChain, CrewAI, etc.) that do not manage embedding models. Clients MAY still provide embeddings for override purposes.
- `server_side_embedding: false` — Clients MUST supply their own embeddings for vector search, or restrict to keyword-only search. Framework adapters that cannot provide embeddings SHOULD document this limitation clearly.

### 8.8 Reference Implementation (KD6)

The KD6 reference implementation provides two embedding providers selected via the `KD6_EMBEDDING_PROVIDER` environment variable:

| Provider | `KD6_EMBEDDING_PROVIDER` | Description |
|---|---|---|
| **Local (default)** | `local` | In-process ONNX inference via [fastembed-rs](https://github.com/Anush008/fastembed-rs). Default model: `all-MiniLM-L6-v2` (384 dimensions, ~25MB). Downloads on first use, cached thereafter. No API keys or external services required. |
| **OpenAI-compatible** | `openai-compatible` | Calls any endpoint implementing the OpenAI `/v1/embeddings` API: OpenAI, Azure OpenAI, Ollama, vLLM, LiteLLM, etc. |
| **None** | `none` | Pass-through mode. No embeddings are computed. Callers must supply embeddings in requests or use keyword-only search. |

**Environment variables:**

```bash
# Local provider (default — no configuration needed)
KD6_EMBEDDING_PROVIDER=local

# OpenAI-compatible remote provider
KD6_EMBEDDING_PROVIDER=openai-compatible
KD6_EMBEDDING_ENDPOINT=https://api.openai.com/v1    # required
KD6_EMBEDDING_MODEL=text-embedding-3-small           # required
KD6_EMBEDDING_API_KEY=sk-...                         # optional (not needed for Ollama)
KD6_EMBEDDING_DIMENSIONS=1536                        # optional (default: 1536)

# Disable embedding
KD6_EMBEDDING_PROVIDER=none
```

**Behavior:**
- On write: if the request omits `embedding`, the configured provider computes it from `content` before storing. Caller-provided embeddings are used as-is but validated for correct dimensionality.
- On search: if the request omits `embedding`, the provider computes a query embedding from `query` before performing vector similarity search.
- On update: if `content` changes and no new `embedding` is provided, the provider recomputes the embedding.
- Dimensionality mismatches between caller-provided embeddings and the configured model are rejected with `400 Bad Request`.

### 8.9 Framework Integration Pattern

The client-side/server-side embedding design enables a clean integration pattern for agent frameworks. Because the OMS service handles embedding, client adapters do not need an embedding model — they are thin HTTP clients that map framework APIs to OMS REST calls.

**Example: LangChain VectorStore adapter**

LangChain's `VectorStore` interface expects `add_texts(texts)` to handle vectorization and `similarity_search(query)` to return relevant documents. With server-side embedding, the adapter simply forwards text to the OMS API:

```python
from langchain_core.vectorstores import VectorStore

class KD6VectorStore(VectorStore):
    def __init__(self, base_url="http://localhost:8080", embedding=None):
        # embedding parameter is optional — server handles it by default
        self._base_url = base_url
        self._embedding = embedding  # client-side override (optional)

    def add_texts(self, texts, metadatas=None, **kwargs):
        entries = [{"content": text, ...} for text in texts]
        # No embedding computation needed — KD6 does it server-side
        return self._post("/memories/batch", {"entries": entries})

    def similarity_search(self, query, k=4, **kwargs):
        # Just send the query string — KD6 embeds and searches
        return self._post("/search", {"query": query, "top_k": k})
```

This pattern applies to any framework: the adapter maps the framework's text-based API to OMS REST calls, and the server handles embedding transparently. The optional `embedding` parameter allows advanced callers to provide pre-computed vectors when they need control over the embedding model.

---

## 9. Backend Provider Interface (SPI)

For customers who want to implement their own memory backend, the spec defines a **Service Provider Interface (SPI):**

```python
class OMSProvider(ABC):
    """Interface that backend implementors must fulfill.
    
    Implementors declare which capabilities they support via the
    capabilities() method. The platform gracefully degrades when
    a backend doesn't support an optional feature.
    """

    # --- Store Management ---
    @abstractmethod
    def create_store(self, config: StoreConfig) -> Store: ...
    @abstractmethod
    def get_store(self, store_id: str) -> Store: ...
    @abstractmethod
    def delete_store(self, store_id: str) -> None: ...

    # --- Memory CRUD ---
    @abstractmethod
    def put(self, store_id: str, entry: MemoryEntry) -> MemoryEntry:
        """Create a memory entry. If entry.upsert_key is set and a matching
        entry exists in the same store, layer, and scope, the existing entry
        is replaced atomically (see 4.3.2)."""
        ...
    @abstractmethod
    def get(self, store_id: str, memory_id: str) -> MemoryEntry: ...
    @abstractmethod
    def update(self, store_id: str, memory_id: str, patch: MemoryPatch) -> MemoryEntry: ...
    @abstractmethod
    def delete(self, store_id: str, memory_id: str) -> AuditRecord: ...
    @abstractmethod
    def list(self, store_id: str, filters: Filters) -> Page[MemoryEntry]: ...

    # --- Search ---
    @abstractmethod
    def search(self, store_id: str, query: SearchQuery) -> list[SearchResult]: ...

    # --- Optional: Graph Operations ---
    def graph_traverse(self, store_id: str, start_entity: str,
                       depth: int, filters: Filters) -> GraphResult | None: ...

    # --- Optional: Lifecycle ---
    def compact(self, store_id: str, layer: str, 
                policy: CompactionPolicy) -> CompactionResult | None: ...
    def archive(self, store_id: str, 
                criteria: ArchiveCriteria) -> ArchiveResult | None: ...
    def purge(self, store_id: str, scope: MemoryScope) -> PurgeResult: ...

    # --- Health & Capabilities ---
    @abstractmethod
    def stats(self, store_id: str) -> StoreStats: ...
    @abstractmethod
    def capabilities(self) -> ProviderCapabilities: ...
```

The `ProviderCapabilities` object declares what the backend supports:

```python
@dataclass
class ProviderCapabilities:
    supported_layers: list[str]               # e.g., ["working", "episodic", "semantic"]
    vector_search: bool                       # supports embedding-based search
    graph_support: bool                       # supports graph storage and traversal
    temporal_queries: bool                    # supports valid_from/valid_until filtering
    keyword_search: bool                      # supports BM25/full-text search
    max_embedding_dimensions: int | None      # e.g., 3072
    supported_distance_metrics: list[str]     # e.g., ["cosine", "euclidean", "dot"]
    compaction_support: bool                  # supports memory compaction
    archival_support: bool                    # supports layer-to-layer migration
    max_entry_size_bytes: int | None
    batch_operations: bool                    # supports batch put/delete
    pub_sub_notifications: bool               # supports real-time change notifications
    encryption_at_rest: bool
    audit_log: bool
    embedding: EmbeddingCapabilities | None   # server-side embedding support (see section 8)
```

This enables:
- The platform to route requests to the appropriate backend
- Graceful degradation when a backend doesn't support a feature (e.g., return `501 Not Implemented` for graph queries on a vector-only backend)
- Customers to start with a simple Level 1 implementation and add capabilities incrementally
- Third-party backend implementations to advertise their strengths

---

## 10. Data Sovereignty

Data sovereignty is a first-class concern in the OMS spec:

```yaml
SovereigntyConfig:
  mode: "strict" | "preferred" | "any"
  region: string                          # e.g., "westeurope", "eastus"
  replication:
    enabled: boolean
    target_regions: string[]              # for read replicas
    consistency: "strong" | "eventual"
```

**Rules:**
- `strict`: All memory data MUST reside in the specified region. Cross-region reads are served via proxy from the home region. Latency impact is accepted for compliance.
- `preferred`: Data resides in the specified region by default. Cross-region access is permitted for performance.
- `any`: No region constraint. Platform optimizes for latency.

**Cross-region parent-child scenarios:**
- Inherited memory is served via **read-only proxy** from the parent's home region
- Child's own working memory is stored in the child's local region (or parent's region, per policy)
- Bubble-up results are written to the parent's home region
- The system MUST log cross-region access events in the audit trail

---

## 11. Conformance Levels

To enable incremental adoption and community contribution, the spec defines three conformance levels:

### Level 1: Core (Minimum Viable Implementation)

An implementation at this level provides basic memory functionality:

- Memory store CRUD (create, get, update, delete, list)
- Memory entry CRUD (create, get, update, delete, list)
- Basic search (vector similarity with metadata filtering)
- Single-layer support (flat memory, no layer distinction required)
- Tenant isolation via scope (at minimum, `tenant_id` enforcement)
- Authentication (OAuth 2.1 bearer token)
- Capabilities discovery endpoint
- Plain string content support (see 4.3.1)
- Upsert semantics via `upsert_key` (see 4.3.2)

**Optional at Level 1:**
- Default store alias `_default` with auto-provisioning (see 4.1.1)
- Default tenant resolution (see 4.4.1)
- Server-side embedding via a configured `EmbeddingProvider` (see section 8). When not configured, callers must supply pre-computed embeddings for vector search. Keyword-only search remains available without an embedding provider.

These optional features lower the adoption barrier for single-agent and development scenarios. When both are enabled, an agent's first memory write requires zero setup calls. Implementations that support them MUST document their security implications and provide configuration to disable them in production.

**Estimated implementation effort:** 2–4 weeks for a team familiar with vector databases.

### Level 2: Standard

All of Level 1, plus:

- Multi-layer memory (working, episodic, semantic — minimum three layers)
- Memory lifecycle operations (TTL enforcement, compaction, archival)
- Parent-child memory inheritance (inherit + bubble-up)
- Shared memory spaces (blackboard pattern)
- Audit logging for all write operations
- Keyword/BM25 search
- Batch operations
- Hierarchical scoping (at minimum: tenant → agent → session)
- Server-side embedding with at least one built-in or configurable `EmbeddingProvider` (see section 8)

**Estimated implementation effort:** 2–3 months.

### Level 3: Advanced

All of Level 2, plus:

- Graph memory (entity storage, relationship traversal, temporal queries)
- Data sovereignty controls (region pinning, cross-region proxy)
- GDPR purge operations with compliance certification
- MCP server integration (expose memory as MCP resources/tools)
- A2A integration (inheritance via A2A Task artifacts)
- Real-time notifications (pub/sub for shared memory changes)
- Cross-region replication with configurable consistency
- All five memory layers supported
- Cryptographic audit trail integrity

**Estimated implementation effort:** 6–12 months.

---

## 12. Reference Implementation Guidance

A reference implementation targeting Azure SHOULD use the following technology mapping:

| OMS Component | Recommended OSS | Azure Service | Notes |
|--------------|----------------|---------------|-------|
| API Layer | FastAPI + gRPC | Azure Container Apps | Stateless, auto-scaling |
| Working Memory | Redis / Valkey | Azure Cache for Redis | Sub-ms latency, pub/sub for shared spaces |
| Episodic / Semantic (Vector) | Qdrant or Milvus | Cosmos DB for NoSQL (DiskANN) [8] | Partition key per tenant |
| Semantic (Graph) | Neo4j or Memgraph | Cosmos DB for Gremlin | Database per tenant |
| Archival | MinIO / S3 | Azure Blob Storage | Lifecycle policies for cost optimization |
| Metadata / Config | PostgreSQL | Cosmos DB for PostgreSQL | Tenant registry, store config |
| Audit Log | Append-only log | Azure Event Hubs → Data Explorer | Partitioned by tenant |
| Embedding | FastEmbed (ONNX) / Local model | Azure OpenAI (text-embedding-3-*) | Local default; remote override (see section 8.8) |
| Tenant Orchestration | Kubernetes + Helm | AKS (namespace-per-tenant) | Tiered isolation model |

---

## 13. Relationship to Existing Work

The OMS spec draws from and complements existing work:

| Source | What OMS Adopts | What OMS Adds |
|--------|----------------|---------------|
| **Mem0** [5][6] | Factory-pattern backend abstraction; multi-level scoping; vector+graph dual storage | Tenant isolation; parent-child inheritance; lifecycle management; capabilities discovery |
| **SAMEP** [7] | Security model (encryption, ACLs, audit trails); namespace isolation; protocol compatibility | Memory layers; pluggable backends; shared spaces; sovereignty; conformance levels |
| **Azure Foundry** [9] | Managed store lifecycle; scope-based isolation; SDK patterns | Multi-layer architecture; open specification; pluggable backends; graph/temporal memory |
| **Zep/Graphiti** [10] | Temporal metadata (valid_from/valid_until); bi-temporal querying | Integrated as optional temporal capability within semantic layer |
| **Letta/MemGPT** [11] | OS-inspired tiered memory; self-editing agent patterns | Formalized as memory layers with explicit lifecycle rules |
| **MemoryOS** [12] | Memory segmentation; paging; garbage collection concepts | Formalized as compaction and archival lifecycle operations |
| **CrewAI** [14] | Hierarchical delegation with memory; shared crew memory | Formalized as InheritanceSpec and SharedMemorySpace APIs |
| **memX** [13] | Redis-backed pub/sub shared memory; schema validation | Formalized as shared memory spaces with conflict resolution |
| **MCP** [18] | Agent-to-tool connectivity pattern | OMS memory server exposed as MCP resources/tools |
| **A2A** [19] | Agent-to-agent delegation pattern | OMS inheritance integrated with A2A Task artifacts |

---

## 14. References

[1] Tulving, E. (1972). "Episodic and Semantic Memory." In E. Tulving & W. Donaldson (Eds.), *Organization of Memory* (pp. 381–403). Academic Press.

[2] Baddeley, A.D. & Hitch, G.J. (1974). "Working Memory." In G.H. Bower (Ed.), *The Psychology of Learning and Motivation* (Vol. 8, pp. 47–89). Academic Press.

[3] Squire, L.R. (2004). "Memory Systems of the Brain: A Brief History and Current Perspective." *Neurobiology of Learning and Memory*, 82(3), 171–177.

[4] Atkinson, R.C. & Shiffrin, R.M. (1968). "Human Memory: A Proposed System and Its Control Processes." In K.W. Spence & J.T. Spence (Eds.), *The Psychology of Learning and Motivation* (Vol. 2, pp. 89–195). Academic Press.

[5] Mem0 GitHub Repository. https://github.com/mem0ai/mem0 (Apache 2.0 License, 48k+ stars as of May 2026).

[6] Chhikara, P., Khant, D., Aryan, S., Singh, T., & Yadav, D. (2025). "Mem0: Building Production-Ready AI Agents with Scalable Long-Term Memory." arXiv:2504.19413. https://arxiv.org/abs/2504.19413

[7] Masoor, H. (2025). "SAMEP: A Secure Protocol for Persistent Context Sharing Across AI Agents." arXiv:2507.10562. https://arxiv.org/abs/2507.10562

[8] Microsoft Research (2025). "Cost-Effective, Low Latency Vector Search with Azure Cosmos DB." arXiv:2505.05885. https://arxiv.org/abs/2505.05885

[9] Microsoft Learn. "Create and Use Memory — Microsoft Foundry Agent Service." https://learn.microsoft.com/en-us/azure/foundry/agents/how-to/memory-usage

[10] Preston, D. et al. (2025). "Zep: A Temporal Knowledge Graph Architecture for Agent Memory." arXiv:2501.13956. https://arxiv.org/abs/2501.13956

[11] Packer, C., Wooders, S., Lin, K., Fang, V., Patil, S.G., Stoica, I., & Gonzalez, J.E. (2023). "MemGPT: Towards LLMs as Operating Systems." arXiv:2310.08560. https://arxiv.org/abs/2310.08560

[12] "Memory OS of AI Agent." (2025). arXiv:2506.06326. https://arxiv.org/abs/2506.06326

[13] memX: Shared Memory for Multi-Agent LLM Systems. https://github.com/MehulG/memX

[14] CrewAI Documentation. https://docs.crewai.com/ | GitHub: https://github.com/crewAIInc/crewAI

[15] LangGraph GitHub Repository. https://github.com/langchain-ai/langgraph

[16] "Building Multi-Agent Systems with Shared Memory Guide." Hindsight/Vectorize (2026). https://hindsight.vectorize.io/guides/2026/04/21/guide-building-multi-agent-systems-with-shared-memory

[17] "Agent Memory Sharing Strategies: Blackboard, Message Passing, and Vector Stores." Callsphere (2026). https://callsphere.ai/blog/agent-memory-sharing-strategies-blackboard-message-passing-vector-stores

[18] Model Context Protocol (MCP) Specification. https://modelcontextprotocol.io/specification/ | GitHub: https://github.com/modelcontextprotocol/modelcontextprotocol

[19] A2A Protocol Specification. https://a2a-protocol.org/specification | GitHub: https://github.com/a2aproject/A2A
