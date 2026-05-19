"""
Bastion-Grond bridge: address risk endpoint.

Drop this into ``src/tools/`` and register the route in ``src/api/main.py``.

POST /api/v1/tools/address-risk
    Input:  { "target": "7xKXgR...", "chains": ["solana"],
              "analyst_id": "bastion-sidecar", "session_id": "..." }
    Output: { "risk_score": 85, "confidence": 0.72, "evidence": [...],
              "summary": "..." }

The endpoint orchestrates a multi-tool OSINT sweep against a blockchain
address: it searches for scam/rug/hack reports via Tavily web search,
optionally queries Twitter/X for community flags, and computes a
normalised 0-100 risk score.

Design:
  - Each search result becomes one Evidence item with Provenance.
  - Risk score is driven by keyword density (scam, rug, hack, phishing,
    drained, exploit) weighted by source confidence.
  - Failures degrade gracefully: partial results are still returned.
"""
from __future__ import annotations

import asyncio
import re
from datetime import UTC, datetime
from typing import Any

import structlog
from pydantic import BaseModel, Field

from src.core.audit import AuditLogger
from src.core.config import get_settings
from src.core.exceptions import ToolExecutionError
from src.models.evidence import ClaimType, Evidence, Provenance, SourceTool
from src.tools.tavily_tool import TavilyAdapter, TavilyInput

log = structlog.get_logger("grond.tools.address_risk")

# ═══════════════════════════════════════════════════════════════════════════
# Query templates — tuned for blockchain address intelligence
# ═══════════════════════════════════════════════════════════════════════════

RISK_QUERIES: list[str] = [
    '"{target}" scam OR rug OR phishing OR hack',
    '"{target}" drained OR exploit OR stolen',
    '"{target}" wallet address suspicious',
    '"{target}" Solana scam report',
    '"{target}" blockchain investigation',
    '"{target}" (site:x.com OR site:twitter.com)',
]

RISK_KEYWORDS: dict[str, float] = {
    "scam": 0.8,
    "rug": 0.9,
    "hack": 0.7,
    "drained": 0.8,
    "exploit": 0.7,
    "phishing": 0.85,
    "stolen": 0.75,
    "suspicious": 0.5,
    "fraud": 0.7,
    "malicious": 0.8,
    "compromised": 0.6,
    "fake": 0.55,
    "warning": 0.4,
    "banned": 0.6,
    "blacklist": 0.7,
}


# ═══════════════════════════════════════════════════════════════════════════
# Models
# ═══════════════════════════════════════════════════════════════════════════


class AddressRiskInput(BaseModel):
    target: str = Field(..., description="Blockchain address to research")
    chains: list[str] = Field(default_factory=lambda: ["solana"])
    analyst_id: str = "bastion-sidecar"
    session_id: str = Field(default_factory=lambda: "risk-check")
    max_results_per_query: int = Field(default=5, ge=1, le=10)


class AddressRiskOutput(BaseModel):
    risk_score: int = Field(ge=0, le=100)
    confidence: float = Field(ge=0.0, le=1.0)
    evidence: list[Evidence] = Field(default_factory=list)
    summary: str = ""


# ═══════════════════════════════════════════════════════════════════════════
# Core logic
# ═══════════════════════════════════════════════════════════════════════════


async def _search_address(
    target: str,
    query: str,
    audit: AuditLogger,
    max_results: int,
) -> list[Evidence]:
    """Run a single Tavily search query for the target address."""
    settings = get_settings()
    inp = TavilyInput(
        target=target,
        query=query,
        claim_type=ClaimType.WEB_MENTION,
        analyst_id="bastion-sidecar",
        session_id="address-risk",
        search_depth="advanced",
        max_results=max_results,
    )
    adapter = TavilyAdapter(audit=audit, api_key=settings.tavily_api_key)
    try:
        return await adapter.run(inp)
    except ToolExecutionError:
        return []


def _score_text(text: str) -> tuple[float, list[str]]:
    """Return (keyword_score, matched_keywords) for a text snippet."""
    text_lower = text.lower()
    total = 0.0
    matched: list[str] = []
    for kw, weight in RISK_KEYWORDS.items():
        if kw in text_lower:
            total = max(total, weight)
            matched.append(kw)
    return total, matched


def _compute_risk(evidence_items: list[Evidence]) -> tuple[int, float, str]:
    """Compute risk score, confidence, and summary from evidence items."""
    if not evidence_items:
        return 0, 0.0, "No risk signals found for this address."

    scores: list[float] = []
    all_matched: set[str] = set()

    for item in evidence_items:
        snippet = str(item.value.get("snippet", "") or item.claim or "")
        kw_score, matched = _score_text(snippet)
        if kw_score > 0:
            weighted = kw_score * item.confidence
            scores.append(weighted)
            all_matched.update(matched)

    if not scores:
        return 0, max((e.confidence for e in evidence_items), default=0.0), (
            "Address searched — no risk keywords detected in results."
        )

    avg_score = sum(scores) / len(scores)
    risk_score = min(int(avg_score * 100), 100)

    kw_list = ", ".join(sorted(all_matched))
    confidence = min(len(scores) / max(len(evidence_items), 1), 1.0)

    if risk_score >= 70:
        summary = f"HIGH RISK — matched keywords: {kw_list}. {len(scores)} risk signals from {len(evidence_items)} sources."
    elif risk_score >= 35:
        summary = f"MEDIUM RISK — matched keywords: {kw_list}. {len(scores)} risk signals from {len(evidence_items)} sources."
    else:
        summary = f"LOW RISK — minor signals: {kw_list}. {len(evidence_items)} sources checked."

    return risk_score, confidence, summary


# ═══════════════════════════════════════════════════════════════════════════
# Endpoint
# ═══════════════════════════════════════════════════════════════════════════


async def address_risk_endpoint(
    inp: AddressRiskInput,
    *,
    audit: AuditLogger,
) -> AddressRiskOutput:
    """
    Research a blockchain address for scam/rug/hack/exploit associations.

    Called by Bastion's GrondOracle via ``POST /api/v1/tools/address-risk``.
    """
    target = inp.target.strip()
    if not target:
        return AddressRiskOutput(risk_score=0, confidence=0.0, summary="Empty target.")

    audit.record(
        "address_risk_start",
        tool="address_risk",
        target=target,
        chains=inp.chains,
    )

    queries = [q.format(target=target) for q in RISK_QUERIES]

    tasks = [
        _search_address(target, q, audit, inp.max_results_per_query)
        for q in queries
    ]
    results = await asyncio.gather(*tasks)

    all_evidence: list[Evidence] = []
    for batch in results:
        all_evidence.extend(batch)

    risk_score, confidence, summary = _compute_risk(all_evidence)

    audit.record(
        "address_risk_complete",
        tool="address_risk",
        target=target,
        risk_score=risk_score,
        confidence=confidence,
        evidence_count=len(all_evidence),
    )

    log.info(
        "address_risk_done",
        target=target,
        risk_score=risk_score,
        evidence_count=len(all_evidence),
    )

    return AddressRiskOutput(
        risk_score=risk_score,
        confidence=confidence,
        evidence=all_evidence,
        summary=summary,
    )
