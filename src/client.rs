use std::collections::BTreeMap;

use anyhow::Result;
use futures_util::StreamExt;
use serde_json::{json, Value};

use crate::config::MAX_TOKENS;
use crate::search::{search_tool_schema, tavily_search};

#[derive(Clone, Copy, Default)]
pub struct Usage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    #[allow(dead_code)]
    pub total_tokens: u64,
}

#[derive(Default)]
struct ToolAccum {
    id: String,
    name: String,
    arguments: String,
}

pub struct ChatClient {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
    pub last_usage: Option<Usage>,
}

impl ChatClient {
    pub fn new(base_url: String, api_key: String) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()?;
        Ok(Self {
            http,
            base_url,
            api_key,
            last_usage: None,
        })
    }

    /// Point the client at a different provider endpoint and key.
    pub fn set_provider(&mut self, base_url: String, api_key: String) {
        self.base_url = base_url;
        self.api_key = api_key;
    }

    /// Stream a chat completion, invoking `on_token` for each text token.
    ///
    /// Tool calls are handled internally: the requested tool is executed, its
    /// result is fed back into the conversation, and streaming continues until
    /// the model returns a plain assistant message. Returns the final content.
    pub async fn stream_chat<F>(
        &mut self,
        messages: &[Value],
        model: &str,
        mut on_token: F,
    ) -> Result<Option<String>>
    where
        F: FnMut(&str),
    {
        let tools = vec![search_tool_schema()];
        let mut convo: Vec<Value> = messages.to_vec();

        loop {
            let (content, tool_calls) = self
                .stream_request(&convo, model, &tools, &mut on_token)
                .await?;

            if tool_calls.is_empty() {
                return Ok(content);
            }

            // Record the assistant turn that requested the tool calls.
            convo.push(json!({
                "role": "assistant",
                "content": content,
                "tool_calls": tool_calls,
            }));

            for tc in &tool_calls {
                let fn_name = tc["function"]["name"].as_str().unwrap_or("");
                let args_str = tc["function"]["arguments"].as_str().unwrap_or("{}");
                let args: Value = serde_json::from_str(args_str).unwrap_or_else(|_| json!({}));

                let result = if fn_name == "web_search" {
                    let query = args.get("query").and_then(Value::as_str).unwrap_or("");
                    on_token(&format!("\n🔍 Searching: {query}\n"));
                    tavily_search(query).await
                } else {
                    format!("Unknown tool: {fn_name}")
                };

                convo.push(json!({
                    "role": "tool",
                    "tool_call_id": tc["id"],
                    "content": result,
                }));
            }
        }
    }

    /// Issue one streaming request and return `(content, tool_calls)`.
    async fn stream_request<F>(
        &mut self,
        messages: &[Value],
        model: &str,
        tools: &[Value],
        on_token: &mut F,
    ) -> Result<(Option<String>, Vec<Value>)>
    where
        F: FnMut(&str),
    {
        let payload = json!({
            "model": model,
            "messages": messages,
            "max_tokens": MAX_TOKENS,
            "stream": true,
            "stream_options": {"include_usage": true},
            "tools": tools,
        });

        let resp = self
            .http
            .post(&self.base_url)
            .bearer_auth(&self.api_key)
            .json(&payload)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            on_token(&format!("\n[API Error {status}]: {body}"));
            return Ok((None, vec![]));
        }

        let mut collected = String::new();
        let mut tools_accum: BTreeMap<i64, ToolAccum> = BTreeMap::new();
        let mut buf = String::new();
        let mut stream = resp.bytes_stream();

        'outer: while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            buf.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(pos) = buf.find('\n') {
                let line: String = buf.drain(..=pos).collect();
                let line = line.trim_end_matches(['\r', '\n']);

                let Some(data) = line.strip_prefix("data: ") else {
                    continue;
                };
                if data.trim() == "[DONE]" {
                    break 'outer;
                }

                let Ok(parsed) = serde_json::from_str::<Value>(data) else {
                    continue;
                };

                if let Some(usage) = parsed.get("usage").filter(|u| !u.is_null()) {
                    self.last_usage = Some(Usage {
                        prompt_tokens: usage["prompt_tokens"].as_u64().unwrap_or(0),
                        completion_tokens: usage["completion_tokens"].as_u64().unwrap_or(0),
                        total_tokens: usage["total_tokens"].as_u64().unwrap_or(0),
                    });
                }

                let delta = &parsed["choices"][0]["delta"];

                if let Some(text) = delta.get("content").and_then(Value::as_str) {
                    collected.push_str(text);
                    on_token(text);
                }

                if let Some(tc_list) = delta.get("tool_calls").and_then(Value::as_array) {
                    for tc in tc_list {
                        let idx = tc.get("index").and_then(Value::as_i64).unwrap_or(0);
                        let entry = tools_accum.entry(idx).or_default();
                        if let Some(id) = tc.get("id").and_then(Value::as_str) {
                            if !id.is_empty() {
                                entry.id = id.to_string();
                            }
                        }
                        if let Some(name) = tc["function"]["name"].as_str() {
                            if !name.is_empty() {
                                entry.name = name.to_string();
                            }
                        }
                        if let Some(args) = tc["function"]["arguments"].as_str() {
                            entry.arguments.push_str(args);
                        }
                    }
                }
            }
        }

        let tool_calls: Vec<Value> = tools_accum
            .into_values()
            .map(|t| {
                json!({
                    "id": t.id,
                    "type": "function",
                    "function": {"name": t.name, "arguments": t.arguments},
                })
            })
            .collect();

        let content = if collected.is_empty() {
            None
        } else {
            Some(collected)
        };

        Ok((content, tool_calls))
    }

    /// Non-streaming completion, used by deep search synthesis.
    pub async fn complete_chat(&mut self, messages: &[Value], model: &str) -> Option<String> {
        let payload = json!({
            "model": model,
            "messages": messages,
            "max_tokens": MAX_TOKENS,
            "stream": false,
        });

        let resp = self
            .http
            .post(&self.base_url)
            .bearer_auth(&self.api_key)
            .json(&payload)
            .send()
            .await
            .ok()?;

        if !resp.status().is_success() {
            return None;
        }

        let data: Value = resp.json().await.ok()?;

        if let Some(usage) = data.get("usage").filter(|u| !u.is_null()) {
            self.last_usage = Some(Usage {
                prompt_tokens: usage["prompt_tokens"].as_u64().unwrap_or(0),
                completion_tokens: usage["completion_tokens"].as_u64().unwrap_or(0),
                total_tokens: usage["total_tokens"].as_u64().unwrap_or(0),
            });
        }

        data["choices"][0]["message"]["content"]
            .as_str()
            .map(str::to_string)
    }
}
