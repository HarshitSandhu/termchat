# termchat

A terminal-based chat client for OpenAI-compatible LLM providers — [Cerebras](https://cerebras.ai), [OpenRouter](https://openrouter.ai), and [Fireworks AI](https://fireworks.ai). Stream responses with markdown rendering, search the web mid-conversation, and switch between providers and models on the fly. Written in Rust.

> **Work in progress** — features and APIs may change. Contributions and feedback welcome.

![Rust](https://img.shields.io/badge/rust-1.74%2B-orange)

## Features

- **Streaming responses** — tokens stream live, then the message is re-rendered as formatted markdown
- **Multiple providers** — switch between Cerebras, OpenRouter, and Fireworks AI with `/provider`; paste each provider's API key once and it's remembered
- **Model switching** — swap models mid-conversation with `/model` (arrow-key picker)
- **Web search** — built-in Tavily search, available as a slash command or automatic tool call
- **Deep search** — multi-step current-events research with `/deepsearch`
- **Conversation history** — save and load conversations
- **Token tracking** — see prompt/completion tokens after each response

## Setup

```bash
# Clone the repo
git clone https://github.com/HarshitSandhu/termchat.git
cd termchat

# Build
cargo build --release

# Configure API keys
cp .env.example .env
# Edit .env and add a key for your provider(s):
#   CEREBRAS_API_KEY / OPENROUTER_API_KEY / FIREWORKS_API_KEY
#   TAVILY_API_KEY   (optional, for web search)
```

You can also set provider keys at runtime with `/provider`; they're saved to
`.termchat.json` (gitignored) and take precedence over the `.env` values.

## Usage

```bash
cargo run --release
# or, after `cargo install --path .`
termchat
```

The `.env`, `.last_model`, and `history/` files are resolved relative to the
current working directory, so run termchat from the project root.

### Commands

| Command | Description |
|---------|-------------|
| `/provider [name]` | Switch provider (`cerebras`/`openrouter`/`fireworks`) and paste its API key; omit name for an arrow-key picker |
| `/model <name>` | Switch to a different model (omit name for an arrow-key picker) |
| `/models` | List popular models for the current provider |
| `/search <query>` | Manual web search via Tavily |
| `/deepsearch <topic>` | Run multi-step current-events research |
| `/save` | Save current conversation |
| `/load` | Load a previous conversation |
| `/history` | List saved conversations |
| `/clear` | Clear current conversation |
| `/help` | Show help |
| `/quit`, `/exit` | Exit |

## Requirements

- Rust 1.74+ (2021 edition)
- [Cerebras API key](https://cloud.cerebras.ai)
- [Tavily API key](https://tavily.com) (optional, for web search)

## License

MIT
