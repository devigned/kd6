"""Configuration for the CrewAI + KD6 example.

Loads settings from environment variables with sensible defaults.
Uses GitHub Models for both LLM inference and embeddings.
"""

from __future__ import annotations

import os
import sys


def _require_env(name: str) -> str:
    val = os.environ.get(name)
    if not val:
        # Try gh CLI as fallback
        if name == "GITHUB_TOKEN":
            import subprocess

            try:
                result = subprocess.run(
                    ["gh", "auth", "token"],
                    capture_output=True,
                    text=True,
                    timeout=5,
                )
                if result.returncode == 0 and result.stdout.strip():
                    val = result.stdout.strip()
                    os.environ[name] = val
                    return val
            except (FileNotFoundError, subprocess.TimeoutExpired):
                pass

        print(f"Error: {name} environment variable is required.", file=sys.stderr)
        print(f"Set it in your .env file or run: gh auth login --scopes models:read", file=sys.stderr)
        sys.exit(1)
    return val


# ── GitHub Models configuration ──────────────────────────────────────────

GITHUB_TOKEN: str = _require_env("GITHUB_TOKEN")
GITHUB_MODELS_URL: str = "https://models.github.ai/inference"
CHAT_MODEL: str = os.environ.get("CHAT_MODEL", "openai/gpt-4o-mini")
EMBEDDING_MODEL: str = os.environ.get("EMBEDDING_MODEL", "openai/text-embedding-3-small")

# ── KD6 configuration ────────────────────────────────────────────────────

KD6_URL: str = os.environ.get("KD6_URL", "http://localhost:8080")
KD6_STORE_NAME: str = os.environ.get("KD6_STORE_NAME", "crewai-incident-response")
KD6_TENANT_ID: str = os.environ.get("KD6_TENANT_ID", "crewai-demo")


def get_crewai_llm():
    """Create a CrewAI LLM instance configured for GitHub Models."""
    from crewai import LLM

    return LLM(
        model=CHAT_MODEL,
        base_url=GITHUB_MODELS_URL,
        api_key=GITHUB_TOKEN,
    )


def get_crewai_embedder_config() -> dict:
    """Return the embedder config dict for CrewAI Memory.

    CrewAI's Memory class accepts an embedder config that it passes to
    its internal embedding factory. This configures it to use GitHub Models'
    OpenAI-compatible embedding endpoint.
    """
    return {
        "provider": "openai",
        "config": {
            "model": EMBEDDING_MODEL,
            "api_key": GITHUB_TOKEN,
            "api_base": GITHUB_MODELS_URL,
        },
    }
