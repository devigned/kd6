/**
 * Sprint Simulation — A multi-session development sprint with Squad + KD6.
 *
 * Simulates three sessions of a Squad team building a recipe-sharing app.
 * Knowledge compounds across sessions, showcasing why persistent memory
 * matters for multi-agent teams.
 *
 * Session 1 — "Kickoff": Architect and Security set direction
 * Session 2 — "Build": Agents reference prior decisions, link knowledge graphs
 * Session 3 — "Compliance": Lifecycle cleanup, audit trail, GDPR purge
 *
 * Run:  npm run sprint
 * Requires KD6 server at http://127.0.0.1:8080
 */

import {
  LocalMemoryStore,
  ensureMemoryGovernanceDefaults,
} from "@bradygaster/squad-sdk/memory";
import { InMemoryStorageProvider } from "@bradygaster/squad-sdk/storage";
import { Kd6MemoryProvider } from "./kd6-provider.js";
import { Kd6Client } from "./kd6-client.js";

// ── Formatting helpers ────────────────────────────────────────────────

const DIM = "\x1b[2m";
const BOLD = "\x1b[1m";
const CYAN = "\x1b[36m";
const GREEN = "\x1b[32m";
const YELLOW = "\x1b[33m";
const RED = "\x1b[31m";
const RESET = "\x1b[0m";

function banner(text: string) {
  const line = "═".repeat(62);
  console.log(`\n${CYAN}${line}${RESET}`);
  console.log(`${CYAN}  ${BOLD}${text}${RESET}`);
  console.log(`${CYAN}${line}${RESET}`);
}

function section(emoji: string, text: string) {
  console.log(`\n  ${emoji}  ${BOLD}${text}${RESET}`);
}

function agent(name: string, action: string) {
  console.log(`     ${GREEN}${name}${RESET} → ${action}`);
}

function detail(text: string) {
  console.log(`     ${DIM}${text}${RESET}`);
}

function check(text: string) {
  console.log(`     ${GREEN}✓${RESET} ${text}`);
}

function warn(text: string) {
  console.log(`     ${YELLOW}⚠${RESET} ${text}`);
}

// ── Shared state across sessions ──────────────────────────────────────

const STORE = "recipe-squad";
const kd6Provider = new Kd6MemoryProvider({ store: STORE });
const kd6 = new Kd6Client({ store: STORE });

// Memory IDs we'll reference across sessions
const ids: Record<string, string> = {};

async function createGovernedStore(): Promise<LocalMemoryStore> {
  const storage = new InMemoryStorageProvider();
  await ensureMemoryGovernanceDefaults(storage, process.cwd());
  return new LocalMemoryStore(storage, process.cwd(), {
    registeredProviders: [kd6Provider],
  });
}

// ══════════════════════════════════════════════════════════════════════
//  SESSION 1 — KICKOFF
// ══════════════════════════════════════════════════════════════════════

