/**
 * Kd6MemoryProvider — Squad MemoryProvider backed by KD6's HTTP API.
 *
 * This file validates that Squad can integrate with KD6 using the
 * zero-setup API (default tenant, default store, upsert, plain strings).
 *
 * Usage:
 *   npx tsx validation/squad-kd6-provider.ts
 *
 * Requires a running KD6 server at http://127.0.0.1:18080
 */

// ── Squad types (inlined to avoid dependency) ──────────────────────────────

type MemoryClass = 'TRANSIENT' | 'LOCAL' | 'DECISION' | 'POLICY' | 'COPILOT_MEMORY' | 'FORBIDDEN';
type MemoryLoadGuidance = 'ALWAYS' | 'ON-DEMAND' | 'ARCHIVE' | 'NEVER';

interface MemoryClassification {
  class: MemoryClass;
  allowed: boolean;
  reason: string;
  destination: 'none' | 'local' | 'decision-inbox' | 'policy-inbox' | 'external-semantic';
  loadGuidance: MemoryLoadGuidance;
}

interface CopilotMemoryProviderWriteRequest {
  content: string;
  title: string;
  author?: string;
  metadata?: Record<string, string>;
  classification: MemoryClassification;
}

interface CopilotMemoryProviderWriteResult {
  id: string;
  path?: string;
}

interface MemoryProviderSearchResult {
  id: string;
  title: string;
  snippet: string;
  path?: string;
  class: MemoryClass;
  loadGuidance: MemoryLoadGuidance;
  score?: number;
}

interface MemoryProviderStatus {
  id: string;
  name: string;
  available: boolean;
  reason?: string;
}

interface MemoryProvider {
  readonly id: string;
  readonly name: string;
  readonly supportedClasses: ReadonlyArray<MemoryClass>;
  status(): Promise<MemoryProviderStatus>;
  write(request: CopilotMemoryProviderWriteRequest): Promise<CopilotMemoryProviderWriteResult>;
  search(query: string): Promise<MemoryProviderSearchResult[]>;
  delete(id: string): Promise<boolean>;
}

// ── KD6 layer mapping ──────────────────────────────────────────────────────

const CLASS_TO_LAYER: Record<string, string> = {
  LOCAL: 'episodic',
  DECISION: 'semantic',
  POLICY: 'procedural',
};

const LAYER_TO_CLASS: Record<string, MemoryClass> = {
  episodic: 'LOCAL',
  semantic: 'DECISION',
  procedural: 'POLICY',
  working: 'LOCAL',
};

const LAYER_TO_GUIDANCE: Record<string, MemoryLoadGuidance> = {
  episodic: 'ON-DEMAND',
  semantic: 'ALWAYS',
  procedural: 'ALWAYS',
  working: 'ON-DEMAND',
};

// ── Kd6MemoryProvider ──────────────────────────────────────────────────────

class Kd6MemoryProvider implements MemoryProvider {
  readonly id = 'kd6';
  readonly name = 'KD6 Open Memory Service';
  readonly supportedClasses: ReadonlyArray<MemoryClass> = ['LOCAL', 'DECISION', 'POLICY'];

  constructor(
    private readonly baseUrl: string = 'http://127.0.0.1:18080',
    private readonly store: string = '_default',
  ) {}

  async status(): Promise<MemoryProviderStatus> {
    try {
      const res = await fetch(`${this.baseUrl}/health`);
      const body = await res.json() as { status: string };
      return {
        id: this.id,
        name: this.name,
        available: body.status === 'ok',
      };
    } catch (err) {
      return {
        id: this.id,
        name: this.name,
        available: false,
        reason: String(err),
      };
    }
  }

  async write(request: CopilotMemoryProviderWriteRequest): Promise<CopilotMemoryProviderWriteResult> {
    const layer = CLASS_TO_LAYER[request.classification.class] ?? 'working';
    const upsertKey = request.metadata?.['upsert_key'];

    const body: Record<string, unknown> = {
      content: request.content,
      owner_agent_id: request.author ?? 'squad',
      layer,
      scope: {},
      tags: [request.classification.class.toLowerCase()],
    };
    if (upsertKey) body.upsert_key = upsertKey;

    const res = await fetch(`${this.baseUrl}/v1/stores/${this.store}/memories`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });

    if (!res.ok) {
      const text = await res.text();
      throw new Error(`KD6 write failed (${res.status}): ${text}`);
    }

    const entry = await res.json() as { id: string };
    return {
      id: entry.id,
      path: `kd6:${this.store}:${entry.id}`,
    };
  }

  async search(query: string): Promise<MemoryProviderSearchResult[]> {
    const res = await fetch(`${this.baseUrl}/v1/stores/${this.store}/search`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ query, keyword: true, top_k: 20 }),
    });

    if (!res.ok) return [];

    const results = await res.json() as Array<{
      entry: {
        id: string;
        layer: string;
        content: string;
        tags: string[];
      };
      score: number;
    }>;

    return results.map(r => ({
      id: r.entry.id,
      title: r.entry.tags[0] ?? r.entry.layer,
      snippet: typeof r.entry.content === 'string'
        ? r.entry.content.slice(0, 240)
        : JSON.stringify(r.entry.content).slice(0, 240),
      path: `kd6:${this.store}:${r.entry.id}`,
      class: LAYER_TO_CLASS[r.entry.layer] ?? 'LOCAL',
      loadGuidance: LAYER_TO_GUIDANCE[r.entry.layer] ?? 'ON-DEMAND',
      score: r.score,
    }));
  }

  async delete(id: string): Promise<boolean> {
    const res = await fetch(`${this.baseUrl}/v1/stores/${this.store}/memories/${id}`, {
      method: 'DELETE',
    });
    return res.ok;
  }
}

