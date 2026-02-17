import httpx
from termchat.config import TAVILY_API_KEY

SEARCH_TOOL_SCHEMA = {
    "type": "function",
    "function": {
        "name": "web_search",
        "description": "Search the web for current information. Use this when the user asks about recent events, news, or anything that requires up-to-date information.",
        "parameters": {
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query",
                }
            },
            "required": ["query"],
        },
    },
}


def tavily_search(query: str) -> str:
    if not TAVILY_API_KEY:
        return "Error: TAVILY_API_KEY not set in .env file."

    try:
        resp = httpx.post(
            "https://api.tavily.com/search",
            json={
                "api_key": TAVILY_API_KEY,
                "query": query,
                "max_results": 5,
            },
            timeout=15,
        )
        resp.raise_for_status()
        data = resp.json()
    except httpx.HTTPError as e:
        return f"Search error: {e}"

    results = data.get("results", [])
    if not results:
        return "No search results found."

    lines = []
    for r in results:
        title = r.get("title", "")
        url = r.get("url", "")
        snippet = r.get("content", "")
        lines.append(f"**{title}**\n{snippet}\n{url}")
    return "\n\n---\n\n".join(lines)
