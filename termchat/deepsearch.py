from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime
from typing import Callable

from termchat.client import ChatClient
from termchat.search import tavily_search_results


@dataclass
class DeepSearchResult:
    query: str
    title: str
    url: str
    snippet: str
    score: float
    published_date: str


def _query_plan(topic: str, now: datetime) -> list[str]:
    date_label = now.strftime("%B %d, %Y")
    month_label = now.strftime("%B %Y")
    return [
        f"{topic} latest developments {date_label}",
        f"{topic} breaking news {date_label}",
        f"{topic} what changed this week {month_label}",
        f"{topic} timeline latest updates",
    ]


def _score_result(result: dict, query_index: int) -> float:
    score = float(result.get("score", 0.0))
    published_date = (result.get("published_date") or "").strip()
    if published_date:
        score += 1.0
    score += max(0, 0.25 - (query_index * 0.05))
    return score


def _collect_results(queries: list[str]) -> tuple[list[DeepSearchResult], list[str]]:
    collected: list[DeepSearchResult] = []
    errors: list[str] = []
    seen_urls: set[str] = set()

    for idx, query in enumerate(queries):
        results = tavily_search_results(query, max_results=5)
        if isinstance(results, str):
            errors.append(f"{query}: {results}")
            continue

        for item in results:
            url = item.get("url", "").strip()
            if not url or url in seen_urls:
                continue
            seen_urls.add(url)
            collected.append(
                DeepSearchResult(
                    query=query,
                    title=item.get("title", "").strip(),
                    url=url,
                    snippet=item.get("content", "").strip(),
                    score=_score_result(item, idx),
                    published_date=(item.get("published_date") or "").strip(),
                )
            )

    collected.sort(key=lambda item: item.score, reverse=True)
    return collected[:10], errors


def _build_synthesis_prompt(topic: str, findings: list[DeepSearchResult], now: datetime) -> str:
    evidence_lines = []
    for idx, item in enumerate(findings, start=1):
        published = item.published_date or "unknown publish date"
        evidence_lines.append(
            "\n".join(
                [
                    f"[{idx}] {item.title}",
                    f"query: {item.query}",
                    f"published: {published}",
                    f"url: {item.url}",
                    f"snippet: {item.snippet or 'No snippet available.'}",
                ]
            )
        )

    evidence_block = "\n\n".join(evidence_lines)
    today = now.strftime("%B %d, %Y")
    return "\n".join(
        [
            f"You are preparing a deep-search brief for current events as of {today}.",
            f"Topic: {topic}",
            "Use only the evidence provided below.",
            "If evidence is thin or conflicting, say so explicitly.",
            "Write concise markdown with these sections:",
            "## Summary",
            "## What Changed Recently",
            "## Timeline",
            "## Sources",
            "## Open Questions",
            "Requirements:",
            "- Mention exact dates when available.",
            "- Keep the Summary to 4 bullet points max.",
            "- In Sources, cite each source as a markdown bullet with title and URL.",
            "- Do not invent facts beyond the supplied evidence.",
            "",
            "Evidence:",
            evidence_block,
        ]
    )


def _fallback_report(topic: str, findings: list[DeepSearchResult], now: datetime) -> str:
    today = now.strftime("%B %d, %Y")
    summary_lines = []
    timeline_lines = []
    source_lines = []

    for item in findings[:4]:
        summary_lines.append(f"- **{item.title}**: {item.snippet or 'No snippet available.'}")

    for item in findings[:6]:
        published = item.published_date or today
        timeline_lines.append(f"- **{published}**: {item.title}")

    for item in findings[:8]:
        source_lines.append(f"- [{item.title}]({item.url})")

    return "\n".join(
        [
            "## Summary",
            *summary_lines,
            "",
            "## What Changed Recently",
            "Evidence was collected, but model synthesis failed, so this is a source-driven fallback brief.",
            "",
            "## Timeline",
            *timeline_lines,
            "",
            "## Sources",
            *source_lines,
            "",
            "## Open Questions",
            "- Which of these reports are primary reporting versus rewrites?",
            "- Are there official statements that confirm the latest developments?",
        ]
    )


def deep_search_current_events(
    topic: str,
    client: ChatClient,
    model: str,
    progress: Callable[[str], None] | None = None,
) -> tuple[str, dict]:
    now = datetime.now()
    queries = _query_plan(topic, now)

    if progress:
        progress("Planning queries")
    if progress:
        progress("Searching recent coverage")
    findings, errors = _collect_results(queries)

    if not findings:
        error_text = "\n".join(errors) if errors else "No search results found."
        return f"Deep search could not find usable results.\n\n{error_text}", {
            "queries": queries,
            "sources": [],
            "errors": errors,
        }

    if progress:
        progress("Ranking and grouping reports")
    prompt = _build_synthesis_prompt(topic, findings, now)

    if progress:
        progress("Writing brief")
    report = client.complete_chat(
        messages=[{"role": "user", "content": prompt}],
        model=model,
    )

    if not report:
        report = _fallback_report(topic, findings, now)

    metadata = {
        "queries": queries,
        "sources": [
            {
                "title": item.title,
                "url": item.url,
                "query": item.query,
                "published_date": item.published_date,
            }
            for item in findings
        ],
        "errors": errors,
    }
    return report, metadata