// ── Validation tests ───────────────────────────────────────────────────────

async function run() {
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

  // 1. Status check
  console.log('\n📋 Test: status()');
  const status = await provider.status();
  assert(status.available, 'KD6 is available');
  assert(status.id === 'kd6', 'Provider ID is kd6');

  // 2. Write LOCAL memory
  console.log('\n📋 Test: write LOCAL memory');
  const localResult = await provider.write({
    content: 'The project uses Next.js 14 with App Router',
    title: 'Framework choice',
    author: 'squad-architect',
    classification: {
      class: 'LOCAL',
      allowed: true,
      reason: 'episodic context',
      destination: 'local',
      loadGuidance: 'ON-DEMAND',
    },
  });
  assert(!!localResult.id, `Got memory ID: ${localResult.id}`);
  assert(localResult.path?.startsWith('kd6:'), `Path starts with kd6: ${localResult.path}`);

  // 3. Write DECISION memory
  console.log('\n📋 Test: write DECISION memory');
  const decisionResult = await provider.write({
    content: 'Use Tailwind CSS for styling instead of CSS modules',
    title: 'Styling decision',
    author: 'squad-ux-agent',
    classification: {
      class: 'DECISION',
      allowed: true,
      reason: 'architectural decision',
      destination: 'decision-inbox',
      loadGuidance: 'ALWAYS',
    },
  });
  assert(!!decisionResult.id, `Got DECISION ID: ${decisionResult.id}`);

  // 4. Write POLICY memory
  console.log('\n📋 Test: write POLICY memory');
  const policyResult = await provider.write({
    content: 'All API endpoints must validate input with zod schemas',
    title: 'Validation policy',
    author: 'squad-security',
    classification: {
      class: 'POLICY',
      allowed: true,
      reason: 'security policy',
      destination: 'policy-inbox',
      loadGuidance: 'ALWAYS',
    },
  });
  assert(!!policyResult.id, `Got POLICY ID: ${policyResult.id}`);

  // 5. Search
  console.log('\n📋 Test: search()');
  const searchResults = await provider.search('Tailwind CSS');
  assert(searchResults.length > 0, `Found ${searchResults.length} results for "Tailwind CSS"`);
  const tailwind = searchResults.find(r => r.snippet.includes('Tailwind'));
  assert(!!tailwind, 'Found the Tailwind decision');
  assert(tailwind?.class === 'DECISION', `Mapped class is DECISION (got ${tailwind?.class})`);
  assert(tailwind?.loadGuidance === 'ALWAYS', `Guidance is ALWAYS (got ${tailwind?.loadGuidance})`);

  // 6. Upsert (supersede pattern)
  console.log('\n📋 Test: upsert (supersede)');
  const v1 = await provider.write({
    content: 'Deploy to Azure App Service',
    title: 'Deploy target',
    author: 'squad-devops',
    metadata: { upsert_key: 'deploy-target' },
    classification: {
      class: 'DECISION',
      allowed: true,
      reason: 'deployment decision',
      destination: 'decision-inbox',
      loadGuidance: 'ALWAYS',
    },
  });

  const v2 = await provider.write({
    content: 'Deploy to Azure Container Apps instead',
    title: 'Deploy target',
    author: 'squad-devops',
    metadata: { upsert_key: 'deploy-target' },
    classification: {
      class: 'DECISION',
      allowed: true,
      reason: 'deployment decision updated',
      destination: 'decision-inbox',
      loadGuidance: 'ALWAYS',
    },
  });

  assert(v1.id === v2.id, `Upsert reused same ID: ${v1.id} === ${v2.id}`);

  const deploySearch = await provider.search('Container Apps');
  const containerEntry = deploySearch.find(r => r.snippet.includes('Container Apps'));
  assert(!!containerEntry, 'Upserted content is searchable');

  // 7. Delete
  console.log('\n📋 Test: delete()');
  const deleted = await provider.delete(policyResult.id);
  assert(deleted, `Deleted policy memory ${policyResult.id}`);

  const searchAfterDelete = await provider.search('zod schemas');
  const stillExists = searchAfterDelete.find(r => r.id === policyResult.id);
  assert(!stillExists, 'Deleted memory no longer appears in search');

  // Summary
  console.log(`\n${'═'.repeat(50)}`);
  console.log(`Results: ${passed} passed, ${failed} failed`);
  console.log(`${'═'.repeat(50)}\n`);

  process.exit(failed > 0 ? 1 : 0);
}

run().catch(err => {
  console.error('Fatal error:', err);
  process.exit(1);
});