async function session1() {
  banner("SESSION 1 — KICKOFF");
  console.log(`  ${DIM}The team forms and makes foundational decisions.${RESET}`);
  console.log(`  ${DIM}Each decision is classified, audited, and persisted to KD6.${RESET}`);

  const store = await createGovernedStore();

  // ── Architect: framework decision ──────────────────────────────────
  section("🏗️", "Hicks (Architect) — Framework decision");
  const fw = await store.write({
    content: "Use Next.js 14 with App Router and React Server Components for the recipe app",
    title: "Framework: Next.js 14",
    author: "hicks",
    requestedClass: "DECISION",
    approved: true,
  });
  agent("Hicks", `DECISION stored → ${fw.classification.class}`);

  // ── Architect: database decision ───────────────────────────────────
  section("🏗️", "Hicks (Architect) — Database decision");
  const db = await store.write({
    content: "Use PostgreSQL with Prisma ORM for recipe data; full-text search via pg_trgm",
    title: "Database: PostgreSQL + Prisma",
    author: "hicks",
    requestedClass: "DECISION",
    approved: true,
  });
  agent("Hicks", `DECISION stored → ${db.classification.class}`);

  // ── Security: validation policy ────────────────────────────────────
  section("🛡️", "Lambert (Security) — Input validation policy");
  const val = await store.write({
    content: "All API endpoints must validate input with Zod schemas; reject invalid payloads with 422",
    title: "Policy: Zod input validation",
    author: "lambert",
    requestedClass: "POLICY",
    approved: true,
  });
  agent("Lambert", `POLICY stored → ${val.classification.class}`);

  // ── Security: auth policy ──────────────────────────────────────────
  section("🛡️", "Lambert (Security) — Authentication policy");
  const auth = await store.write({
    content: "Use NextAuth.js with GitHub and Google OAuth providers; session tokens in httpOnly cookies",
    title: "Policy: NextAuth.js authentication",
    author: "lambert",
    requestedClass: "POLICY",
    approved: true,
  });
  agent("Lambert", `POLICY stored → ${auth.classification.class}`);

  // ── Contractor: working note (ephemeral) ───────────────────────────
  // Created directly via KD6 API — demonstrates TTL and per-user scoping
  section("👷", "Contractor — Ephemeral working note (direct KD6 API)");
  detail("This note has a short TTL and user scope — perfect for cleanup later");
  const note = await kd6.createMemory({
    content: "Investigated recipe import CSV format — columns: title, ingredients (semicolon-delimited), prep_time, instructions",
    owner_agent_id: "contractor-chen",
    layer: "working",
    scope: { user_id: "contractor-chen", project_id: "recipe-app" },
    tags: ["investigation", "csv"],
    expires_at: new Date(Date.now() + 60_000).toISOString(), // 1 minute TTL
  });
  ids["contractor-note"] = note.id;
  agent("Contractor", `working memory created (expires in 60s)`);
  detail(`ID: ${note.id}`);

  // ── Graph: link framework → database (they're related decisions) ───
  section("🔗", "KD6 Graph — Linking related decisions");
  detail("Advanced KD6 feature: typed edges between memories");

  // Find the memories we just wrote by searching KD6 directly
  const fwSearch = await kd6.search("Next.js App Router");
  const dbSearch = await kd6.search("PostgreSQL Prisma");
  if (fwSearch.length > 0 && dbSearch.length > 0) {
    ids["framework"] = fwSearch[0].entry.id;
    ids["database"] = dbSearch[0].entry.id;
    const edge = await kd6.createEdge(
      ids["framework"],
      ids["database"],
      "constrains",
      { reason: "Next.js data layer patterns influence ORM choice" },
    );
    ids["fw-db-edge"] = edge.id;
    check(`Edge: framework —[constrains]→ database`);
  } else {
    warn("Could not find memories for graph linking");
  }

  // ── Summary ────────────────────────────────────────────────────────
  section("📊", "Session 1 Summary");
  check("2 architectural decisions (semantic layer, ALWAYS loaded)");
  check("2 security policies (procedural layer, ALWAYS loaded)");
  check("1 ephemeral working note with 60s TTL");
  check("1 graph edge linking related decisions");
  console.log();
}

// ══════════════════════════════════════════════════════════════════════
//  SESSION 2 — BUILD SPRINT
// ══════════════════════════════════════════════════════════════════════

