"""Incident Response Crew — a multi-agent simulation powered by KD6 memory.

This example demonstrates a 4-agent crew handling a production incident.
Each agent uses KD6 as persistent, shared memory through CrewAI's Memory system.

Scenario:
    A fictional e-commerce platform "ShopStream" is experiencing elevated error
    rates on its checkout service. The crew must investigate, diagnose, fix,
    and document the incident.

Agents:
    1. Incident Commander — coordinates response, tracks timeline, makes decisions
    2. Platform Engineer — investigates infrastructure, checks metrics and logs
    3. Backend Developer — analyzes code, identifies root cause, proposes fixes
    4. Communications Lead — drafts status updates and post-incident review

KD6 features demonstrated:
    - Memory layers: working (active investigation), semantic (extracted facts),
      episodic (incident timeline events)
    - Scoped visibility: each agent has its own scope, shared under /incident
    - Knowledge graph: edges link related findings (e.g., "log entry" → "root cause")
    - Cross-session persistence: run twice to see agents recall prior incidents
"""

from __future__ import annotations

import os
import sys
from pathlib import Path

# Load .env if present
_env_file = Path(__file__).parent.parent / ".env"
if _env_file.exists():
    for line in _env_file.read_text().splitlines():
        line = line.strip()
        if line and not line.startswith("#") and "=" in line:
            key, _, value = line.partition("=")
            os.environ.setdefault(key.strip(), value.strip())

from crewai import Agent, Crew, Process, Task

from .config import (
    KD6_STORE_NAME,
    KD6_TENANT_ID,
    KD6_URL,
    get_crewai_embedder_config,
    get_crewai_llm,
)
from .kd6_backend import Kd6StorageBackend

# ── Scenario context ─────────────────────────────────────────────────────

INCIDENT_BRIEF = """\
INCIDENT ALERT — ShopStream Checkout Service

Severity: SEV-1 (customer-facing)
Started: 14:23 UTC
Detection: Automated alerting on 5xx error rate > 5%

Symptoms:
- Checkout API returning HTTP 503 for ~30% of requests
- Payment processing timeouts increased from p99=200ms to p99=8000ms
- Cart abandonment rate spiked 4x in the last 15 minutes
- No recent deployments (last deploy was 6 hours ago)

Affected services: checkout-api, payment-gateway, order-service
Infrastructure: Kubernetes cluster (us-east-1), PostgreSQL (RDS), Redis (ElastiCache)

Initial metrics:
- checkout-api CPU: 45% (normal)
- checkout-api memory: 78% (elevated, usually 55%)
- PostgreSQL connections: 450/500 (near limit, usually 200)
- Redis hit rate: 62% (degraded, usually 95%)
"""


