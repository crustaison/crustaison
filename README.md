# Crustaison 🦀

A personal AI agent built in Rust. Runs as a Telegram bot, powered by MiniMax M2.1 (cloud) with a local model stack via the Nexa SDK for embeddings, routing, fast reasoning, and vision.

## What it does

Crustaison ("Crusty") is a persistent AI assistant that lives in Telegram. You talk to it like a person; it figures out what tools to use, calls them, and reports back. It can run shell commands on the host machine, browse the web, manage Google Calendar and Gmail, search Google Drive, schedule reminders, make raw HTTP requests, and more.

The hard part isn't the tools — it's that MiniMax M2.1 doesn't output tool calls in a consistent format. Across different prompts and sessions it switches between at least 6 different syntaxes for expressing the same thing. Crustaison has a multi-strategy parser that tries all of them in order so tool calls get executed regardless of which format the model happened to produce.

Beyond the conversation loop, Crusty runs a **Ralph-loop** — a reflex layer (antennae), reusable task recipes (molts), and named failure recovery (regrowth) — that lets it self-monitor, apply consensus safety checks to destructive operations, and rate its own output for offline improvement.

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
  │  1. Load doctrine (soul.md + MOLTLOG.md)       │
  │  2. Append user message to history             │
  │  3. antennae fire: PreRequest                  │
  │  4. Call LLM provider (MiniMax M2.1)           │
  │  5. parse_tool_calls() → Vec<ToolCall>         │
  │  6. If tool calls found:                       │
  │     a. antennae fire: PreToolUse               │
  │        → destructive_guard may Block           │
  │     b. Execute via ToolRegistry                │
  │     c. antennae fire: PostToolUse              │
  │        → telemetry logs the call               │
  │     d. On failure → regrowth recipes retry     │
  │     e. Append results, loop back to step 4     │
  │  7. strip_tool_calls() → clean response text   │
  │  8. antennae fire: ResponseComplete            │
  │     → judge scores the reply async             │
  │  9. Return to Telegram                         │
  └────────────────────────────────────────────────┘
       │
       ├── providers/       MiniMax, Ollama, Nexa SDK clients
       ├── tools/           All tool implementations
       ├── authority/       Executor safety policy
       ├── antennae.rs      Lifecycle event bus (hooks)
       ├── molts.rs         Named reusable task recipes
       ├── regrowth.rs      Named failure-recovery recipes
       ├── judge.rs         Async response-quality scorer
       ├── destructive_guard.rs  Dual-LLM consensus on risky ops
       ├── telemetry.rs     Per-tool-call JSONL logger
       ├── compact.rs       On-demand doctrine compaction
       ├── rag/             RAG engine (Nexa embeddings)
       ├── vector/          SQLite-backed vector store
       ├── sessions/        Conversation history (SQLite)
       └── doctrine/        soul.md + MOLTLOG.md → identity