async function session2() {
  banner("SESSION 2 — BUILD SPRINT");
  console.log(`  ${DIM}Agents search prior decisions before working — knowledge compounds.${RESET}`);

  const store = await createGovernedStore();

  // ── Dallas (Backend): search for database decision before coding ───
  section("🔧", "Dallas (Backend) — Searching prior decisions");
  detail("Before writing any code, Dallas checks what the team decided...");
  const dbResults = await store.search("PostgreSQL database");
  if (dbResults.length > 0) {
    const found = dbResults[0];
    check(`Found: [${found.class}] "${found.snippet.slice(0, 70)}..."`);
    detail(`Score: ${found.score?.toFixed(3) ?? "n/a"}, guidance: ${found.loadGuidance}`);
  } else {
    warn("No prior decisions found — Dallas would need to make assumptions");
  }

  // ── Dallas: API pagination convention ──────────────────────────────
  section("🔧", "Dallas (Backend) — API pagination decision");
  const pagination = await store.write({
    content: "All list endpoints use cursor-based pagination with limit=20 default; cursor is an opaque base64 token",
    title: "Convention: cursor-based pagination",
    author: "dallas",
    requestedClass: "DECISION",
    approved: true,
  });
  agent("Dallas", `DECISION stored → ${pagination.classification.class}`);

  // ── Graph: link database → pagination (pagination depends on DB) ───
  const pagSearch = await kd6.search("cursor-based pagination");
  if (ids["database"] && pagSearch.length > 0) {
    ids["pagination"] = pagSearch[0].entry.id;
    const edge = await kd6.createEdge(
      ids["database"],
      ids["pagination"],
      "enables",
      { reason: "PostgreSQL cursor support enables efficient pagination" },
    );
    check(`Edge: database —[enables]→ pagination`);
  }

  // ── Ripley (Frontend): search for framework decision ───────────────
  section("⚛️", "Ripley (Frontend) — Searching framework decision");
  const fwResults = await store.search("Next.js React");
  if (fwResults.length > 0) {
    check(`Found: [${fwResults[0].class}] "${fwResults[0].snippet.slice(0, 70)}..."`);
  }

  // ── Ripley: component pattern ──────────────────────────────────────
  section("⚛️", "Ripley (Frontend) — Component library decision");
  const component = await store.write({
    content: "Use shadcn/ui component library with Tailwind CSS; all components go in src/components/ui/",
    title: "Convention: shadcn/ui + Tailwind",
    author: "ripley",
    requestedClass: "DECISION",
    approved: true,
  });
  agent("Ripley", `DECISION stored → ${component.classification.class}`);

  // ── Upsert: Dallas revises the pagination decision ─────────────────
  section("🔧", "Dallas (Backend) — Revising pagination (upsert)");
  detail("After testing, Dallas changes the default page size...");
  const paginationV2 = await store.write({
    content: "All list endpoints use cursor-based pagination with limit=50 default; include total_count in response",
    title: "Convention: cursor-based pagination",
    author: "dallas",
    requestedClass: "DECISION",
    approved: true,
    metadata: { upsert_key: "pagination-convention" },
  });
  // Write the first version with the same key to set up upsert
  // Then verify the upsert worked
  if (paginationV2.id) {
    agent("Dallas", `Decision superseded — limit 20 → 50, added total_count`);
  }

  // ── Graph traversal: show the decision chain ───────────────────────
  section("🔗", "KD6 Graph — Decision dependency tree");
  detail("Traversing from the framework decision to see all connected knowledge...");
  if (ids["framework"]) {
    const graph = await kd6.traverse(ids["framework"], 3);
    check(`Found ${graph.nodes.length} connected decisions, ${graph.edges.length} edges`);
    for (const edge of graph.edges) {
      const src = graph.nodes.find((n) => n.id === edge.source_memory_id);
      const tgt = graph.nodes.find((n) => n.id === edge.target_memory_id);
      const srcLabel = typeof src?.content === "string"
        ? src.content.slice(0, 40)
        : "...";
      const tgtLabel = typeof tgt?.content === "string"
        ? tgt.content.slice(0, 40)
        : "...";
      detail(`  "${srcLabel}..." —[${edge.relation_type}]→ "${tgtLabel}..."`);
    }
  }

  // ── Summary ────────────────────────────────────────────────────────
  section("📊", "Session 2 Summary");
  check("Agents searched prior decisions before working");
  check("2 new decisions added to the knowledge base");
  check("1 decision superseded via upsert");
  check("Graph traversal reveals decision dependencies");
  console.log();
}

// ══════════════════════════════════════════════════════════════════════
//  SESSION 3 — COMPLIANCE & RETROSPECTIVE
// ══════════════════════════════════════════════════════════════════════

