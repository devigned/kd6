/**
 * Validation suite for the KD6 Squad MemoryProvider.
 *
 * Tests the Kd6MemoryProvider directly (bypassing governance) to verify
 * it correctly implements the Squad MemoryProvider interface against
 * a live KD6 server.
 *
 * Usage:  npm run validate
 * Requires KD6 server at http://127.0.0.1:8080
 */

import { Kd6MemoryProvider } from "./kd6-provider.js";
import type { CopilotMemoryProviderWriteRequest } from "@bradygaster/squad-sdk/memory";

const provider = new Kd6MemoryProvider();
let passed = 0;
let failed = 0;

function assert(condition: boolean, msg: string) {
  if (condition) {
    console.log(`  ✅ ${msg}`);
    passed++;
  } else {
    console.log(`  ❌ ${msg}`);
    failed++;
  }
}

async function run() {
  // 1. Status
  console.log("\n📋 status()");
  const status = await provider.status();
  assert(status.available, "KD6 is available");
  assert(status.id === "kd6", "Provider ID is kd6");

  // 2. Write LOCAL
  console.log("\n📋 write(LOCAL)");
  const local = await provider.write({
    content: "The project uses Next.js 14 with App Router",
    title: "Framework choice",
    author: "squad-architect",
    classification: {
      class: "LOCAL",
      allowed: true,
      reason: "episodic context",
      destination: "local",
      loadGuidance: "ON-DEMAND",
    },
  } as CopilotMemoryProviderWriteRequest);
  assert(!!local.id, `Got memory ID: ${local.id}`);
  assert(!!local.path?.startsWith("kd6:"), `Path: ${local.path}`);

  // 3. Write DECISION
  console.log("\n📋 write(DECISION)");
  const decision = await provider.write({
    content: "Use Tailwind CSS for styling instead of CSS modules",
    title: "Styling decision",
    author: "squad-ux-agent",
    classification: {
      class: "DECISION",
      allowed: true,
      reason: "architectural decision",
      destination: "decision-inbox",
      loadGuidance: "ALWAYS",
    },
  } as CopilotMemoryProviderWriteRequest);
  assert(!!decision.id, `Got DECISION ID: ${decision.id}`);

  // 4. Write POLICY
  console.log("\n📋 write(POLICY)");
  const policy = await provider.write({
    content: "All API endpoints must validate input with zod schemas",
    title: "Validation policy",
    author: "squad-security",
    classification: {
      class: "POLICY",
      allowed: true,
      reason: "security policy",
      destination: "policy-inbox",
      loadGuidance: "ALWAYS",
    },
  } as CopilotMemoryProviderWriteRequest);
  assert(!!policy.id, `Got POLICY ID: ${policy.id}`);

  // 5. Search
  console.log("\n📋 search()");
  const results = await provider.search("Tailwind CSS");
  assert(results.length > 0, `Found ${results.length} results`);
  const tailwind = results.find((r) => r.snippet.includes("Tailwind"));
  assert(!!tailwind, "Found the Tailwind decision");
  assert(
    tailwind?.class === "DECISION",
    `Class is DECISION (got ${tailwind?.class})`,
  );

  // 6. Upsert
  console.log("\n📋 upsert (supersede)");
  const v1 = await provider.write({
    content: "Deploy to Azure App Service",
    title: "Deploy target",
    author: "squad-devops",
    metadata: { upsert_key: "deploy-target-validate" },
    classification: {
      class: "DECISION",
      allowed: true,
      reason: "deployment decision",
      destination: "decision-inbox",
      loadGuidance: "ALWAYS",
    },
  } as CopilotMemoryProviderWriteRequest);
  const v2 = await provider.write({
    content: "Deploy to Azure Container Apps instead",
    title: "Deploy target",
    author: "squad-devops",
    metadata: { upsert_key: "deploy-target-validate" },
    classification: {
      class: "DECISION",
      allowed: true,
      reason: "deployment decision updated",
      destination: "decision-inbox",
      loadGuidance: "ALWAYS",
    },
  } as CopilotMemoryProviderWriteRequest);
  assert(v1.id === v2.id, `Upsert reused same ID: ${v1.id}`);

  // 7. Delete
  console.log("\n📋 delete()");
  const deleted = await provider.delete(policy.id);
  assert(deleted, `Deleted policy memory ${policy.id}`);

  const afterDelete = await provider.search("zod schemas");
  assert(
    !afterDelete.find((r) => r.id === policy.id),
    "Deleted memory gone from search",
  );

  // Summary
  console.log(`\n${"═".repeat(50)}`);
  console.log(`Results: ${passed} passed, ${failed} failed`);
  console.log(`${"═".repeat(50)}\n`);

  process.exit(failed > 0 ? 1 : 0);
}

run().catch((err) => {
  console.error("Fatal error:", err);
  process.exit(1);
});
