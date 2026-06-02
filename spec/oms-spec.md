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
8. [Backend Provider Interface (SPI)](#8-backend-provider-interface-spi)
9. [Data Sovereignty](#9-data-sovereignty)
10. [Conformance Levels](#10-conformance-levels)
11. [Reference Implementation Guidance](#11-reference-implementation-guidance)
12. [Relationship to Existing Work](#12-relationship-to-existing-work)
13. [References](#13-references)

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
    embedding_model: string | null        # model for vector embeddings
    compaction_policy: CompactionPolicy | null
  metadata: map<string, string>           # arbitrary user metadata
  created_at: timestamp
  updated_at: timestamp
```

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
  content: string | object                # the memory content
  embedding: float[] | null               # vector representation (computed by service or provided)
  metadata:
    owner_agent_id: string                # which agent created this entry
    scope: MemoryScope                    # visibility and sharing scope
    tags: string[]                        # user-defined tags for filtering
    categories: string[]                  # system-defined categories
    source: SourceReference | null        # provenance (e.g., conversation ID, document ID)
    temporal:                             # optional temporal metadata (inspired by Zep [10])
      valid_from: timestamp | null        # when this fact became true
      valid_until: timestamp | null       # when this fact became false (null = still valid)
      confidence: float | null            # confidence score (0.0–1.0)
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

### 4.4 Memory Scope (Hierarchical)

Scopes define visibility boundaries and enable parent-child memory inheritance. The design follows a hierarchical model where more specific scopes inherit from broader ones:

```yaml
MemoryScope:
  tenant_id: string               # REQUIRED — the hard isolation boundary
  org_id: string | null           # organizational unit within tenant
  team_id: string | null          # team within org
  project_id: string | null       # project within team
  user_id: string | null          # end-user
  agent_id: string | null         # specific agent
  session_id: string | null       # specific conversation session
  run_id: string | null           # specific execution run
```

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
POST   /v1/stores/{store_id}/memories                # Create memory entry
GET    /v1/stores/{store_id}/memories/{memory_id}     # Get by ID
PATCH  /v1/stores/{store_id}/memories/{memory_id}     # Update entry (versioned)
DELETE /v1/stores/{store_id}/memories/{memory_id}     # Delete entry (audited)
GET    /v1/stores/{store_id}/memories                 # List with filters + pagination
```

### 5.3 Memory Search

```http
POST /v1/stores/{store_id}/search
Content-Type: application/json

{
  "query": "string",                    // natural language or structured query
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
- **Tenant context** — via `X-Tenant-ID` header or `tenant_id` claim in JWT
- **Agent identity** — via `X-Agent-ID` header or `agent_id` claim in JWT
- **Authentication** — OAuth 2.1 bearer token or mTLS

The service MUST enforce:
- **Tenant isolation** at the data layer — requests MUST NOT cross tenant boundaries under any circumstances
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

## 8. Backend Provider Interface (SPI)

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
    def put(self, store_id: str, entry: MemoryEntry) -> MemoryEntry: ...
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
```

This enables:
- The platform to route requests to the appropriate backend
- Graceful degradation when a backend doesn't support a feature (e.g., return `501 Not Implemented` for graph queries on a vector-only backend)
- Customers to start with a simple Level 1 implementation and add capabilities incrementally
- Third-party backend implementations to advertise their strengths

---

## 9. Data Sovereignty

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

## 10. Conformance Levels

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

## 11. Reference Implementation Guidance

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
| Embedding | Local model (e.g., Sentence Transformers) | Azure OpenAI (text-embedding-3-*) | Configurable per store |
| Tenant Orchestration | Kubernetes + Helm | AKS (namespace-per-tenant) | Tiered isolation model |

---

## 12. Relationship to Existing Work

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

## 13. References

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
