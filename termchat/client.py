import json
from typing import Generator

import httpx

from termchat.config import CEREBRAS_API_KEY, MAX_TOKENS
from termchat.search import SEARCH_TOOL_SCHEMA, tavily_search

API_URL = "https://api.cerebras.ai/v1/chat/completions"


class ChatClient:
    def __init__(self):
        self.http = httpx.Client(timeout=60)
        self.headers = {
            "Authorization": f"Bearer {CEREBRAS_API_KEY}",
            "Content-Type": "application/json",
        }
        self.last_usage: dict | None = None

    def stream_chat(
        self, messages: list[dict], model: str
    ) -> Generator[str, None, str | None]:
        """Stream chat completions. Yields tokens. Returns full assistant message content.
        Handles tool calls internally: executes the tool, feeds results back, and continues streaming."""
        tools = [SEARCH_TOOL_SCHEMA]
        full_response = yield from self._stream_request(messages, model, tools)
        return full_response

    def complete_chat(self, messages: list[dict], model: str) -> str | None:
        payload = {
            "model": model,
            "messages": messages,
            "max_tokens": MAX_TOKENS,
            "stream": False,
        }
        try:
            resp = self.http.post(API_URL, json=payload, headers=self.headers, timeout=120)
        except httpx.HTTPError:
            return None
        if resp.status_code != 200:
            return None

        data = resp.json()
        usage = data.get("usage") or {}
        self.last_usage = {
            "prompt_tokens": usage.get("prompt_tokens", 0),
            "completion_tokens": usage.get("completion_tokens", 0),
            "total_tokens": usage.get("total_tokens", 0),
        }

        choices = data.get("choices") or []
        if not choices:
            return None
        message = choices[0].get("message") or {}
        content = message.get("content")
        if isinstance(content, str):
            return content
        return None

    def _stream_request(
        self, messages: list[dict], model: str, tools: list[dict]
    ) -> Generator[str, None, str | None]:
        payload = {
            "model": model,
            "messages": messages,
            "max_tokens": MAX_TOKENS,
            "stream": True,
            "stream_options": {"include_usage": True},
            "tools": tools,
        }

        collected_content = ""
        tool_calls_accum: dict[int, dict] = {}

        with self.http.stream(
            "POST", API_URL, json=payload, headers=self.headers, timeout=120
        ) as resp:
            if resp.status_code != 200:
                error_body = resp.read().decode()
                yield f"\n[API Error {resp.status_code}]: {error_body}"
                return None

            for line in resp.iter_lines():
                if not line.startswith("data: "):
                    continue
                data_str = line[6:]
                if data_str.strip() == "[DONE]":
                    break
                try:
                    chunk = json.loads(data_str)
                except json.JSONDecodeError:
                    continue

                # Capture usage from any chunk that includes it
                if usage := chunk.get("usage"):
                    self.last_usage = {
                        "prompt_tokens": usage.get("prompt_tokens", 0),
                        "completion_tokens": usage.get("completion_tokens", 0),
                        "total_tokens": usage.get("total_tokens", 0),
                    }

                delta = (
                    chunk.get("choices", [{}])[0].get("delta", {})
                    if chunk.get("choices")
                    else {}
                )

                # Accumulate text content
                if text := delta.get("content"):
                    collected_content += text
                    yield text

                # Accumulate tool calls
                if tc_list := delta.get("tool_calls"):
                    for tc in tc_list:
                        idx = tc.get("index", 0)
                        if idx not in tool_calls_accum:
                            tool_calls_accum[idx] = {
                                "id": tc.get("id", ""),
                                "type": "function",
                                "function": {"name": "", "arguments": ""},
                            }
                        if tc.get("id"):
                            tool_calls_accum[idx]["id"] = tc["id"]
                        fn = tc.get("function", {})
                        if fn.get("name"):
                            tool_calls_accum[idx]["function"]["name"] = fn["name"]
                        if fn.get("arguments"):
                            tool_calls_accum[idx]["function"]["arguments"] += fn[
                                "arguments"
                            ]

        # If we got tool calls, execute them and re-stream
        if tool_calls_accum:
            tool_calls_list = [tool_calls_accum[i] for i in sorted(tool_calls_accum)]

            assistant_msg = {"role": "assistant", "content": collected_content or None, "tool_calls": tool_calls_list}
            messages = [*messages, assistant_msg]

            for tc in tool_calls_list:
                fn_name = tc["function"]["name"]
                try:
                    fn_args = json.loads(tc["function"]["arguments"])
                except json.JSONDecodeError:
                    fn_args = {}

                if fn_name == "web_search":
                    query = fn_args.get("query", "")
                    yield f"\n🔍 Searching: {query}\n"
                    result = tavily_search(query)
                else:
                    result = f"Unknown tool: {fn_name}"

                messages.append(
                    {
                        "role": "tool",
                        "tool_call_id": tc["id"],
                        "content": result,
                    }
                )

            # Continue streaming with tool results
            full_response = yield from self._stream_request(messages, model, tools)
            return full_response

        return collected_content or None

    def get_generation_stats(self) -> dict | None:
        """Return token usage captured from the last streaming response."""
        return self.last_usage

    def close(self):
        self.http.close()