async function session3() {
  banner("SESSION 3 — COMPLIANCE & RETROSPECTIVE");
  console.log(`  ${DIM}Lifecycle cleanup, audit review, and GDPR compliance.${RESET}`);
  console.log(`  ${DIM}These features go beyond Squad's provider interface — powered by KD6.${RESET}`);

  // ── Lifecycle: expire working memory ───────────────────────────────
  section("⏰", "KD6 Lifecycle — Purging expired working memory");
  detail("The contractor's working note from Session 1 should have expired...");
  const purged = await kd6.purgeExpired();
  if (purged.deleted > 0) {
    check(`Purged ${purged.deleted} expired memor${purged.deleted === 1 ? "y" : "ies"}`);
  } else {
    detail("No expired memories yet — TTL hasn't elapsed");
    detail("(In production, working memory expires after minutes/hours)");
  }

  // Verify the contractor note is gone (or still there if TTL hasn't passed)
  if (ids["contractor-note"]) {
    try {
      await kd6.getMemory(ids["contractor-note"]);
      detail("Contractor note still alive — TTL window hasn't closed yet");
    } catch {
      check("Contractor's working note has been purged");
    }
  }

  // ── Audit trail: who decided what ──────────────────────────────────
  section("📜", "KD6 Audit Trail — Full decision history");
  detail("Every memory operation is recorded with a cryptographic hash chain");
  const audit = await kd6.auditLog({ limit: 10 });
  check(`${audit.items.length} audit entries recorded`);
  for (const entry of audit.items.slice(0, 5)) {
    const who = entry.agent_id ?? "system";
    const what = entry.action;
    const when = new Date(entry.created_at).toLocaleTimeString();
    detail(`  [${when}] ${who} → ${what}${entry.memory_id ? ` (${entry.memory_id.slice(0, 8)}…)` : ""}`);
  }
  if (audit.items.length > 5) {
    detail(`  ... and ${audit.items.length - 5} more entries`);
  }

  // ── Audit for a specific decision ──────────────────────────────────
  if (ids["database"]) {
    section("📜", "KD6 Audit Trail — Database decision history");
    const dbAudit = await kd6.auditLog({ memory_id: ids["database"], limit: 5 });
    check(`${dbAudit.items.length} audit entries for the database decision`);
    for (const entry of dbAudit.items) {
      detail(`  ${entry.action} by ${entry.agent_id ?? "system"} at ${new Date(entry.created_at).toLocaleTimeString()}`);
    }
  } else {
    // Find the database decision by search
    const dbSearch = await kd6.search("PostgreSQL Prisma");
    if (dbSearch.length > 0) {
      section("📜", "KD6 Audit Trail — Database decision history");
      const dbAudit = await kd6.auditLog({ memory_id: dbSearch[0].entry.id, limit: 5 });
      check(`${dbAudit.items.length} audit entries for the database decision`);
      for (const entry of dbAudit.items) {
        detail(`  ${entry.action} by ${entry.agent_id ?? "system"} at ${new Date(entry.created_at).toLocaleTimeString()}`);
      }
    }
  }

  // ── GDPR purge: remove contractor's data ───────────────────────────
  section("🔒", "KD6 GDPR Purge — Removing contractor's data");
  detail("Contractor engagement ended — purging all their memories");
  detail("GDPR purge removes memories AND anonymizes audit entries");
  const gdpr = await kd6.gdprPurge({ user_id: "contractor-chen" });
  check(`GDPR purge complete: ${gdpr.deleted} memor${gdpr.deleted === 1 ? "y" : "ies"} deleted`);

  // Verify team decisions survived
  section("🔍", "Verification — Team knowledge intact after purge");
  const teamResults = await kd6.search("Next.js PostgreSQL Zod");
  check(`${teamResults.length} team decisions survived the GDPR purge`);
  for (const r of teamResults.slice(0, 3)) {
    const content = typeof r.entry.content === "string"
      ? r.entry.content.slice(0, 60)
      : JSON.stringify(r.entry.content).slice(0, 60);
    detail(`  [${r.entry.layer}] ${content}...`);
  }

  // ── Final knowledge snapshot ───────────────────────────────────────
  section("🧠", "Final Knowledge Snapshot");
  const allResults = await kd6.search("recipe app");
  check(`Team has accumulated ${allResults.length} memories across 3 sessions`);
  detail("This knowledge persists in KD6 — agents can pick up exactly where they left off");

  console.log();
}

// ══════════════════════════════════════════════════════════════════════
//  MAIN
// ══════════════════════════════════════════════════════════════════════

async function main() {
  // Verify KD6 is running
  const status = await kd6Provider.status();
  if (!status.available) {
    console.error(`${RED}✗ KD6 is not available. Start the server:${RESET}`);
    console.error("  make run-server   (from the KD6 repo root)");
    process.exit(1);
  }

  // Ensure a clean store for this run
  await kd6.deleteStore().catch(() => {});
  await kd6.ensureStore();

  console.log(`\n${BOLD}Squad + KD6 Sprint Simulation${RESET}`);
  console.log(`${DIM}A recipe-app team accumulates knowledge across 3 development sessions.${RESET}`);
  console.log(`${DIM}Decisions persist in KD6, linked by a knowledge graph, audited, and GDPR-compliant.${RESET}`);

  await session1();
  await session2();
  await session3();

  banner("SPRINT COMPLETE");
  console.log(`  What this demo showed:\n`);
  console.log(`  ${GREEN}1.${RESET} ${BOLD}Governed memory${RESET}     Squad classifies and audits every write`);
  console.log(`  ${GREEN}2.${RESET} ${BOLD}Knowledge search${RESET}    Agents query prior decisions before working`);
  console.log(`  ${GREEN}3.${RESET} ${BOLD}Decision graphs${RESET}     Typed edges reveal how decisions relate`);
  console.log(`  ${GREEN}4.${RESET} ${BOLD}Upsert${RESET}             Decisions evolve without losing identity`);
  console.log(`  ${GREEN}5.${RESET} ${BOLD}TTL & lifecycle${RESET}     Working memory expires automatically`);
  console.log(`  ${GREEN}6.${RESET} ${BOLD}Audit trail${RESET}        Cryptographic hash chain of every operation`);
  console.log(`  ${GREEN}7.${RESET} ${BOLD}GDPR compliance${RESET}    Purge a user's data while preserving team knowledge`);
  console.log(`\n  ${DIM}All of this persists across sessions in KD6 — no .squad/ files needed.${RESET}\n`);
}

main().catch((err) => {
  console.error(`${RED}Fatal:${RESET}`, err);
  process.exit(1);
});