```

---

## LLM Providers

| Role | Model | Endpoint | Purpose |
|------|-------|----------|---------|
| Primary chat | MiniMax M2.1 | Cloud API | All conversational responses + consensus on destructive ops |
| Router | Qwen3-1.7B (`/no_think`) | Nexa `localhost:18181` | Fast classification (~272ms), judge scoring |
| Reasoning | Qwen3.5-35B-A3B (Q4_K_M) | `nexa infer` subprocess | Heavy local reasoning when MiniMax is unreachable or not wanted |
| Vision | Qwen3-VL-2B-Instruct | Nexa `localhost:18181` | Image description / receipt parsing |
| Embedding | Qwen3-Embedding-0.6B-GGUF:F16 | Nexa `localhost:18181` | 1024-dim vectors for RAG |
| Fallback chat | Ollama models | `localhost:11434` | Available but unused in normal flow |

The 35B is invoked as a `nexa infer` subprocess (fresh process per call) rather than over the serve HTTP API — `nexa serve` proved unstable on 35B and corrupted context for other co-resident models. `nexa serve` handles the small-model concurrent traffic (router + embedding + vision) fine.

---

## Ralph-loop (antennae / molts / regrowth)

Crusty is structured around three crustacean-themed primitives that wrap conventional agent concepts. Event names and frontmatter fields are kept byte-compatible with the ClawdCode spec so skills and hooks written against that interface drop in unchanged.

### Antennae — lifecycle hooks

`antennae.rs` is an event bus. Registered listeners receive `AntennaSignal`s at known points (`PreToolUse`, `PostToolUse`, `PostToolUseFailure`, `ResponseComplete`, session start/end, etc.) and return an `AntennaOutcome` that can block, warn, or pass-through.

Listeners currently wired in:
- **destructive_guard** — on `PreToolUse`, if the call matches a destructive pattern (`rm -rf`, force-push, schema drops, service kills), asks the router + MiniMax concurrently. Only a concurring YES lets it through; any NO or timeout blocks. ~2s latency on ~5% of commands.
- **telemetry** — on `PostToolUse` / `PostToolUseFailure`, appends JSONL to `~/.local/share/crustaison/tool_calls.jsonl`. Reveals hot tools, failing tools, and dead tools for removal.
- **judge** — on `ResponseComplete`, spawns an async task that asks the router (1.7B `/no_think`, ~300ms) to rate helpfulness and factuality 1-5. Scores land in `~/.local/share/crustaison/judge_scores.jsonl` for offline GEPA prompt evolution. Never blocks the user reply.

### Molts — named reusable task recipes

A molt is a directory under `~/.config/crustaison/molts/<name>/` containing a `MOLT.md` with YAML frontmatter + a markdown body. Only the frontmatter is loaded and shown to the LLM at startup (progressive disclosure); the body is read from disk when `recall_molt` is explicitly invoked.

### Regrowth — named failure recovery

When a tool call fails, the regrowth module classifies the failure into a `LimbLoss` variant (transient network, rate-limit, auth-expired, etc.) and applies a small set of recipes (~3 cover most observed transients) to retry before escalating to the user.

### Compact

`compact_doctrine` is an explicit tool (not a cron) that reads an append-only doctrine file (`MOLTLOG.md` or `memory.md`), asks MiniMax to fold duplicates and preserve attribution, and writes back atomically with a timestamped `.bak`.

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

## RAG / Heartbeat (Nexa)

### Embeddings / RAG

`Qwen3-Embedding-0.6B-GGUF:F16` runs via the Nexa server at `localhost:18181` and returns 1024-dim vectors. When a document is indexed, its text is sent to the Nexa embedding endpoint and the resulting vector is stored in the SQLite-backed vector store (`~/.local/share/crustaison/coordinator_store/embeddings.json`). On each query, the user message is embedded the same way and the nearest stored vectors are retrieved to provide grounded context to MiniMax before it responds.

### Heartbeat

Every 5 minutes the heartbeat task wakes up, checks email for anything urgent, and sends a short status message to the owner's Telegram. A local Nexa model generates the status text, so the heartbeat works even if MiniMax is unreachable and doesn't leak anything to the cloud.

---

## Tools

| Tool | What it does |
|------|-------------|
| `exec` | Run shell commands on the host. Deny-list blocks `rm -rf /` and `curl \| sh`. `sudo` is allowed. |
| `files` | Read, write, append, list, search files on the host filesystem. |
| `web` | DuckDuckGo search, URL fetch (HTML → text), weather via wttr.in. |
| `browser` | Headless browser for pages that require JavaScript. |
| `http` | Raw HTTP requests with arbitrary method, headers, and body. |
| `google` | Gmail search/send/reply. Google Calendar create/list/delete (handles timezone + all-day vs timed). |
| `gdrive` | Google Drive file search and content access. |
| `schedule` | One-off and recurring reminders delivered via Telegram message. |
| `image` | Describe or analyse an image file or URL (Qwen3-VL-2B). |
| `lake` | Lake of the Ozarks lookups (marina-adjacent data). |
| `roster` | Staff / sheriff / marina roster queries backed by scraper scripts. |
| `compact_doctrine` | Compact an append-only doctrine file via MiniMax. |
| `recall_molt` | Load the body of a named molt (skill) when it's needed. |

---

## Doctrine (soul.md + MOLTLOG.md)

The agent's identity and behaviour rules live in `~/.config/crustaison/doctrine/soul.md`. A second file, `MOLTLOG.md`, is an append-only log of Ralph-loop learnings ("what I learned this cycle") loaded as `Doctrine.moltlog` and injected into every system prompt. Both are plain Markdown so they can be edited without touching Rust code.

- **soul.md** — who Crusty is, operator authority, scope boundaries, tone.
- **MOLTLOG.md** — rolling ledger of lessons; Crusty appends to it via the files tool when it notices something worth remembering.
- **memory.md**, **agents.md**, **principles.md** — additional slices loaded by `DoctrineLoader`.

---

## Executor policy

`authority/executor.rs` enforces a command-level safety policy on top of the `exec` tool. Before any shell command runs, it's checked against a deny-list of patterns. Currently blocked:

- `rm -rf /` — recursive delete from root
- `curl | sh` — remote code execution via pipe

The **destructive_guard** antenna listener sits on top of this and adds a semantic dual-LLM check on destructive *intent*, catching commands the static deny-list would miss.

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

# Offline eval harness — replay captured conversations (evals/*.jsonl)
cargo run --release --bin eval

# GEPA prompt-evolution loop — uses judge scores to evolve prompts
cargo run --release --bin gepa
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
