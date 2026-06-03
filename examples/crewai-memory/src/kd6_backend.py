"""KD6 storage backend for CrewAI's unified memory system.

Implements CrewAI's StorageBackend protocol by translating memory operations
into KD6 REST API calls. This bridges CrewAI's path-based scoping model with
KD6's structured OMS scope hierarchy.

Scope mapping:
    CrewAI scope paths are mapped to KD6's MemoryScope fields:
        /                         → tenant-level (no additional scope fields)
        /{org}                    → org_id
        /{org}/{team}             → org_id + team_id
        /{org}/{team}/{project}   → org_id + team_id + project_id
        /{org}/{team}/{project}/{user}  → ... + user_id
        etc. following: org > team > project > user > agent > session > run
"""

from __future__ import annotations

import logging
from datetime import datetime, timezone
from typing import Any
from uuid import UUID

import httpx

from crewai.memory.storage.backend import StorageBackend
from crewai.memory.types import MemoryRecord, ScopeInfo

logger = logging.getLogger(__name__)

# Maps CrewAI scope path segments to KD6 MemoryScope field names, in order.
_SCOPE_FIELDS = ("org_id", "team_id", "project_id", "user_id", "agent_id", "session_id", "run_id")


def _scope_path_to_kd6(path: str) -> dict[str, str]:
    """Convert a CrewAI scope path like '/crew/research' to KD6 MemoryScope fields."""
    parts = [p for p in path.strip("/").split("/") if p]
    scope: dict[str, str] = {}
    for i, part in enumerate(parts):
        if i < len(_SCOPE_FIELDS):
            scope[_SCOPE_FIELDS[i]] = part
    return scope


def _kd6_scope_to_path(scope: dict[str, Any]) -> str:
    """Convert KD6 MemoryScope fields back to a CrewAI scope path."""
    parts: list[str] = []
    for field in _SCOPE_FIELDS:
        val = scope.get(field)
        if val:
            parts.append(val)
        else:
            break  # Stop at first gap to maintain hierarchy
    return "/" + "/".join(parts) if parts else "/"


def _record_to_kd6_request(
    record: MemoryRecord,
    *,
    layer: str = "semantic",
    owner_agent_id: str = "crewai",
) -> dict[str, Any]:
    """Convert a CrewAI MemoryRecord to a KD6 CreateMemoryRequest body."""
    scope = _scope_path_to_kd6(record.scope)

    # If the scope has an agent_id, use it as owner; otherwise use the default
    effective_owner = scope.pop("agent_id", None) or owner_agent_id
    if "agent_id" not in scope and effective_owner != owner_agent_id:
        scope["agent_id"] = effective_owner

    body: dict[str, Any] = {
        "layer": layer,
        "content": {
            "text": record.content,
            "crewai_metadata": record.metadata,
        },
        "owner_agent_id": effective_owner,
        "scope": scope,
        "categories": record.categories,
        "access_control": {
            "policy": "private" if record.private else "public_read",
        },
        "confidence": record.importance,
        "upsert_key": record.id,
    }

    if record.embedding is not None:
        body["embedding"] = record.embedding

    if record.source:
        body["source"] = {"uri": record.source}

    return body


def _kd6_entry_to_record(entry: dict[str, Any]) -> MemoryRecord:
    """Convert a KD6 MemoryEntry JSON to a CrewAI MemoryRecord."""
    content_val = entry.get("content", "")
    crewai_metadata: dict[str, Any] = {}

    if isinstance(content_val, dict):
        text = content_val.get("text", "")
        crewai_metadata = content_val.get("crewai_metadata", {})
    elif isinstance(content_val, str):
        text = content_val
    else:
        text = str(content_val)

    scope_dict = entry.get("scope", {})
    scope_path = _kd6_scope_to_path(scope_dict)

    access = entry.get("access_control", {})
    is_private = access.get("policy", "private") == "private"

    source_ref = entry.get("source")
    source = source_ref.get("uri") if source_ref else None

    # Use upsert_key as the CrewAI record ID, falling back to KD6 UUID
    record_id = entry.get("upsert_key") or str(entry.get("id", ""))

    # Store KD6 UUID in metadata for future operations
    crewai_metadata["_kd6_id"] = str(entry.get("id", ""))
    crewai_metadata["_kd6_version"] = entry.get("version", 1)

    created_at_str = entry.get("created_at", "")
    try:
        created_at = datetime.fromisoformat(created_at_str.replace("Z", "+00:00"))
    except (ValueError, AttributeError):
        created_at = datetime.now(timezone.utc)

    return MemoryRecord(
        id=record_id,
        content=text,
        scope=scope_path,
        categories=entry.get("categories", []),
        metadata=crewai_metadata,
        importance=entry.get("confidence") or 0.5,
        created_at=created_at,
        last_accessed=datetime.now(timezone.utc),
        embedding=entry.get("embedding"),
        source=source,
        private=is_private,
    )


