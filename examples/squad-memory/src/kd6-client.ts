/**
 * Kd6Client — Lightweight helper for KD6 REST API operations beyond
 * what the Squad MemoryProvider interface exposes.
 *
 * Used in the sprint demo to access graph edges, audit logs, lifecycle
 * management, and GDPR purge — advanced KD6 features that Squad's
 * governance layer doesn't currently surface.
 */

export interface Kd6ClientOptions {
  baseUrl?: string;
  store?: string;
  tenantId?: string;
}

export interface MemoryEntry {
  id: string;
  layer: string;
  content: string | Record<string, unknown>;
  owner_agent_id: string;
  tags: string[];
  scope: Record<string, string>;
  version: number;
  confidence?: number;
  valid_from?: string;
  valid_until?: string;
  entity_type?: string;
  created_at: string;
  updated_at: string;
  expires_at?: string;
}

export interface GraphEdge {
  id: string;
  source_memory_id: string;
  target_memory_id: string;
  relation_type: string;
  weight: number;
  metadata: Record<string, unknown>;
}

export interface AuditEntry {
  id: string;
  memory_id?: string;
  action: string;
  agent_id?: string;
  details?: Record<string, unknown>;
  created_at: string;
  redacted?: boolean;
}

export class Kd6Client {
  private readonly baseUrl: string;
  private readonly store: string;
  private readonly tenantId: string;

  constructor(options: Kd6ClientOptions = {}) {
    this.baseUrl = (options.baseUrl ?? "http://127.0.0.1:8080").replace(
      /\/$/,
      "",
    );
    this.store = options.store ?? "_default";
    this.tenantId = options.tenantId ?? "_default";
  }

  private headers(): Record<string, string> {
    return {
      "Content-Type": "application/json",
      "X-Tenant-Id": this.tenantId,
    };
  }

  private url(path: string): string {
    return `${this.baseUrl}/v1/stores/${this.store}${path}`;
  }

  // ── Store management ─────────────────────────────────────────────

  async ensureStore(): Promise<void> {
    // Try to reach the store; create it if it doesn't exist
    const res = await fetch(`${this.baseUrl}/v1/stores/${this.store}`, {
      headers: this.headers(),
    });
    if (res.ok) return;

    const create = await fetch(`${this.baseUrl}/v1/stores`, {
      method: "POST",
      headers: this.headers(),
      body: JSON.stringify({ name: this.store }),
    });
    if (!create.ok && create.status !== 409) {
      throw new Error(`store create failed (${create.status}): ${await create.text()}`);
    }
  }

  async deleteStore(): Promise<boolean> {
    const res = await fetch(`${this.baseUrl}/v1/stores/${this.store}`, {
      method: "DELETE",
      headers: this.headers(),
    });
    return res.ok;
  }

  // ── Memory CRUD ────────────────────────────────────────────────────

  async createMemory(body: Record<string, unknown>): Promise<MemoryEntry> {
    const res = await fetch(this.url("/memories"), {
      method: "POST",
      headers: this.headers(),
      body: JSON.stringify(body),
    });
    if (!res.ok) throw new Error(`create failed (${res.status}): ${await res.text()}`);
    return res.json() as Promise<MemoryEntry>;
  }

  async getMemory(id: string): Promise<MemoryEntry> {
    const res = await fetch(this.url(`/memories/${id}`), {
      headers: this.headers(),
    });
    if (!res.ok) throw new Error(`get failed (${res.status}): ${await res.text()}`);
    return res.json() as Promise<MemoryEntry>;
  }

  async deleteMemory(id: string): Promise<boolean> {
    const res = await fetch(this.url(`/memories/${id}`), {
      method: "DELETE",
      headers: this.headers(),
    });
    return res.ok;
  }

  // ── Graph ──────────────────────────────────────────────────────────

  async createEdge(
    sourceId: string,
    targetId: string,
    relationType: string,
    metadata: Record<string, unknown> = {},
  ): Promise<GraphEdge> {
    const res = await fetch(this.url("/graph/edges"), {
      method: "POST",
      headers: this.headers(),
      body: JSON.stringify({
        source_memory_id: sourceId,
        target_memory_id: targetId,
        relation_type: relationType,
        metadata,
      }),
    });
    if (!res.ok) throw new Error(`edge failed (${res.status}): ${await res.text()}`);
    return res.json() as Promise<GraphEdge>;
  }

  async traverse(
    startId: string,
    depth = 2,
    relationTypes?: string[],
  ): Promise<{ nodes: MemoryEntry[]; edges: GraphEdge[] }> {
    const body: Record<string, unknown> = { start_memory_id: startId, depth };
    if (relationTypes) body.relation_types = relationTypes;

    const res = await fetch(this.url("/graph/traverse"), {
      method: "POST",
      headers: this.headers(),
      body: JSON.stringify(body),
    });
    if (!res.ok) throw new Error(`traverse failed (${res.status}): ${await res.text()}`);
    return res.json() as Promise<{ nodes: MemoryEntry[]; edges: GraphEdge[] }>;
  }

  // ── Audit ──────────────────────────────────────────────────────────

  async auditLog(filter: Record<string, unknown> = {}): Promise<{ items: AuditEntry[] }> {
    const params = new URLSearchParams();
    for (const [k, v] of Object.entries(filter)) {
      if (v != null) params.set(k, String(v));
    }
    const qs = params.toString();
    const res = await fetch(this.url(`/audit${qs ? `?${qs}` : ""}`), {
      headers: this.headers(),
    });
    if (!res.ok) throw new Error(`audit failed (${res.status}): ${await res.text()}`);
    return res.json() as Promise<{ items: AuditEntry[] }>;
  }

  // ── Lifecycle ──────────────────────────────────────────────────────

  async purgeExpired(): Promise<{ deleted: number }> {
    const res = await fetch(this.url("/expired"), {
      method: "DELETE",
      headers: this.headers(),
    });
    if (!res.ok) throw new Error(`purge failed (${res.status}): ${await res.text()}`);
    return res.json() as Promise<{ deleted: number }>;
  }

  // ── GDPR ───────────────────────────────────────────────────────────

  async gdprPurge(scope: Record<string, string>): Promise<{ deleted: number }> {
    const res = await fetch(this.url("/gdpr/purge"), {
      method: "POST",
      headers: this.headers(),
      body: JSON.stringify(scope),
    });
    if (!res.ok) throw new Error(`gdpr failed (${res.status}): ${await res.text()}`);
    return res.json() as Promise<{ deleted: number }>;
  }

  // ── Search ─────────────────────────────────────────────────────────

  async search(
    query: string,
    options: { keyword?: boolean; top_k?: number } = {},
  ): Promise<Array<{ entry: MemoryEntry; score: number }>> {
    const res = await fetch(this.url("/search"), {
      method: "POST",
      headers: this.headers(),
      body: JSON.stringify({
        query,
        keyword: options.keyword ?? true,
        top_k: options.top_k ?? 20,
      }),
    });
    if (!res.ok) return [];
    return res.json() as Promise<Array<{ entry: MemoryEntry; score: number }>>;
  }
}
