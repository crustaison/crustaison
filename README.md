# Crustaison 🦀

A personal AI agent built in Rust. Runs as a Telegram bot, powered by MiniMax M2.1, with a comprehensive tool system for real-world task execution.

## What it does

Crustaison ("Crusty") is a persistent AI assistant that lives in Telegram. It can run shell commands, browse the web, manage Google Calendar and Gmail, search Google Drive, schedule reminders, fetch URLs, and more — all through natural language.

It handles whatever format the underlying LLM decides to output tool calls in, with a multi-strategy parser that covers 6+ formats MiniMax has been observed using.

## Features

- **Telegram Bot** — primary interface, single-user (owner-authorized only)
- **MiniMax M2.1** — primary LLM with fallback to Ollama/Nexa local models
- **Multi-strategy tool call parsing** — handles XML, JSON, `[TOOL_CALL]`, `<invoke>`, `<FunctionCall>`, and bare identifier formats
- **RAG** — Retrieval-Augmented Generation via Nexa Qwen3-Embedding vector store
- **Heartbeat** — periodic email monitoring and Nexa watchdog
- **Operator authority doctrine** — soul.md defines identity and operator trust hierarchy

## Tools

| Tool | Description |
|------|-------------|
| `exec` | Run shell commands (with configurable deny-list) |
| `files` | Read, write, list, search files |
| `web` | Web search (DuckDuckGo), fetch URLs, weather |
| `browser` | Headless browser for JS-heavy pages |
| `http` | Raw HTTP requests with custom headers |
| `google` | Gmail search/send, Google Calendar create/list/delete |
| `gdrive` | Google Drive search and file access |
| `schedule` | Set one-off and recurring reminders via Telegram |
| `image` | Image description/analysis |

## Architecture

```
crustaison/
├── src/
│   ├── agent.rs           # Core agent loop, multi-strategy tool call parser
│   ├── authority/         # Executor policy (denied commands, safety checks)
│   ├── cognition/         # Planning and reflection
│   ├── doctrine/          # Identity rules (soul.md)
│   ├── providers/         # LLM backends: MiniMax, Ollama, Nexa
│   ├── rag/               # Retrieval-Augmented Generation
│   ├── runtime/           # Heartbeat, checks
│   ├── sessions/          # SQLite session storage
│   ├── telegram/          # Telegram bot handler
│   ├── tools/             # All tool implementations
│   └── vector/            # Vector store for embeddings
├── doctrine/
│   └── soul.md            # Agent identity and operator authority rules
└── Cargo.toml
```

## Running

```bash
# Build
cargo build --release

# Telegram bot (primary mode)
./target/release/crustaison telegram

# TUI (local terminal)
./target/release/crustaison tui

# REST API daemon
./target/release/crustaison daemon
```

## Configuration

`~/.config/crustaison/config.toml`:

```toml
[cognition]
model = "minimax"
max_tokens = 8192
temperature = 0.7

[telegram]
bot_token = ""
allowed_users = []

[security]
allow_destructive = false
```

## Environment Variables

| Variable | Description |
|----------|-------------|
| `CRUSTAISON_API_KEY` | MiniMax API key |
| `CRUSTAISON_TELEGRAM_TOKEN` | Telegram bot token |
| `GOG_KEYRING_PASSWORD` | Password for gog Google CLI keyring |

## Systemd Service

```ini
[Unit]
Description=Crustaison Telegram AI Agent
After=network.target

[Service]
Type=simple
User=sean
WorkingDirectory=/home/sean/crustaison
ExecStart=/home/sean/crustaison/target/release/crustaison telegram
Restart=always
RestartSec=10
Environment=RUST_LOG=info

[Install]
WantedBy=multi-user.target
```

## License

MIT
