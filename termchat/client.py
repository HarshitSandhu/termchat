import json
import time
from typing import Generator

import httpx

from termchat.config import OPENROUTER_API_KEY, MAX_TOKENS
from termchat.search import SEARCH_TOOL_SCHEMA, tavily_search

API_URL = "https://openrouter.ai/api/v1/chat/completions"


class ChatClient:
    def __init__(self):
        self.http = httpx.Client(timeout=60)
        self.headers = {
            "Authorization": f"Bearer {OPENROUTER_API_KEY}",
            "Content-Type": "application/json",
        }
        self.last_generation_id: str | None = None

    def stream_chat(
        self, messages: list[dict], model: str
    ) -> Generator[str, None, str | None]:
        """Stream chat completions. Yields tokens. Returns full assistant message content.
        Handles tool calls internally: executes the tool, feeds results back, and continues streaming."""
        tools = [SEARCH_TOOL_SCHEMA]
        full_response = yield from self._stream_request(messages, model, tools)
        return full_response

    def _stream_request(
        self, messages: list[dict], model: str, tools: list[dict]
    ) -> Generator[str, None, str | None]:
        payload = {
            "model": model,
            "messages": messages,
            "max_tokens": MAX_TOKENS,
            "stream": True,
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

                # Capture generation ID from the first chunk
                if gen_id := chunk.get("id"):
                    self.last_generation_id = gen_id

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

            # Add assistant message with tool calls
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
        """Fetch token usage and cost for the last generation from OpenRouter."""
        if not self.last_generation_id:
            return None
        # OpenRouter needs a moment to finalize generation stats
        time.sleep(1)
        try:
            resp = self.http.get(
                f"https://openrouter.ai/api/v1/generation?id={self.last_generation_id}",
                headers=self.headers,
            )
            if resp.status_code != 200:
                return None
            data = resp.json().get("data", {})
            return {
                "prompt_tokens": data.get("tokens_prompt", 0),
                "completion_tokens": data.get("tokens_completion", 0),
                "total_tokens": (data.get("tokens_prompt", 0) + data.get("tokens_completion", 0)),
                "cost": data.get("total_cost", 0),
            }
        except Exception:
            return None

    def close(self):
        self.http.close()
