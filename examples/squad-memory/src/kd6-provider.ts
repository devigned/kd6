/**
 * Kd6MemoryProvider — A Squad MemoryProvider backed by KD6's HTTP API.
 *
 * Maps Squad's memory classes to KD6's memory layers:
 *
 *   Squad Class  →  KD6 Layer   →  Load Guidance
 *   ──────────────────────────────────────────────
 *   LOCAL        →  episodic    →  ON-DEMAND
 *   DECISION     →  semantic    →  ALWAYS
 *   POLICY       →  procedural  →  ALWAYS
 *
 * Registered as a provider with Squad's LocalMemoryStore, so all
 * governed memory operations (classify → write → search → delete)
 * flow through KD6 automatically.
 */

import type {
  MemoryClass,
  MemoryProvider,
  MemoryProviderStatus,
  MemoryProviderSearchResult,
  CopilotMemoryProviderWriteRequest,
  CopilotMemoryProviderWriteResult,
} from "@bradygaster/squad-sdk/memory";

const CLASS_TO_LAYER: Record<string, string> = {
  LOCAL: "episodic",
  DECISION: "semantic",
  POLICY: "procedural",
};

const LAYER_TO_CLASS: Record<string, MemoryClass> = {
  episodic: "LOCAL",
  semantic: "DECISION",
  procedural: "POLICY",
  working: "LOCAL",
};

const LAYER_TO_GUIDANCE: Record<string, string> = {
  episodic: "ON-DEMAND",
  semantic: "ALWAYS",
  procedural: "ALWAYS",
  working: "ON-DEMAND",
};

export interface Kd6ProviderOptions {
  /** KD6 server base URL. Default: http://127.0.0.1:8080 */
  baseUrl?: string;
  /** Store name. Default: _default (auto-provisioned). */
  store?: string;
  /** Tenant ID header. Default: _default. */
  tenantId?: string;
}

export class Kd6MemoryProvider implements MemoryProvider {
  readonly id = "kd6";
  readonly name = "KD6 Open Memory Service";
  readonly supportedClasses: ReadonlyArray<MemoryClass> = [
    "LOCAL",
    "DECISION",
    "POLICY",
  ];

  private readonly baseUrl: string;
  private readonly storeName: string;
  private readonly tenantId: string;

  constructor(options: Kd6ProviderOptions = {}) {
    this.baseUrl = (options.baseUrl ?? "http://127.0.0.1:8080").replace(
      /\/$/,
      "",
    );
    this.storeName = options.store ?? "_default";
    this.tenantId = options.tenantId ?? "_default";
  }

  private headers(): Record<string, string> {
    return {
      "Content-Type": "application/json",
      "X-Tenant-Id": this.tenantId,
    };
  }

  /** Base URL for store-scoped operations. */
  private storeUrl(): string {
    return `${this.baseUrl}/v1/stores/${this.storeName}`;
  }

  async status(): Promise<MemoryProviderStatus> {
    try {
      const res = await fetch(`${this.baseUrl}/health`);
      const body = (await res.json()) as { status: string };
      return {
        id: this.id,
        name: this.name,
        available: body.status === "ok",
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

  async write(
    request: CopilotMemoryProviderWriteRequest,
  ): Promise<CopilotMemoryProviderWriteResult> {
    const layer =
      CLASS_TO_LAYER[request.classification.class] ?? "working";
    const upsertKey = request.metadata?.["upsert_key"];

    const body: Record<string, unknown> = {
      content: request.content,
      owner_agent_id: request.author ?? "squad",
      layer,
      scope: {},
      tags: [request.classification.class.toLowerCase()],
    };
    if (upsertKey) body.upsert_key = upsertKey;

    const res = await fetch(`${this.storeUrl()}/memories`, {
      method: "POST",
      headers: this.headers(),
      body: JSON.stringify(body),
    });

    if (!res.ok) {
      const text = await res.text();
      throw new Error(`KD6 write failed (${res.status}): ${text}`);
    }

    const entry = (await res.json()) as { id: string };
    return {
      id: entry.id,
      path: `kd6:${this.storeName}:${entry.id}`,
    };
  }

  async search(query: string): Promise<MemoryProviderSearchResult[]> {
    const res = await fetch(`${this.storeUrl()}/search`, {
      method: "POST",
      headers: this.headers(),
      body: JSON.stringify({ query, keyword: true, top_k: 20 }),
    });

    if (!res.ok) return [];

    const results = (await res.json()) as Array<{
      entry: {
        id: string;
        layer: string;
        content: string | Record<string, unknown>;
        tags: string[];
      };
      score: number;
    }>;

    return results.map((r) => ({
      id: r.entry.id,
      title: r.entry.tags[0] ?? r.entry.layer,
      snippet:
        typeof r.entry.content === "string"
          ? r.entry.content.slice(0, 240)
          : JSON.stringify(r.entry.content).slice(0, 240),
      path: `kd6:${this.storeName}:${r.entry.id}`,
      class: (LAYER_TO_CLASS[r.entry.layer] ?? "LOCAL") as MemoryClass,
      loadGuidance: (LAYER_TO_GUIDANCE[r.entry.layer] ?? "ON-DEMAND") as
        | "ALWAYS"
        | "ON-DEMAND"
        | "ARCHIVE"
        | "NEVER",
      score: r.score,
    }));
  }

  async delete(id: string): Promise<boolean> {
    const res = await fetch(`${this.storeUrl()}/memories/${id}`, {
      method: "DELETE",
      headers: this.headers(),
    });
    return res.ok;
  }
}