def build_crew() -> Crew:
    """Construct the incident response crew with KD6 memory."""
    llm = get_crewai_llm()

    # ── KD6 memory backend ───────────────────────────────────────────
    kd6_backend = Kd6StorageBackend(
        kd6_url=KD6_URL,
        store_name=KD6_STORE_NAME,
        tenant_id=KD6_TENANT_ID,
        default_layer="semantic",
        default_owner="incident-crew",
    )

    # ── Agents ───────────────────────────────────────────────────────

    incident_commander = Agent(
        role="Incident Commander",
        goal=(
            "Coordinate the incident response. Track the timeline of events, "
            "delegate investigation tasks, and make decisions about mitigation "
            "and resolution. Ensure all findings are documented."
        ),
        backstory=(
            "You are a senior SRE who has led dozens of production incidents. "
            "You know that clear communication and systematic investigation "
            "are key. You maintain a detailed timeline and ensure nothing "
            "falls through the cracks."
        ),
        llm=llm,
        verbose=True,
    )

    platform_engineer = Agent(
        role="Platform Engineer",
        goal=(
            "Investigate the infrastructure layer. Analyze metrics, check "
            "resource utilization, connection pools, and network issues. "
            "Report findings with specific data points."
        ),
        backstory=(
            "You are an infrastructure specialist who lives in Grafana dashboards "
            "and kubectl. You can read metrics patterns and correlate infrastructure "
            "events with application behavior. You always cite specific numbers."
        ),
        llm=llm,
        verbose=True,
    )

    backend_developer = Agent(
        role="Backend Developer",
        goal=(
            "Analyze the application code and behavior. Identify the root cause "
            "by correlating infrastructure findings with application patterns. "
            "Propose specific code or configuration fixes."
        ),
        backstory=(
            "You are a senior backend developer who wrote parts of the checkout "
            "service. You understand connection pooling, caching strategies, "
            "and common failure modes. You think in terms of code paths and "
            "data flows."
        ),
        llm=llm,
        verbose=True,
    )

    comms_lead = Agent(
        role="Communications Lead",
        goal=(
            "Draft clear, accurate status updates for stakeholders and a "
            "comprehensive post-incident review. Synthesize technical findings "
            "into business-relevant language."
        ),
        backstory=(
            "You are a technical program manager who bridges engineering and "
            "business. You distill complex technical incidents into clear "
            "narratives with actionable follow-ups. You always include "
            "timeline, impact, root cause, and remediation."
        ),
        llm=llm,
        verbose=True,
    )

    # ── Tasks (sequential investigation flow) ────────────────────────

    triage = Task(
        description=(
            f"Review the incident alert and perform initial triage.\n\n"
            f"{INCIDENT_BRIEF}\n\n"
            "Create a structured timeline of events so far. Identify the most "
            "likely failure domain (infrastructure, application, or external). "
            "Assign specific investigation areas to the team. "
            "Record your triage findings as structured observations."
        ),
        expected_output=(
            "A triage report containing:\n"
            "1. Incident timeline (what happened when)\n"
            "2. Initial hypothesis for the failure domain\n"
            "3. Specific investigation assignments\n"
            "4. Severity confirmation and escalation status"
        ),
        agent=incident_commander,
    )

    investigate_infra = Task(
        description=(
            "Investigate the infrastructure based on the triage report. "
            "Analyze the metrics provided in the incident alert:\n"
            "- PostgreSQL connections at 450/500 (near exhaustion)\n"
            "- Redis hit rate dropped from 95% to 62%\n"
            "- Memory usage elevated from 55% to 78%\n\n"
            "Determine: Is this a connection pool leak? A cache failure? "
            "A resource exhaustion issue? Provide specific findings with "
            "data points and your confidence level for each finding."
        ),
        expected_output=(
            "Infrastructure investigation report with:\n"
            "1. Database connection analysis (is the pool leaking?)\n"
            "2. Redis cache degradation analysis\n"
            "3. Memory pressure analysis\n"
            "4. Correlation between these signals\n"
            "5. Confidence-rated hypotheses"
        ),
        agent=platform_engineer,
        context=[triage],
    )

    investigate_code = Task(
        description=(
            "Using the infrastructure findings, analyze the application "
            "behavior to identify the root cause. Consider:\n"
            "- Why would DB connections spike without a deployment?\n"
            "- What could cause Redis hit rate to drop?\n"
            "- How do these relate to the 503 errors?\n\n"
            "Think about: connection pool configuration, cache invalidation "
            "patterns, retry storms, and cascading failures. "
            "Propose a specific root cause and a fix."
        ),
        expected_output=(
            "Root cause analysis containing:\n"
            "1. The specific root cause with supporting evidence\n"
            "2. The failure cascade (what led to what)\n"
            "3. Proposed immediate fix (mitigation)\n"
            "4. Proposed permanent fix (prevention)\n"
            "5. Risk assessment for each fix"
        ),
        agent=backend_developer,
        context=[triage, investigate_infra],
    )

    write_postmortem = Task(
        description=(
            "Synthesize all findings into a post-incident review. "
            "Include the full timeline, root cause, impact assessment, "
            "resolution steps, and follow-up action items. "
            "Write for both technical and non-technical stakeholders."
        ),
        expected_output=(
            "Post-incident review document with:\n"
            "1. Executive summary (2-3 sentences)\n"
            "2. Timeline of events\n"
            "3. Root cause explanation\n"
            "4. Customer impact assessment\n"
            "5. Resolution steps taken\n"
            "6. Action items with owners and due dates\n"
            "7. Lessons learned"
        ),
        agent=comms_lead,
        context=[triage, investigate_infra, investigate_code],
    )

    # ── Build the crew ───────────────────────────────────────────────

    crew = Crew(
        agents=[incident_commander, platform_engineer, backend_developer, comms_lead],
        tasks=[triage, investigate_infra, investigate_code, write_postmortem],
        process=Process.sequential,
        memory=True,
        verbose=True,
    )

    # Wire up KD6 as the memory backend
    from crewai.memory import Memory

    crew._memory = Memory(
        storage=kd6_backend,
        llm=llm,
        embedder=get_crewai_embedder_config(),
        root_scope="/incident/shopstream",
    )

    return crew


def main() -> None:
    """Run the incident response crew."""
    print("=" * 70)
    print("  ShopStream Incident Response Simulation")
    print("  Powered by CrewAI + KD6 Memory")
    print("=" * 70)
    print()

    crew = build_crew()

    print("Starting incident response...\n")
    result = crew.kickoff()

    print("\n" + "=" * 70)
    print("  INCIDENT RESPONSE COMPLETE")
    print("=" * 70)
    print()
    print(result)
    print()
    print("Memories persisted in KD6. Run again to see cross-session recall.")


if __name__ == "__main__":
    main()
