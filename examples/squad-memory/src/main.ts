/**
 * Squad + KD6 Memory Example
 *
 * Demonstrates using KD6 as a registered MemoryProvider with Squad's
 * LocalMemoryStore. All memory operations flow through Squad's governance
 * layer (classification, audit) and are persisted to KD6.
 *
 * Prerequisites:
 *   KD6 server running at http://127.0.0.1:8080
 *   See README.md for setup instructions.
 */

import {
  LocalMemoryStore,
  ensureMemoryGovernanceDefaults,
} from "@bradygaster/squad-sdk/memory";
import { InMemoryStorageProvider } from "@bradygaster/squad-sdk/storage";
import { Kd6MemoryProvider } from "./kd6-provider.js";

function section(title: string) {
  console.log(`\n${"─".repeat(60)}`);
  console.log(`  ${title}`);
  console.log(`${"─".repeat(60)}`);
}

async function main() {
  // ── 1. Initialize KD6 provider and verify connectivity ───────────────
  section("1. Initialize KD6 provider");
  const kd6 = new Kd6MemoryProvider();
  const status = await kd6.status();
  if (!status.available) {
    console.error("KD6 is not available. Start the server first:");
    console.error("  make run-server   (from KD6 repo root)");
    process.exit(1);
  }
  console.log(`  ✓ ${status.name} is online`);

  // ── 2. Create Squad LocalMemoryStore with KD6 registered ─────────────
  section("2. Create Squad LocalMemoryStore with KD6");
  const storage = new InMemoryStorageProvider();
  const projectRoot = process.cwd();

  // Scaffold governance defaults (config.json, audit.jsonl, etc.)
  await ensureMemoryGovernanceDefaults(storage, projectRoot);

  // Create the store with KD6 as a registered provider
  const memoryStore = new LocalMemoryStore(storage, projectRoot, {
    registeredProviders: [kd6],
  });

  console.log("  ✓ LocalMemoryStore created with KD6 registered");
  console.log("    Governance: classify → audit → persist to KD6");

  // ── 3. Architect writes a DECISION ───────────────────────────────────
  section("3. Architect stores framework decision (governed write)");
  const fwResult = await memoryStore.write({
    content:
      "The project uses Next.js 14 with App Router and server components",
    title: "Framework choice",
    author: "squad-architect",
    requestedClass: "DECISION",
    approved: true,
  });
  console.log("  ✓ Governed write complete");
  console.log(`    Stored: ${fwResult.stored}, class: ${fwResult.classification.class}`);
  if (fwResult.id) console.log(`    ID: ${fwResult.id}`);

  // ── 4. Security agent writes a POLICY ────────────────────────────────
  section("4. Security agent stores validation policy");
  const polResult = await memoryStore.write({
    content: "All API endpoints must validate input with zod schemas",
    title: "Input validation policy",
    author: "squad-security",
    requestedClass: "POLICY",
    approved: true,
  });
  console.log("  ✓ Governed write complete");
  console.log(`    Stored: ${polResult.stored}, class: ${polResult.classification.class}`);
  if (polResult.id) console.log(`    ID: ${polResult.id}`);

  // ── 5. Search through the governed store ─────────────────────────────
  section("5. Search for project context via governed search");
  const results = await memoryStore.search("Next.js");
  console.log(`  Found ${results.length} result(s):`);
  for (const r of results) {
    console.log(`    • [${r.class}] ${r.snippet.slice(0, 80)}`);
    if (r.score != null) {
      console.log(
        `      score: ${r.score.toFixed(3)}, guidance: ${r.loadGuidance}`,
      );
    }
  }

  // ── 6. Verify KD6 persistence directly ───────────────────────────────
  section("6. Verify memories persisted in KD6 directly");
  const directResults = await kd6.search("validation");
  console.log(
    `  KD6 direct search for "validation": ${directResults.length} result(s)`,
  );
  for (const r of directResults) {
    console.log(`    • [${r.class}] ${r.snippet.slice(0, 80)}`);
  }

  // ── Summary ──────────────────────────────────────────────────────────
  section("Done");
  console.log("  This example demonstrated:");
  console.log(
    "    • Squad's LocalMemoryStore with KD6 as a registered provider",
  );
  console.log("    • Governed writes: classify → audit → persist to KD6");
  console.log("    • Governed search: Squad governance + KD6 FTS5 results");
  console.log(
    "    • Direct KD6 verification: memories persist in KD6's store\n",
  );
}

main().catch((err) => {
  console.error("Fatal error:", err);
  process.exit(1);
});
