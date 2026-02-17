# termchat

A terminal-based chat client powered by [OpenRouter](https://openrouter.ai). Stream responses with live markdown rendering, search the web mid-conversation, and switch between models on the fly.

> **Work in progress** — features and APIs may change. Contributions and feedback welcome.

![Python 3.10+](https://img.shields.io/badge/python-3.10%2B-blue)

## Features

- **Streaming markdown** — responses render live with syntax highlighting
- **Model switching** — swap models mid-conversation with `/model`
- **Web search** — built-in Tavily search, available as a slash command or automatic tool call
- **Conversation history** — save and load conversations
- **Token & cost tracking** — see prompt/completion tokens and cost after each response

## Setup

```bash
# Clone the repo
git clone https://github.com/HarshitSandhu/termchat.git
cd termchat

# Create a virtual environment
python -m venv .venv
source .venv/bin/activate

# Install
pip install -e .

# Configure API keys
cp .env.example .env
# Edit .env and add your keys:
#   OPENROUTER_API_KEY=your-key-here
#   TAVILY_API_KEY=your-key-here  (optional, for web search)
```

## Usage

```bash
termchat
```

### Commands

| Command | Description |
|---------|-------------|
| `/model <name>` | Switch to a different model |
| `/models` | List popular OpenRouter models |
| `/search <query>` | Manual web search via Tavily |
| `/save` | Save current conversation |
| `/load` | Load a previous conversation |
| `/history` | List saved conversations |
| `/clear` | Clear current conversation |
| `/help` | Show help |
| `/quit` | Exit |

## Requirements

- Python 3.10+
- [OpenRouter API key](https://openrouter.ai/keys)
- [Tavily API key](https://tavily.com) (optional, for web search)

## License

MIT
