"""Smoke test for the KD6 storage backend — no LLM calls required.

Validates that the Kd6StorageBackend can perform CRUD operations against
a running KD6 server. Run this before the full crew to verify connectivity.

Usage:
    # Start KD6 server first:
    KD6_DATABASE_URL="sqlite:kd6-test.db?mode=rwc" cargo run -p kd6-server

    # Then run smoke test:
    python -m src.smoke_test
"""

from __future__ import annotations

import os
import sys
from datetime import datetime, timezone
from pathlib import Path

# Load .env if present
_env_file = Path(__file__).parent.parent / ".env"
if _env_file.exists():
    for line in _env_file.read_text().splitlines():
        line = line.strip()
        if line and not line.startswith("#") and "=" in line:
            key, _, value = line.partition("=")
            os.environ.setdefault(key.strip(), value.strip())

from crewai.memory.types import MemoryRecord

from .kd6_backend import Kd6StorageBackend

KD6_URL = os.environ.get("KD6_URL", "http://localhost:8080")
STORE_NAME = "crewai-smoke-test"
TENANT_ID = "smoke-test"


def _ok(msg: str) -> None:
    print(f"  ✓ {msg}")


def _fail(msg: str) -> None:
    print(f"  ✗ {msg}", file=sys.stderr)
    sys.exit(1)


def main() -> None:
    print("KD6 Storage Backend Smoke Test")
    print(f"  Server: {KD6_URL}")
    print(f"  Store:  {STORE_NAME}")
    print()

    # 1. Create backend (ensures store exists)
    try:
        backend = Kd6StorageBackend(
            kd6_url=KD6_URL,
            store_name=STORE_NAME,
            tenant_id=TENANT_ID,
        )
        _ok("Connected to KD6 and ensured store exists")
    except Exception as e:
        _fail(f"Failed to connect to KD6: {e}")
        return

    # 2. Save records
    records = [
        MemoryRecord(
            id="smoke-1",
            content="Redis cache hit rate dropped from 95% to 62%",
            scope="/incident/test/infra",
            categories=["infrastructure", "redis"],
            importance=0.9,
            source="smoke-test",
        ),
        MemoryRecord(
            id="smoke-2",
            content="PostgreSQL connections at 450 out of 500 limit",
            scope="/incident/test/infra",
            categories=["infrastructure", "database"],
            importance=0.85,
            source="smoke-test",
        ),
        MemoryRecord(
            id="smoke-3",
            content="Root cause identified: connection pool leak in checkout service",
            scope="/incident/test/code",
            categories=["root-cause", "code"],
            importance=1.0,
            source="smoke-test",
            private=True,
        ),
    ]

    try:
        backend.save(records)
        _ok(f"Saved {len(records)} records")
    except Exception as e:
        _fail(f"Failed to save records: {e}")

    # 3. List records
    try:
        all_records = backend.list_records()
        assert len(all_records) >= 3, f"Expected >= 3 records, got {len(all_records)}"
        _ok(f"Listed {len(all_records)} records")
    except Exception as e:
        _fail(f"Failed to list records: {e}")

    # 4. Get single record
    try:
        rec = all_records[0]
        kd6_id = rec.metadata.get("_kd6_id")
        fetched = backend.get_record(kd6_id)
        assert fetched is not None, "get_record returned None"
        assert fetched.content, "Record has no content"
        _ok(f"Retrieved record: {fetched.content[:50]}...")
    except Exception as e:
        _fail(f"Failed to get record: {e}")

    # 5. Update record
    try:
        rec = all_records[0]
        rec.content = rec.content + " [UPDATED]"
        backend.update(rec)
        _ok("Updated record")
    except Exception as e:
        _fail(f"Failed to update record: {e}")

    # 6. Count records
    try:
        total = backend.count()
        assert total >= 3, f"Expected >= 3, got {total}"
        _ok(f"Count: {total} records total")
    except Exception as e:
        _fail(f"Failed to count: {e}")

    # 7. List scopes
    try:
        scopes = backend.list_scopes("/")
        assert len(scopes) > 0, "No scopes found"
        _ok(f"Scopes: {scopes}")
    except Exception as e:
        _fail(f"Failed to list scopes: {e}")

    # 8. List categories
    try:
        cats = backend.list_categories()
        assert len(cats) > 0, "No categories found"
        _ok(f"Categories: {cats}")
    except Exception as e:
        _fail(f"Failed to list categories: {e}")

    # 9. Delete specific records
    try:
        kd6_ids = [r.metadata["_kd6_id"] for r in all_records[:2]]
        deleted = backend.delete(record_ids=kd6_ids)
        assert deleted >= 2, f"Expected >= 2 deleted, got {deleted}"
        _ok(f"Deleted {deleted} records by ID")
    except Exception as e:
        _fail(f"Failed to delete: {e}")

    # 10. Reset (clean up)
    try:
        backend.reset()
        remaining = backend.count()
        _ok(f"Reset complete ({remaining} records remaining)")
    except Exception as e:
        _fail(f"Failed to reset: {e}")

    backend.close()

    print()
    print("All smoke tests passed! ✓")
    print("The KD6 backend is ready for the CrewAI example.")


if __name__ == "__main__":
    main()
