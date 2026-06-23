use chrono::{DateTime, Local};
use serde_json::{json, Value};

use crate::client::ChatClient;
use crate::search::tavily_search_results;

#[derive(Clone)]
pub struct DeepSearchResult {
    pub query: String,
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub score: f64,
    pub published_date: String,
}

fn query_plan(topic: &str, now: &DateTime<Local>) -> Vec<String> {
    let date_label = now.format("%B %d, %Y").to_string();
    let month_label = now.format("%B %Y").to_string();
    vec![
        format!("{topic} latest developments {date_label}"),
        format!("{topic} breaking news {date_label}"),
        format!("{topic} what changed this week {month_label}"),
        format!("{topic} timeline latest updates"),
    ]
}

fn score_result(result: &Value, query_index: usize) -> f64 {
    let mut score = result.get("score").and_then(Value::as_f64).unwrap_or(0.0);
    let published = result
        .get("published_date")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if !published.is_empty() {
        score += 1.0;
    }
    score += f64::max(0.0, 0.25 - (query_index as f64 * 0.05));
    score
}

async fn collect_results(queries: &[String]) -> (Vec<DeepSearchResult>, Vec<String>) {
    let mut collected: Vec<DeepSearchResult> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    let mut seen_urls: std::collections::HashSet<String> = std::collections::HashSet::new();

    for (idx, query) in queries.iter().enumerate() {
        match tavily_search_results(query, 5).await {
            Err(msg) => errors.push(format!("{query}: {msg}")),
            Ok(results) => {
                for item in &results {
                    let url = item.get("url").and_then(Value::as_str).unwrap_or("").trim();
                    if url.is_empty() || seen_urls.contains(url) {
                        continue;
                    }
                    seen_urls.insert(url.to_string());
                    collected.push(DeepSearchResult {
                        query: query.clone(),
                        title: item
                            .get("title")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .trim()
                            .to_string(),
                        url: url.to_string(),
                        snippet: item
                            .get("content")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .trim()
                            .to_string(),
                        score: score_result(item, idx),
                        published_date: item
                            .get("published_date")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .trim()
                            .to_string(),
                    });
                }
            }
        }
    }

    collected.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    collected.truncate(10);
    (collected, errors)
}

fn build_synthesis_prompt(
    topic: &str,
    findings: &[DeepSearchResult],
    now: &DateTime<Local>,
) -> String {
    let mut evidence_lines = Vec::new();
    for (idx, item) in findings.iter().enumerate() {
        let published = if item.published_date.is_empty() {
            "unknown publish date"
        } else {
            &item.published_date
        };
        let snippet = if item.snippet.is_empty() {
            "No snippet available."
        } else {
            &item.snippet
        };
        evidence_lines.push(format!(
            "[{}] {}\nquery: {}\npublished: {}\nurl: {}\nsnippet: {}",
            idx + 1,
            item.title,
            item.query,
            published,
            item.url,
            snippet,
        ));
    }
    let evidence_block = evidence_lines.join("\n\n");
    let today = now.format("%B %d, %Y").to_string();

    [
        format!("You are preparing a deep-search brief for current events as of {today}."),
        format!("Topic: {topic}"),
        "Use only the evidence provided below.".to_string(),
        "If evidence is thin or conflicting, say so explicitly.".to_string(),
        "Write concise markdown with these sections:".to_string(),
        "## Summary".to_string(),
        "## What Changed Recently".to_string(),
        "## Timeline".to_string(),
        "## Sources".to_string(),
        "## Open Questions".to_string(),
        "Requirements:".to_string(),
        "- Mention exact dates when available.".to_string(),
        "- Keep the Summary to 4 bullet points max.".to_string(),
        "- In Sources, cite each source as a markdown bullet with title and URL.".to_string(),
        "- Do not invent facts beyond the supplied evidence.".to_string(),
        String::new(),
        "Evidence:".to_string(),
        evidence_block,
    ]
    .join("\n")
}

fn fallback_report(findings: &[DeepSearchResult], now: &DateTime<Local>) -> String {
    let today = now.format("%B %d, %Y").to_string();
    let mut lines = vec!["## Summary".to_string()];

    for item in findings.iter().take(4) {
        let snippet = if item.snippet.is_empty() {
            "No snippet available."
        } else {
            &item.snippet
        };
        lines.push(format!("- **{}**: {}", item.title, snippet));
    }

    lines.push(String::new());
    lines.push("## What Changed Recently".to_string());
    lines.push(
        "Evidence was collected, but model synthesis failed, so this is a source-driven fallback brief."
            .to_string(),
    );
    lines.push(String::new());
    lines.push("## Timeline".to_string());
    for item in findings.iter().take(6) {
        let published = if item.published_date.is_empty() {
            today.as_str()
        } else {
            &item.published_date
        };
        lines.push(format!("- **{}**: {}", published, item.title));
    }

    lines.push(String::new());
    lines.push("## Sources".to_string());
    for item in findings.iter().take(8) {
        lines.push(format!("- [{}]({})", item.title, item.url));
    }

    lines.push(String::new());
    lines.push("## Open Questions".to_string());
    lines.push("- Which of these reports are primary reporting versus rewrites?".to_string());
    lines.push(
        "- Are there official statements that confirm the latest developments?".to_string(),
    );

    lines.join("\n")
}

/// Run the multi-step current-events research pipeline. Returns the markdown
/// report and a metadata object (queries, sources, errors).
pub async fn deep_search_current_events(
    topic: &str,
    client: &mut ChatClient,
    model: &str,
    progress: &dyn Fn(&str),
) -> (String, Value) {
    let now = Local::now();
    let queries = query_plan(topic, &now);

    progress("Planning queries");
    progress("Searching recent coverage");
    let (findings, errors) = collect_results(&queries).await;

    if findings.is_empty() {
        let error_text = if errors.is_empty() {
            "No search results found.".to_string()
        } else {
            errors.join("\n")
        };
        let report = format!("Deep search could not find usable results.\n\n{error_text}");
        let metadata = json!({
            "queries": queries,
            "sources": [],
            "errors": errors,
        });
        return (report, metadata);
    }

    progress("Ranking and grouping reports");
    let prompt = build_synthesis_prompt(topic, &findings, &now);

    progress("Writing brief");
    let report = match client
        .complete_chat(&[json!({"role": "user", "content": prompt})], model)
        .await
    {
        Some(text) if !text.trim().is_empty() => text,
        _ => fallback_report(&findings, &now),
    };

    let sources: Vec<Value> = findings
        .iter()
        .map(|item| {
            json!({
                "title": item.title,
                "url": item.url,
                "query": item.query,
                "published_date": item.published_date,
            })
        })
        .collect();

    let metadata = json!({
        "queries": queries,
        "sources": sources,
        "errors": errors,
    });

    (report, metadata)
}