class Kd6StorageBackend:
    """CrewAI StorageBackend backed by a KD6 memory service.

    This adapter translates CrewAI's memory operations into KD6 REST API calls,
    enabling multi-agent crews to use KD6's layered, scoped, and graph-capable
    memory system.

    Args:
        kd6_url: Base URL of the KD6 server (e.g. "http://localhost:8080").
        store_name: Name of the KD6 store to use (auto-created if missing).
        tenant_id: Tenant identifier for isolation.
        default_layer: Default memory layer for new records.
        default_owner: Default owner_agent_id when scope has no agent segment.
    """

    def __init__(
        self,
        kd6_url: str = "http://localhost:8080",
        store_name: str = "crewai-memory",
        tenant_id: str = "crewai",
        default_layer: str = "semantic",
        default_owner: str = "crewai",
    ) -> None:
        self.kd6_url = kd6_url.rstrip("/")
        self.store_name = store_name
        self.tenant_id = tenant_id
        self.default_layer = default_layer
        self.default_owner = default_owner
        self._client = httpx.Client(
            base_url=self.kd6_url,
            headers={"X-Tenant-ID": self.tenant_id},
            timeout=30.0,
        )
        self._ensure_store()

    def _ensure_store(self) -> None:
        """Create the store if it doesn't exist."""
        r = self._client.get(f"/v1/stores/{self.store_name}")
        if r.status_code == 200:
            logger.info("Using existing KD6 store: %s", self.store_name)
            return
        if r.status_code == 404:
            r2 = self._client.post("/v1/stores", json={"name": self.store_name})
            if r2.status_code == 201:
                logger.info("Created KD6 store: %s", self.store_name)
                return
            # Handle race condition: another process created it
            if r2.status_code == 409:
                logger.info("Store already exists (race): %s", self.store_name)
                return
            r2.raise_for_status()
        r.raise_for_status()

    @property
    def _base(self) -> str:
        return f"/v1/stores/{self.store_name}"

    # ── StorageBackend protocol methods ──────────────────────────────────

    def save(self, records: list[MemoryRecord]) -> None:
        """Save records to KD6 via batch create with upsert."""
        if not records:
            return

        entries = [
            _record_to_kd6_request(r, layer=self.default_layer, owner_agent_id=self.default_owner)
            for r in records
        ]

        r = self._client.post(f"{self._base}/memories/batch", json={"entries": entries})
        r.raise_for_status()

        resp = r.json()
        errors = resp.get("errors", [])
        if errors:
            logger.warning("Batch save had %d errors: %s", len(errors), errors)

    def search(
        self,
        query_embedding: list[float],
        scope_prefix: str | None = None,
        categories: list[str] | None = None,
        metadata_filter: dict[str, Any] | None = None,
        limit: int = 10,
        min_score: float = 0.0,
    ) -> list[tuple[MemoryRecord, float]]:
        """Search KD6 for memories by vector similarity."""
        body: dict[str, Any] = {
            "query": "",
            "embedding": query_embedding,
            "top_k": limit,
            "threshold": min_score,
        }

        if scope_prefix:
            body["scope"] = _scope_path_to_kd6(scope_prefix)

        filters: dict[str, Any] = {}
        if categories:
            filters["categories"] = categories
        if metadata_filter and "owner_agent_id" in metadata_filter:
            filters["owner_agent_id"] = metadata_filter["owner_agent_id"]
        if filters:
            body["filters"] = filters

        r = self._client.post(f"{self._base}/search", json=body)
        r.raise_for_status()

        results: list[tuple[MemoryRecord, float]] = []
        for item in r.json():
            record = _kd6_entry_to_record(item["entry"])
            score = item.get("score", 0.0)
            results.append((record, score))

        return results

    def delete(
        self,
        scope_prefix: str | None = None,
        categories: list[str] | None = None,
        record_ids: list[str] | None = None,
        older_than: datetime | None = None,
        metadata_filter: dict[str, Any] | None = None,
    ) -> int:
        """Delete memories matching the given criteria."""
        if record_ids:
            # Resolve KD6 UUIDs from record IDs
            kd6_ids = self._resolve_kd6_ids(record_ids)
            if not kd6_ids:
                return 0
            r = self._client.post(
                f"{self._base}/memories/batch/delete",
                json={"memory_ids": kd6_ids},
            )
            r.raise_for_status()
            return r.json().get("deleted", 0)

        # For scope/category/age-based deletes, list then batch-delete
        records = self.list_records(scope_prefix=scope_prefix, limit=10000)

        to_delete: list[str] = []
        for rec in records:
            if categories and not any(c in rec.categories for c in categories):
                continue
            if older_than and rec.created_at >= older_than:
                continue
            kd6_id = rec.metadata.get("_kd6_id")
            if kd6_id:
                to_delete.append(kd6_id)

        if not to_delete:
            return 0

        r = self._client.post(
            f"{self._base}/memories/batch/delete",
            json={"memory_ids": to_delete},
        )
        r.raise_for_status()
        return r.json().get("deleted", 0)

    def update(self, record: MemoryRecord) -> None:
        """Update an existing memory record in KD6."""
        kd6_id = record.metadata.get("_kd6_id")
        if not kd6_id:
            # Fall back to save (upsert)
            self.save([record])
            return

        body: dict[str, Any] = {
            "content": {
                "text": record.content,
                "crewai_metadata": {
                    k: v for k, v in record.metadata.items() if not k.startswith("_kd6_")
                },
            },
            "categories": record.categories,
            "access_control": {
                "policy": "private" if record.private else "public_read",
            },
        }

        r = self._client.patch(f"{self._base}/memories/{kd6_id}", json=body)
        r.raise_for_status()

    def get_record(self, record_id: str) -> MemoryRecord | None:
        """Retrieve a single memory record by ID."""
        # Try as KD6 UUID first
        kd6_id = record_id
        r = self._client.get(f"{self._base}/memories/{kd6_id}")
        if r.status_code == 200:
            return _kd6_entry_to_record(r.json())
        if r.status_code == 404:
            # Search by upsert_key: list all and filter
            records = self.list_records(limit=10000)
            for rec in records:
                if rec.id == record_id:
                    return rec
            return None
        r.raise_for_status()
        return None

    def list_records(
        self,
        scope_prefix: str | None = None,
        limit: int = 200,
        offset: int = 0,
    ) -> list[MemoryRecord]:
        """List memory records, optionally filtered by scope prefix."""
        params: dict[str, Any] = {"limit": limit, "offset": offset}
        if scope_prefix:
            scope = _scope_path_to_kd6(scope_prefix)
            for key, val in scope.items():
                params[f"scope.{key}"] = val

        r = self._client.get(f"{self._base}/memories", params=params)
        r.raise_for_status()

        data = r.json()
        items = data.get("items", data) if isinstance(data, dict) else data
        return [_kd6_entry_to_record(entry) for entry in items]

    def get_scope_info(self, scope: str) -> ScopeInfo:
        """Get statistics about a specific scope."""
        records = self.list_records(scope_prefix=scope, limit=10000)
        cats: dict[str, int] = {}
        for rec in records:
            for cat in rec.categories:
                cats[cat] = cats.get(cat, 0) + 1

        return ScopeInfo(
            scope=scope,
            record_count=len(records),
            categories=cats,
            child_scopes=[],  # Would need additional queries
        )

    def list_scopes(self, parent: str = "/") -> list[str]:
        """List child scopes under a parent scope path."""
        records = self.list_records(scope_prefix=parent, limit=10000)
        scopes: set[str] = set()
        parent_depth = len([p for p in parent.strip("/").split("/") if p])
        for rec in records:
            parts = [p for p in rec.scope.strip("/").split("/") if p]
            if len(parts) > parent_depth:
                child = "/" + "/".join(parts[: parent_depth + 1])
                scopes.add(child)
        return sorted(scopes)

    def list_categories(self, scope_prefix: str | None = None) -> dict[str, int]:
        """List categories with counts, optionally filtered by scope."""
        records = self.list_records(scope_prefix=scope_prefix, limit=10000)
        cats: dict[str, int] = {}
        for rec in records:
            for cat in rec.categories:
                cats[cat] = cats.get(cat, 0) + 1
        return cats

    def count(self, scope_prefix: str | None = None) -> int:
        """Count records, optionally filtered by scope."""
        records = self.list_records(scope_prefix=scope_prefix, limit=10000)
        return len(records)

    def reset(self, scope_prefix: str | None = None) -> None:
        """Delete all records in the given scope."""
        deleted = self.delete(scope_prefix=scope_prefix)
        logger.info("Reset: deleted %d records (scope=%s)", deleted, scope_prefix)

    # ── Async variants ───────────────────────────────────────────────────

    async def asave(self, records: list[MemoryRecord]) -> None:
        self.save(records)

    async def asearch(
        self,
        query_embedding: list[float],
        scope_prefix: str | None = None,
        categories: list[str] | None = None,
        metadata_filter: dict[str, Any] | None = None,
        limit: int = 10,
        min_score: float = 0.0,
    ) -> list[tuple[MemoryRecord, float]]:
        return self.search(
            query_embedding, scope_prefix, categories, metadata_filter, limit, min_score
        )

    async def adelete(
        self,
        scope_prefix: str | None = None,
        categories: list[str] | None = None,
        record_ids: list[str] | None = None,
        older_than: datetime | None = None,
        metadata_filter: dict[str, Any] | None = None,
    ) -> int:
        return self.delete(scope_prefix, categories, record_ids, older_than, metadata_filter)

    # ── Graph helpers (not part of StorageBackend, but showcase KD6) ─────

    def create_edge(
        self,
        source_id: str,
        target_id: str,
        relation_type: str,
        *,
        weight: float = 1.0,
        metadata: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        """Create a typed edge between two memory entries in KD6's knowledge graph."""
        body: dict[str, Any] = {
            "source_memory_id": source_id,
            "target_memory_id": target_id,
            "relation_type": relation_type,
            "weight": weight,
        }
        if metadata:
            body["metadata"] = metadata

        r = self._client.post(f"{self._base}/graph/edges", json=body)
        r.raise_for_status()
        return r.json()

    def traverse_graph(
        self,
        start_id: str,
        relation_types: list[str] | None = None,
        max_depth: int = 2,
    ) -> list[dict[str, Any]]:
        """Traverse the knowledge graph from a starting memory entry."""
        body: dict[str, Any] = {
            "start_memory_id": start_id,
            "max_depth": max_depth,
        }
        if relation_types:
            body["relation_types"] = relation_types

        r = self._client.post(f"{self._base}/graph/traverse", json=body)
        r.raise_for_status()
        return r.json()

    # ── Internal helpers ─────────────────────────────────────────────────

    def _resolve_kd6_ids(self, record_ids: list[str]) -> list[str]:
        """Resolve CrewAI record IDs to KD6 UUIDs."""
        kd6_ids: list[str] = []
        for rid in record_ids:
            try:
                UUID(rid)
                kd6_ids.append(rid)
            except ValueError:
                # Not a UUID — search for it via listing
                records = self.list_records(limit=10000)
                for rec in records:
                    if rec.id == rid:
                        kd6_id = rec.metadata.get("_kd6_id")
                        if kd6_id:
                            kd6_ids.append(kd6_id)
                        break
        return kd6_ids

    def close(self) -> None:
        """Close the HTTP client."""
        self._client.close()
