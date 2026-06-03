# CrewAI + KD6: Incident Response Simulation

A multi-agent incident response crew powered by [CrewAI](https://docs.crewai.com)
and [KD6](https://github.com/devigned/kd6), demonstrating how AI agents can
collaborate with persistent, layered, scoped memory.

## What Happens

Four AI agents work together to investigate and resolve a simulated production
incident at "ShopStream", a fictional e-commerce platform experiencing checkout
failures:

| Agent | Role | Memory Usage |
|-------|------|-------------|
| **Incident Commander** | Coordinates the response, maintains timeline | Stores triage decisions, reads all agent findings |
| **Platform Engineer** | Investigates infrastructure metrics | Stores infrastructure findings (DB connections, Redis, memory) |
| **Backend Developer** | Analyzes code behavior, identifies root cause | Reads infra findings, stores root cause analysis |
| **Communications Lead** | Writes status updates and postmortem | Reads all findings, produces final documentation |

### The Incident

The checkout API is returning 503 errors for ~30% of requests. Key signals:
- PostgreSQL connections at 450/500 (near exhaustion)
- Redis cache hit rate dropped from 95% to 62%
- Memory usage elevated from 55% to 78%
- No recent deployments

### How KD6 Memory Is Used

**Memory layers** organize information by purpose:
- `semantic` — extracted facts, metrics, hypotheses
- `episodic` — timeline events, investigation steps
- `working` — active scratchpad for in-progress reasoning

**Scoped visibility** controls who sees what:
- Each agent stores memories under `/incident/shopstream/{agent-scope}`
- The Incident Commander and Communications Lead can read across all scopes
- Private findings stay scoped to the investigating agent until shared

**Knowledge graph** (optional, via the backend's `create_edge()` helper):
- Links related findings: "Redis degradation" → "DB connection spike"
- Enables traversal queries: "What evidence supports this root cause?"

**Cross-session persistence** — run the simulation twice:
- First run: agents investigate from scratch
- Second run: agents recall findings from the first incident

## Prerequisites

1. **Rust toolchain** — to build and run KD6
2. **Python 3.11+** — for CrewAI
3. **GitHub Personal Access Token** — see [Creating a token](#creating-a-github-token) below

### Creating a GitHub token

The example uses [GitHub Models](https://github.com/marketplace/models) for both
LLM inference and embeddings. You need a **fine-grained** Personal Access Token
with the `Models: Read` account permission.

1. Go to [github.com/settings/tokens?type=beta](https://github.com/settings/tokens?type=beta)
   (Fine-grained tokens)
2. Click **Generate new token**
3. Give it a name (e.g. "KD6 CrewAI example") and set an expiration
4. Under **Account permissions**, find **Models** and set it to **Read**
5. Click **Generate token** and copy the value

Then copy the `.env` template and paste your token (or leave `GITHUB_TOKEN=`
blank to auto-detect from `gh auth token`):

```bash
cp .env.example .env
# Edit .env to set GITHUB_TOKEN if not using the gh CLI
```

## Setup

### 1. Start the KD6 server

From the repository root:

```bash
KD6_DATABASE_URL="sqlite:kd6-crewai.db?mode=rwc" \
KD6_EMBEDDING_PROVIDER=openai-compatible \
KD6_EMBEDDING_ENDPOINT=https://models.github.ai/inference \
KD6_EMBEDDING_MODEL=openai/text-embedding-3-small \
KD6_EMBEDDING_API_KEY=$GITHUB_TOKEN \
cargo run -p kd6-server
```

The server starts on `http://localhost:8080`.

### 2. Install Python dependencies

```bash
cd examples/crewai-memory
python3.12 -m venv .venv
source .venv/bin/activate   # or .venv\Scripts\activate on Windows
pip install -e .
```

### 3. Run the smoke test

Validates KD6 connectivity without using any LLM calls:

```bash
python -m src.smoke_test
```

Expected output:
```
KD6 Storage Backend Smoke Test
  Server: http://localhost:8080
  Store:  crewai-smoke-test

  ✓ Connected to KD6 and ensured store exists
  ✓ Saved 3 records
  ✓ Listed 3 records
  ✓ Retrieved record: Redis cache hit rate dropped...
  ✓ Updated record
  ✓ Count: 3 records total
  ✓ Scopes: ['/incident', '/incident/test']
  ✓ Categories: {'infrastructure': 2, 'redis': 1, ...}
  ✓ Deleted 2 records by ID
  ✓ Reset complete (0 records remaining)

All smoke tests passed! ✓
```

### 4. Run the incident response

```bash
python -m src.main
```

The crew runs through the incident sequentially. You'll see output like:

```
======================================================================
  ShopStream Incident Response Simulation
  Powered by CrewAI + KD6 Memory
======================================================================

Starting incident response...

 [17:30:01][INFO]: Working Agent: Incident Commander
 [17:30:01][INFO]: Starting Task: Review the incident alert...

> Entering new CrewAgentExecutor chain...
Thought: I need to analyze the incident metrics and establish a timeline...

Final Answer:
## Triage Report
1. Timeline: 14:23 UTC — automated alert on 5xx rate > 5%
2. Initial hypothesis: database connection pool exhaustion
...

 [17:30:15][INFO]: Working Agent: Platform Engineer
 [17:30:15][INFO]: Starting Task: Investigate the infrastructure...
...

 [17:30:32][INFO]: Working Agent: Backend Developer
 [17:30:32][INFO]: Starting Task: Analyze the application behavior...
...

 [17:30:48][INFO]: Working Agent: Communications Lead
 [17:30:48][INFO]: Starting Task: Synthesize all findings...
...

======================================================================
  INCIDENT RESPONSE COMPLETE
======================================================================

## Post-Incident Review: ShopStream Checkout Service
### Executive Summary
A connection pool leak in the checkout service caused cascading failures...
...

Memories persisted in KD6. Run again to see cross-session recall.
```

Each agent shows its chain-of-thought reasoning before producing a final answer.
The actual content varies per run since it's LLM-generated. A full run typically
takes 1–2 minutes depending on model response times.

## Architecture

```
┌──────────────────────────────────────────────┐
│                  CrewAI                      │
│  ┌────────────┐  ┌─────────────────────────┐ │
│  │ LLM (GPT)  │  │    Memory System        │ │
│  │ via GitHub │  │  ┌───────────────────┐  │ │
│  │ Models     │  │  │ Kd6StorageBackend │──┼─┼──► KD6 REST API
│  └────────────┘  │  └───────────────────┘  │ │    (localhost:8080)
│                  │  ┌───────────────────┐  │ │
│                  │  │ OpenAI Embedder   │──┼─┼──► GitHub Models
│                  │  │ (text-embed-3-sm) │  │ │    Embeddings API
│                  │  └───────────────────┘  │ │
│                  └─────────────────────────┘ │
└──────────────────────────────────────────────┘
         │
         ▼
┌──────────────────────────────────────────────┐
│               KD6 Server                     │
│  ┌────────────┐  ┌───────────────────────┐   │
│  │ REST API   │  │ SQLite + FTS5         │   │
│  │ (Axum)     │  │ Vector Search         │   │
│  └────────────┘  │ Knowledge Graph       │   │
│                  │ Audit Log             │   │
│  ┌────────────┐  └───────────────────────┘   │
│  │ Embeddings │──► GitHub Models             │
│  │ (OpenAI-   │    Embeddings API            │
│  │ compatible)│                              │
│  └────────────┘                              │
└──────────────────────────────────────────────┘
```

## Key Files

| File | Description |
|------|-------------|
| `src/kd6_backend.py` | `Kd6StorageBackend` — CrewAI StorageBackend adapter for KD6 |
| `src/config.py` | Environment configuration (GitHub Models, KD6 URL) |
| `src/main.py` | Incident response crew definition and runner |
| `src/smoke_test.py` | Non-LLM connectivity test for the KD6 backend |

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `GITHUB_TOKEN` | *(required)* | GitHub PAT with `models:read` scope |
| `CHAT_MODEL` | `openai/gpt-4o-mini` | Chat model for GitHub Models |
| `EMBEDDING_MODEL` | `openai/text-embedding-3-small` | Embedding model |
| `KD6_URL` | `http://localhost:8080` | KD6 server URL |
| `KD6_STORE_NAME` | `crewai-incident-response` | KD6 store name |
| `KD6_TENANT_ID` | `crewai-demo` | KD6 tenant for isolation |

## How the Backend Adapter Works

The `Kd6StorageBackend` translates between CrewAI's memory model and KD6's
OMS-compliant API:

| CrewAI Concept | KD6 Equivalent |
|----------------|----------------|
| `MemoryRecord.scope` (path: `/org/team`) | `MemoryScope` (structured: `org_id`, `team_id`) |
| `MemoryRecord.categories` | `categories` |
| `MemoryRecord.importance` (0.0–1.0) | `confidence` (temporal metadata) |
| `MemoryRecord.private` | `AccessControl.policy` (`private` vs `public_read`) |
| `MemoryRecord.source` | `SourceReference.uri` |
| `MemoryRecord.id` | `upsert_key` (enables idempotent saves) |
| `MemoryRecord.metadata` | Stored in `content.crewai_metadata` envelope |

The adapter also exposes KD6-specific features not in the StorageBackend
protocol:

- `create_edge()` — create typed graph edges between memories
- `traverse_graph()` — BFS traversal of the knowledge graph
