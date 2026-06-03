# Squad + KD6: Persistent Memory for Multi-Agent Teams

A multi-agent team builds a recipe-sharing app across three development
sessions. Every decision, policy, and investigation persists in
[KD6](../../README.md) — linked by a knowledge graph, auditable, and
GDPR-compliant. [Squad](https://github.com/bradygaster/squad)'s governance
layer classifies and audits every write before it reaches KD6.

Run `npm run sprint` to see a full sprint unfold in your terminal.

## Why KD6 instead of local files?

Squad stores team memory in `.squad/` — Markdown files committed to the
repo. That works for a single repo, but breaks down when you need:

- **Structured search** across hundreds of decisions (FTS5, vector similarity)
- **Knowledge graphs** that reveal how decisions relate to each other
- **Scoped visibility** so a contractor's notes don't leak to the whole team
- **Temporal awareness** — memories that expire, have confidence scores,
  or are valid only within a time window
- **Compliance** — cryptographic audit trails, GDPR purge that removes a
  user's data while preserving team knowledge
- **Cross-repo sharing** — multiple projects querying the same memory service

KD6 provides all of this through a standard REST API.

## Quick start

```bash
# Terminal 1 — start KD6 server
make run-server

# Terminal 2 — run the sprint simulation
cd examples/squad-memory
npm install
npm run sprint
```

## The sprint

The simulation creates a named KD6 store (`recipe-squad`) and runs three
sessions. Each session creates a fresh Squad `LocalMemoryStore` with KD6
registered as a provider — just as a real team would reconnect across
sessions.

### Session 1 — Kickoff

The team forms and sets foundational direction.

| Agent | Role | What they write | KD6 layer | KD6 feature |
|-------|------|----------------|-----------|-------------|
| **Hicks** | Architect | Next.js 14 with App Router | `semantic` | Governed write via Squad → KD6 |
| **Hicks** | Architect | PostgreSQL + Prisma ORM | `semantic` | Governed write via Squad → KD6 |
| **Lambert** | Security | Zod schema validation on all endpoints | `procedural` | Governed write via Squad → KD6 |
| **Lambert** | Security | NextAuth.js with OAuth providers | `procedural` | Governed write via Squad → KD6 |
| **Contractor** | Investigator | CSV import format investigation | `working` | Direct KD6 API with 60s TTL and `user_id` scope |

After the writes, KD6's graph API links the framework and database
decisions with a typed `constrains` edge — the Next.js data layer
patterns constrain which ORM makes sense.

**KD6 features demonstrated:**
- Squad governance (classify → audit → persist) routes `DECISION` to the
  `semantic` layer and `POLICY` to `procedural`
- Working memory with TTL (`expires_at`) for ephemeral investigation notes
- Per-user scoping (`scope.user_id`) for the contractor's data
- Graph edges with typed relations and metadata

### Session 2 — Build sprint

Agents search prior decisions before writing code — knowledge compounds.

**Dallas (Backend)** searches KD6 for "PostgreSQL database" before writing
any API code. He finds Hicks's database decision from Session 1, complete
with relevance score and load guidance (`ALWAYS` — this decision should be
in every agent's context). Confident in the stack, Dallas adds a pagination
convention and KD6 links it to the database decision with an `enables` edge.

**Ripley (Frontend)** searches for "Next.js React" and finds the framework
decision. She adds a component library convention (shadcn/ui + Tailwind).

**Dallas** then revises his pagination decision — after testing, the
default page size should be 50, not 20. KD6's **upsert** (`upsert_key`)
atomically replaces the old decision. One memory, one ID, updated content.

Finally, a **graph traversal** starting from the framework decision walks
all connected knowledge at depth 3:

```
framework —[constrains]→ database —[enables]→ pagination
```

Three decisions, linked by cause and effect. This is something flat files
can't do — you'd need to grep through Markdown and hope naming conventions
are consistent.

**KD6 features demonstrated:**
- Full-text search with relevance scoring (FTS5)
- Load guidance (`ALWAYS` vs `ON-DEMAND`) per memory class
- Upsert — atomic create-or-replace without duplicate memories
- Graph edge creation and BFS traversal with typed relations

### Session 3 — Compliance & retrospective

The sprint is done. Time for cleanup and compliance.

**Lifecycle management:** KD6 purges expired working memory. The
contractor's investigation note from Session 1 had a 60-second TTL —
`DELETE /expired` removes it. In production, working memory might live
for hours; archival memory lives indefinitely.

**Audit trail:** Every memory operation across all three sessions is
recorded in a cryptographic hash chain. The demo queries the full store
audit (who did what, when) and then drills into the database decision's
specific history. Each audit entry links to the memory it affected, the
agent that acted, and the operation performed.

**GDPR purge:** The contractor's engagement has ended. A single call to
`POST /gdpr/purge` with `{ "user_id": "contractor-chen" }` removes all
their memories. KD6 anonymizes the corresponding audit entries but
preserves the hash chain — you can still verify the audit trail's
integrity without exposing the purged user's data. Team decisions
survive untouched.

**KD6 features demonstrated:**
- TTL-based lifecycle management (`expires_at` + `DELETE /expired`)
- Cryptographic audit trail (SHA-256 hash chain)
- Per-memory audit history
- GDPR purge with audit anonymization
- Scoped deletion — only the targeted user's data is removed

## How the integration works

### Squad governance layer

```
Agent writes memory
    ↓
Squad LocalMemoryStore.write()
    ↓
Classify: DECISION / POLICY / LOCAL / TRANSIENT / FORBIDDEN
    ↓
Audit: log to governance audit trail
    ↓
Route to registered providers (KD6)
    ↓
Kd6MemoryProvider.write()
    ↓
POST /v1/stores/{store_name}/memories
```

Squad's governance ensures only classified, non-forbidden, non-transient
content reaches KD6. The `Kd6MemoryProvider` maps Squad's memory classes
to KD6's memory layers:

| Squad class | KD6 layer    | Load guidance | Why |
|-------------|-------------|---------------|-----|
| `DECISION`  | `semantic`  | `ALWAYS`      | Architectural decisions every agent should know |
| `POLICY`    | `procedural`| `ALWAYS`      | Team-wide rules and constraints |
| `LOCAL`     | `episodic`  | `ON-DEMAND`   | Session-specific context, loaded when relevant |

### Advanced KD6 features (beyond Squad's provider interface)

The sprint simulation also calls KD6's REST API directly for features
that Squad's `MemoryProvider` interface doesn't currently expose:

| Feature | API | What it does |
|---------|-----|-------------|
| Graph edges | `POST /graph/edges` | Link memories with typed, weighted relations |
| Graph traversal | `POST /graph/traverse` | BFS walk from any memory to discover connected knowledge |
| Audit trail | `GET /audit` | Query the cryptographic hash chain of all operations |
| Lifecycle | `DELETE /expired` | Purge memories past their TTL |
| GDPR purge | `POST /gdpr/purge` | Remove a user's data and anonymize audit entries |
| Scoped memory | `scope.user_id` on write | Isolate memory visibility by user, team, project, etc. |
| TTL | `expires_at` on write | Automatically expire ephemeral working memory |

These are accessed via `Kd6Client` (`src/kd6-client.ts`), a lightweight
fetch wrapper separate from the Squad provider.

## Other scripts

| Script | What it does |
|--------|-------------|
| `npm start` | Minimal quickstart — 2 governed writes + search |
| `npm run validate` | 12-assertion test suite against a live KD6 server |
| `npm run sprint` | Full sprint simulation (this README describes it) |

## Prerequisites

- Rust toolchain ([rustup.rs](https://rustup.rs))
- Node.js ≥ 18
- KD6 repository (you're in it)

The Squad SDK is installed automatically from
[devigned/squad#sdk-dist](https://github.com/devigned/squad/tree/sdk-dist)
via `npm install`.

## Files

```
src/
├── kd6-provider.ts  # Squad MemoryProvider → KD6 REST API
├── kd6-client.ts    # Direct KD6 API helper (graph, audit, lifecycle, GDPR)
├── sprint.ts        # Multi-session sprint simulation
├── main.ts          # Minimal governed memory quickstart
└── validate.ts      # Provider validation suite (12 assertions)
```
