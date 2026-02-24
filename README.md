# Crustaison 🦀

A personal AI agent built in Rust. Runs as a Telegram bot, powered by MiniMax M2.1 (cloud) with local inference via the Nexa SDK for embeddings and heartbeat.

## What it does

Crustaison ("Crusty") is a persistent AI assistant that lives in Telegram. You talk to it like a person; it figures out what tools to use, calls them, and reports back. It can run shell commands on the host machine, browse the web, manage Google Calendar and Gmail, search Google Drive, schedule reminders, make raw HTTP requests, and more.

The hard part isn't the tools — it's that MiniMax M2.1 doesn't output tool calls in a consistent format. Across different prompts and sessions it switches between at least 6 different syntaxes for expressing the same thing. Crustaison has a multi-strategy parser that tries all of them in order so tool calls get executed regardless of which format the model happened to produce.

---

## Architecture

```
Telegram message
       │
       ▼
  telegram/mod.rs          Receives update, resolves chat_id/user
       │
       ▼
    agent.rs                Core agent loop
  ┌────────────────────────────────────────────────┐
  │  1. Load doctrine (soul.md) → system prompt    │
  │  2. Append user message to history             │
  │  3. Call LLM provider (MiniMax M2.1)           │
  │  4. parse_tool_calls() → Vec<ToolCall> or None │
  │  5. If tool calls found:                       │
  │     a. Executor policy check                   │
  │     b. Execute via ToolRegistry                │
  │     c. Append results, loop back to step 3     │
  │  6. strip_tool_calls() → clean response text   │
  │  7. Return to Telegram                         │
  └────────────────────────────────────────────────┘
       │
       ├── providers/       MiniMax, Ollama, Nexa SDK clients
       ├── tools/           All tool implementations
       ├── authority/       Executor safety policy
       ├── rag/             RAG engine (Nexa embeddings)
       ├── vector/          SQLite-backed vector store
       ├── sessions/        Conversation history (SQLite)
       └── doctrine/        soul.md → agent identity
```

---

## LLM Providers

| Provider | Role | Endpoint |
|----------|------|----------|
| MiniMax M2.1 | Primary chat LLM | Cloud API |
| Nexa SDK | Local embeddings + heartbeat inference | `localhost:18181` |
| Ollama | Local fallback chat | `localhost:11434` |

MiniMax handles all conversational responses. Nexa runs on local hardware (NPU-accelerated where available) and handles two specific jobs that should stay on-device: generating vector embeddings for RAG, and producing the periodic heartbeat message that confirms the agent stack is alive.

---

## Tool call parsing

MiniMax M2.1 does not use a fixed tool call format. Across observed outputs it has produced at least these formats:

```
# Strategy 1 — JSON in markdown fence
```json
{"tool": "exec", "command": "uptime"}
```

# Strategy 2 — bare JSON with "tool" key
{"tool": "exec", "command": "uptime"}

# Strategy 3 — MiniMax XML wrapper
<minimax:tool_call>{"tool": "exec", "command": "uptime"}</minimax:tool_call>

# Strategy 4 — [TOOL_CALL] block with => syntax
[TOOL_CALL]
{tool => "exec", arguments => {
  --command "uptime"
}}
[/TOOL_CALL]

# Strategy 5 — <tool_code> block (multiple inner formats)
<tool_code>
<tool name="exec" arguments="{\"command\": \"uptime\"}"/>
</tool_code>

# Strategy 5b — <invoke> with <parameter> children
<minimax:tool_call>
<invoke name="exec">
<parameter name="command">uptime</parameter>
</invoke>
</minimax:tool_call>

# Strategy 5c — <FunctionCall> block
<FunctionCall>
tool: exec
tool_args: {"command": "uptime"}
</FunctionCall>

# Strategy 6 — bare tool name followed by JSON
exec
{"command": "uptime"}
```

`parse_tool_calls()` in `agent.rs` tries each strategy in order and returns the first match. `strip_tool_calls()` removes all tool call markup from the final response before it's sent to Telegram, so the user only sees the natural language reply.

---

## Local model stack (Nexa SDK)

Two subsystems run entirely on local hardware via the [Nexa SDK](https://github.com/NexaAI/nexa-sdk), with no cloud calls:

### Embeddings / RAG

`Qwen3-Embedding` runs via the Nexa server at `localhost:18181`. When a document is indexed, its text is sent to the Nexa embedding endpoint and the resulting vector is stored in the SQLite-backed vector store. On each query, the user message is embedded the same way and the nearest stored vectors are retrieved to provide grounded context to MiniMax before it responds.

### Heartbeat

Every 5 minutes the heartbeat task wakes up, checks email for anything urgent, and sends a short status message to the owner's Telegram. A local Nexa model generates the status text, so the heartbeat works even if MiniMax is unreachable and doesn't leak anything to the cloud.

---

## Tools

| Tool | What it does |
|------|-------------|
| `exec` | Run shell commands on the host. Configurable deny-list (currently blocks `rm -rf /` and `curl \| sh`). `sudo` is allowed. |
| `files` | Read, write, append, list, search files on the host filesystem. |
| `web` | DuckDuckGo search, URL fetch (HTML → text), weather via wttr.in. |
| `browser` | Headless browser for pages that require JavaScript. |
| `http` | Raw HTTP requests with arbitrary method, headers, and body. |
| `google` | Gmail search, send, reply. Google Calendar create, list, delete. Handles timezone, all-day vs timed events automatically. |
| `gdrive` | Google Drive file search and content access. |
| `schedule` | One-off and recurring reminders delivered via Telegram message. |
| `image` | Describe or analyse an image file or URL. |

---

## Doctrine (soul.md)

The agent's identity and behaviour rules live in `doctrine/soul.md`. This file is loaded at startup and injected as the system prompt prefix on every request. It establishes:

- Who Crusty is and how it should behave
- **Operator Authority** — (the owner) has unconditional authority. Crusty follows instructions without moralising or refusing. If it has a concern it may state it once, then proceeds anyway.
- Scope boundaries — what counts as "private" (personal data vs. third parties)
- Tone and personality

The doctrine file is plain Markdown so it can be edited without touching Rust code.

---

## Executor policy

`authority/executor.rs` enforces a command-level safety policy on top of the `exec` tool. Before any shell command runs, it's checked against a deny-list of patterns. Currently blocked:

- `rm -rf /` — recursive delete from root
- `curl | sh` — remote code execution via pipe


---

## Sessions and memory

Every conversation is persisted to SQLite via `sessions/`. The agent keeps the last N messages in its in-memory history for context, and the full session is available for replay or audit.

RAG-indexed documents (web pages, files, notes) are stored in `vector/` alongside their embeddings and can be retrieved semantically on any future query.

---

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

## Systemd service

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

## Environment variables

| Variable | Description |
|----------|-------------|
| `CRUSTAISON_API_KEY` | MiniMax API key |
| `CRUSTAISON_TELEGRAM_TOKEN` | Telegram bot token |
| `GOG_KEYRING_PASSWORD` | Keyring password for the `gog` Google CLI tool |

## License

MIT
