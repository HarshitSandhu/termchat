use serde_json::{json, Value};

use crate::config::tavily_api_key;

const TAVILY_URL: &str = "https://api.tavily.com/search";

/// Tool schema advertised to the model so it can request a web search.
pub fn search_tool_schema() -> Value {
    json!({
        "type": "function",
        "function": {
            "name": "web_search",
            "description": "Search the web for current information. Use this when the user asks about recent events, news, or anything that requires up-to-date information.",
            "parameters": {
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The search query"
                    }
                },
                "required": ["query"]
            }
        }
    })
}

/// Run a Tavily search and return the raw `results` array.
///
/// Mirrors the Python `tavily_search_results`: returns `Err(message)` for any
/// configuration or transport failure, and `Ok(vec![])` for an empty result set.
pub async fn tavily_search_results(query: &str, max_results: u32) -> Result<Vec<Value>, String> {
    let key = tavily_api_key();
    if key.is_empty() {
        return Err("Error: TAVILY_API_KEY not set in .env file.".to_string());
    }

    let client = reqwest::Client::new();
    let resp = client
        .post(TAVILY_URL)
        .json(&json!({
            "api_key": key,
            "query": query,
            "max_results": max_results,
        }))
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| format!("Search error: {e}"))?;

    if let Err(e) = resp.error_for_status_ref() {
        return Err(format!("Search error: {e}"));
    }

    let data: Value = resp
        .json()
        .await
        .map_err(|e| format!("Search error: {e}"))?;

    match data.get("results") {
        Some(Value::Array(items)) => Ok(items.clone()),
        _ => Ok(vec![]),
    }
}

/// Search and format the results as a markdown string for feeding back to the model.
pub async fn tavily_search(query: &str) -> String {
    match tavily_search_results(query, 5).await {
        Err(msg) => msg,
        Ok(results) if results.is_empty() => "No search results found.".to_string(),
        Ok(results) => {
            let mut blocks = Vec::new();
            for r in &results {
                let title = r.get("title").and_then(Value::as_str).unwrap_or("");
                let url = r.get("url").and_then(Value::as_str).unwrap_or("");
                let snippet = r.get("content").and_then(Value::as_str).unwrap_or("");
                blocks.push(format!("**{title}**\n{snippet}\n{url}"));
            }
            blocks.join("\n\n---\n\n")
        }
    }
}
