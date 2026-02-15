# Crustaison Completion Guide: Match & Exceed Ozark v5

**For**: Clyde (via OpenClaw on ryz.local)
**Project**: `/home/sean/crustaison/`
**Reference**: `/home/sean/ozark-v5/`
**Date**: February 14, 2026

---

## Current State Analysis

### Crustaison: 1,651 lines across 20 files
### Ozark v5: 8,479 lines across 51 files

Crustaison has **better architecture** but Ozark v5 has **more functionality**. The goal is to bring Crustaison up to feature parity while keeping its superior safety-first design.

---

## Side-by-Side Comparison

| Feature | Ozark v5 | Crustaison | Gap |
|---------|----------|------------|-----|
| **LLM Providers** | 5 (MiniMax, Nexa, Ollama, OpenAI, Anthropic) | 1 (MiniMax) | Need Ollama + Nexa at minimum |
| **Tool System** | 5 tools (exec, files, web, print, moltbook) | 0 (explicitly disabled) | Critical gap |
| **Tool Call Parsing** | Multi-strategy (native + 3 text parsers) | Strips tool calls as hallucinations | Critical gap |
| **Agent Loop** | Full loop: LLM → parse tools → execute → feed back → respond | Single-shot chat only | Critical gap |
| **TUI** | ratatui (animated, input history, cancel, slash commands) | Prints config then exits | Significant gap |
| **Telegram Bot** | Advanced (per-user state, /model switch, /think toggle) | Basic (auth, /clear, /help) | Moderate gap |
| **Streaming** | chat_streaming via provider | Non-streaming only | Moderate gap |
| **Memory** | 3-layer (journal + context + vector) | Working memory JSON + SQLite engine | Moderate gap |
| **Sessions** | SQLite with CRUD, export, search | None | Moderate gap |
| **CLI Subcommands** | 18 (setup, config, doctor, models, memory, git, scheduler, printer, edit, etc.) | 5 (tui, daemon, telegram, check, version) | Significant gap |
| **API Server** | axum (REST + WebSocket) | warp (REST only) | Minor gap |
| **Git Integration** | status, diff, log, branch, stage, commit | None (only ledger uses git) | Nice-to-have |
| **Scheduler** | Cron-like with JSON persistence | Heartbeat config (not running) | Nice-to-have |
| **Security Audit** | Config analysis with severity levels | Policy engine (allow/deny) | Crustaison is better |
| **Gateway** | None | Auth, rate limiting, normalization | Crustaison advantage |
| **Executor** | None | Policy-enforced execution | Crustaison advantage |
| **Doctrine** | soul.md + tools.md + heartbeat.md | soul.md + agents.md + principles.md | Crustaison advantage |
| **Planner** | None | Plan generation (stub) | Crustaison advantage |
| **Reflection** | None | Self-assessment (stub) | Crustaison advantage |
| **Audit Trail** | None | Git-backed immutable ledger | Crustaison advantage |
| **Run Logs** | None | Daily JSONL execution logs | Crustaison advantage |

---

## What to Build (Priority Order)

### PHASE 1: Tool System (CRITICAL - Do This First)
**This is the #1 gap. Without tools, Crusty is just a chatbot.**

### PHASE 2: Agent Loop with Tool Execution
**Connect tools to the LLM so it can actually do things.**

### PHASE 3: Additional Providers (Ollama + Nexa)
**Support local models for fast/free responses.**

### PHASE 4: TUI
**A real terminal interface.**

### PHASE 5: Enhanced Telegram
**Per-user state, model switching, streaming.**

### PHASE 6: Sessions & Memory Upgrade
**Persistent conversations and better memory.**

### PHASE 7: CLI Subcommands
**Config, models, memory, edit, etc.**

---

## PHASE 1: Tool System

Create `src/tools/` directory. Port from Ozark v5 but route through Crustaison's executor/policy layer.

### 1a. Tool Trait — `src/tools/tool.rs`

```rust
//! Tool trait and types

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::any::Any;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub tool: String,
    pub args: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> serde_json::Value;
    async fn call(&self, args: serde_json::Value) -> Result<ToolResult>;
    fn as_any(&self) -> &dyn Any;
}
```

### 1b. Tool Registry — `src/tools/registry.rs`

```rust
//! Tool registry

use std::collections::HashMap;
use std::sync::Arc;
use anyhow::Result;
use super::tool::{Tool, ToolResult};

pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self { tools: HashMap::new() }
    }

    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    pub fn get(&self, name: &str) -> Option<&Arc<dyn Tool>> {
        self.tools.get(name)
    }

    pub fn list(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }

    pub async fn call(&self, name: &str, args: serde_json::Value) -> Result<ToolResult> {
        match self.tools.get(name) {
            Some(tool) => tool.call(args).await,
            None => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Tool '{}' not found", name)),
            }),
        }
    }

    /// Generate tool descriptions for the system prompt
    pub fn tool_descriptions(&self) -> String {
        let mut desc = String::from("## Available Tools\n\n");
        desc.push_str("To use a tool, respond with a JSON block like:\n");
        desc.push_str("```json\n{\"tool\": \"tool_name\", \"args\": {\"param\": \"value\"}}\n```\n\n");
        for tool in self.tools.values() {
            desc.push_str(&format!("### {}\n{}\n", tool.name(), tool.description()));
            desc.push_str(&format!("Parameters: {}\n\n", tool.parameters()));
        }
        desc
    }
}
```

### 1c. Exec Tool — `src/tools/exec.rs`

Port from Ozark v5's `src/tools/exec.rs` (204 lines). Key points:
- Shell command execution via `tokio::process::Command`
- **MUST route through Crustaison's executor/policy layer** for safety checks
- Dangerous pattern detection (rm -rf, sudo, curl|sh, fork bombs, etc.)
- Configurable timeout (default 30s, max 300s)
- Output truncation
- Working directory defaults to `/home/sean`

Reference: `/home/sean/ozark-v5/src/tools/exec.rs`

The exec tool should check the policy before running:
```rust
// Before executing, check policy
let executor = Executor::new();
let command = Command {
    name: "exec".to_string(),
    parameters: serde_json::json!({"command": cmd_str}),
    context: serde_json::json!({}),
};
let result = executor.execute(command).await?;
if !result.success {
    return Ok(ToolResult {
        success: false,
        output: String::new(),
        error: result.error,
    });
}
// Then actually run the shell command...
```

### 1d. Files Tool — `src/tools/files.rs`

Port from Ozark v5's `src/tools/files.rs` (331 lines). Operations:
- `read` — Read file contents (with size limit)
- `write` — Write file
- `list` — List directory (with recursion, capped at 100 entries)
- `exists` — Check if file exists
- `delete` — Delete file
- `mkdir` — Create directory
- `glob` — Pattern matching
- `stat` — File metadata
- `copy` — Copy file/directory
- `move` — Move/rename

Reference: `/home/sean/ozark-v5/src/tools/files.rs`

### 1e. Web Tool — `src/tools/web.rs`

Port from Ozark v5's `src/tools/web.rs` (356 lines). Operations:
- `weather` — Uses Open-Meteo API with hardcoded coordinates for Lake Ozark area cities
- `search` — DuckDuckGo HTML scraping
- `fetch` — HTTP client (GET/POST/PUT/DELETE with headers)
- HTML text extraction (strip scripts, styles, collapse whitespace)

Reference: `/home/sean/ozark-v5/src/tools/web.rs`

**IMPORTANT**: The weather tool has hardcoded coordinates for ~25 cities including Eldon, Osage Beach, Camdenton, Lake Ozark. Copy these coordinates exactly from Ozark v5.

### 1f. Module file — `src/tools/mod.rs`

```rust
pub mod tool;
pub mod registry;
pub mod exec;
pub mod files;
pub mod web;

pub use tool::{Tool, ToolCall, ToolResult};
pub use registry::ToolRegistry;
pub use exec::ExecTool;
pub use files::FilesTool;
pub use web::WebTool;
```

### 1g. Add `async-trait` to `Cargo.toml`

```toml
async-trait = "0.1"
```

This is needed for the `Tool` trait. Ozark v5 already uses it.

---

## PHASE 2: Agent Loop with Tool Execution

This is the critical wiring. The agent needs to:
1. Send messages to MiniMax with tool descriptions in the system prompt
2. Parse the LLM response for tool call JSON
3. Execute the tool calls
4. Feed results back to the LLM
5. Repeat until the LLM responds with plain text (no more tool calls)

### 2a. Update `src/agent.rs`

Replace the current simple `chat()` method with a full agent loop:

```rust
use crate::tools::{ToolRegistry, ToolCall, ToolResult};
use regex::Regex;

pub struct Agent {
    provider: MiniMaxProvider,
    doctrine_loader: DoctrineLoader,
    system_prompt: String,
    history: Vec<ChatMessage>,
    max_history: usize,
    tool_registry: Arc<ToolRegistry>,  // ADD THIS
}

impl Agent {
    pub async fn new(
        provider: MiniMaxProvider,
        doctrine_loader: DoctrineLoader,
        tool_registry: Arc<ToolRegistry>,  // ADD THIS
    ) -> Result<Self, anyhow::Error> {
        // ... existing doctrine loading ...

        // Append tool descriptions to system prompt
        system_prompt.push_str(&tool_registry.tool_descriptions());

        Ok(Self {
            provider,
            doctrine_loader,
            system_prompt,
            history: Vec::new(),
            max_history: 20,
            tool_registry,
        })
    }

    pub async fn chat(&mut self, user_message: &str) -> Result<String, anyhow::Error> {
        self.history.push(ChatMessage {
            role: "user".to_string(),
            content: user_message.to_string(),
        });

        if self.history.len() > self.max_history {
            let drain_count = self.history.len() - self.max_history;
            self.history.drain(0..drain_count);
        }

        // Agent loop: keep going until no more tool calls
        let max_iterations = 10;  // Safety limit
        let mut iteration = 0;

        loop {
            iteration += 1;
            if iteration > max_iterations {
                break;
            }

            let response = self.provider.chat(
                self.history.clone(),
                Some(self.system_prompt.clone()),
            ).await?;

            let content = response.content.clone();

            // Try to parse tool calls from the response
            let tool_calls = parse_tool_calls(&content);

            if tool_calls.is_empty() {
                // No tool calls — this is the final response
                let clean = strip_tool_calls(&content);
                self.history.push(ChatMessage {
                    role: "assistant".to_string(),
                    content: clean.clone(),
                });
                return Ok(clean);
            }

            // Execute tool calls and collect results
            self.history.push(ChatMessage {
                role: "assistant".to_string(),
                content: content.clone(),
            });

            let mut tool_results = Vec::new();
            for tc in &tool_calls {
                let result = self.tool_registry.call(&tc.tool, tc.args.clone()).await?;
                tool_results.push(format!(
                    "Tool '{}' result:\n{}{}",
                    tc.tool,
                    result.output,
                    result.error.map(|e| format!("\nError: {}", e)).unwrap_or_default()
                ));
            }

            // Feed results back as a user message (tool results)
            let results_msg = tool_results.join("\n\n");
            self.history.push(ChatMessage {
                role: "user".to_string(),
                content: format!("[Tool Results]\n{}", results_msg),
            });

            // Loop continues — LLM will see the tool results and respond
        }

        // Fallback if max iterations hit
        Ok("I've reached the maximum number of tool execution steps. Please try a simpler request.".to_string())
    }
}

/// Parse tool calls from LLM output text
/// Supports multiple formats:
/// 1. {"tool": "name", "args": {...}}
/// 2. ```json\n{"tool": "name", "args": {...}}\n```
/// 3. {"tool_calls": [{"tool": "name", "args": {...}}]}
fn parse_tool_calls(text: &str) -> Vec<ToolCall> {
    let mut calls = Vec::new();

    // Strategy 1: Look for JSON in markdown code fences
    let fence_re = Regex::new(r"```(?:json)?\s*\n?([\s\S]*?)\n?```").unwrap();
    for cap in fence_re.captures_iter(text) {
        if let Some(json_str) = cap.get(1) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(json_str.as_str()) {
                if let Some(tc) = try_parse_tool_call(&val) {
                    calls.push(tc);
                }
            }
        }
    }
    if !calls.is_empty() { return calls; }

    // Strategy 2: Look for bare JSON objects with "tool" key
    // Find JSON-like substrings
    let mut depth = 0i32;
    let mut start = None;
    for (i, ch) in text.char_indices() {
        match ch {
            '{' => {
                if depth == 0 { start = Some(i); }
                depth += 1;
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    if let Some(s) = start {
                        let json_str = &text[s..=i];
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(json_str) {
                            if let Some(tc) = try_parse_tool_call(&val) {
                                calls.push(tc);
                            }
                        }
                    }
                    start = None;
                }
            }
            _ => {}
        }
    }

    calls
}

/// Try to extract a ToolCall from a JSON value
fn try_parse_tool_call(val: &serde_json::Value) -> Option<ToolCall> {
    // Format: {"tool": "name", "args": {...}}
    if let Some(tool_name) = val.get("tool").and_then(|v| v.as_str()) {
        let args = val.get("args")
            .or_else(|| val.get("parameters"))
            .or_else(|| val.get("input"))
            .cloned()
            .unwrap_or(serde_json::json!({}));
        return Some(ToolCall {
            tool: tool_name.to_string(),
            args,
        });
    }

    // Format: {"tool_calls": [{...}]}
    if let Some(arr) = val.get("tool_calls").and_then(|v| v.as_array()) {
        for item in arr {
            if let Some(tc) = try_parse_tool_call(item) {
                return Some(tc);
            }
        }
    }

    None
}
```

### 2b. Add `regex` to `Cargo.toml`

```toml
regex = "1"
```

### 2c. Update `src/main.rs`

Wire up the tool registry when creating the agent:

```rust
use crate::tools::{ToolRegistry, ExecTool, FilesTool, WebTool};

// After loading config, before creating agent:
let mut tool_registry = ToolRegistry::new();
tool_registry.register(Arc::new(ExecTool::new()));
tool_registry.register(Arc::new(FilesTool::new()));
tool_registry.register(Arc::new(WebTool::new()));
let tool_registry = Arc::new(tool_registry);

// Update agent creation:
let agent = Arc::new(Mutex::new(
    Agent::new(provider, doctrine_loader.as_ref().clone(), tool_registry.clone()).await?
));
```

### 2d. Update system prompt

Remove the "you do NOT have access to tools" section. Replace with the tool descriptions generated by `tool_registry.tool_descriptions()`. The agent's `new()` method already appends these.

---

## PHASE 3: Additional Providers

### 3a. Ollama Provider — `src/providers/ollama.rs`

Port from `/home/sean/ozark-v5/src/providers/ollama.rs` (315 lines).
- Base URL: `http://localhost:11434`
- Endpoint: `POST /api/chat`
- Supports streaming via NDJSON
- Model listing via `GET /api/tags`

### 3b. Nexa Provider — `src/providers/nexa.rs`

Port from `/home/sean/ozark-v5/src/providers/nexa.rs` (270 lines).
- Base URL: `http://localhost:18181`
- OpenAI-compatible API (`/v1/chat/completions`)
- Model listing via `GET /v1/models`

### 3c. Provider Trait — `src/providers/provider.rs`

Create a common trait so the agent can swap providers:

```rust
use async_trait::async_trait;
use anyhow::Result;

#[async_trait]
pub trait LlmProvider: Send + Sync {
    fn name(&self) -> &str;
    async fn chat(
        &self,
        messages: Vec<ChatMessage>,
        system_prompt: Option<String>,
    ) -> Result<ChatResponse>;
    async fn list_models(&self) -> Result<Vec<String>>;
}
```

Make `MiniMaxProvider`, `OllamaProvider`, and `NexaProvider` all implement this trait. Update `Agent` to use `Arc<dyn LlmProvider>` instead of `MiniMaxProvider` directly.

### 3d. Update Config

```toml
[providers.minimax]
api_key = "sk-cp-..."
base_url = "https://api.minimax.io/v1"
model = "MiniMax-M2.1"

[providers.ollama]
base_url = "http://localhost:11434"
model = "qwen3-coder:30b"

[providers.nexa]
base_url = "http://localhost:18181"
model = "qwen2.5-3b"
```

### 3e. Provider Selection

Add `--provider` flag to CLI commands and `/model` command in Telegram. The config's `cognition.default_provider` determines which is used by default.

---

## PHASE 4: TUI

### 4a. Add ratatui dependencies to `Cargo.toml`

```toml
ratatui = "0.25"
crossterm = "0.27"
```

### 4b. Create `src/tui/mod.rs` and `src/tui/app.rs`

Port the structure from Ozark v5's `src/tui/app.rs` (791 lines). Key features to include:
- Split layout: chat area + input area
- Message display with role indicators (you> / crusty>)
- Input history (up/down arrows)
- Animated thinking indicator
- ESC to cancel processing
- Slash commands: /help, /clear, /quit, /tools, /model, /think
- Auto-scroll to bottom

Reference: `/home/sean/ozark-v5/src/tui/app.rs`

---

## PHASE 5: Enhanced Telegram

### 5a. Per-User State

Each Telegram user should get their own agent instance (or at minimum their own conversation history). Currently all users share one agent. Port the `UserState` pattern from Ozark v5's telegram.rs:

```rust
struct UserState {
    agent: Agent,
    model_name: String,
    show_thinking: bool,
}

// HashMap<ChatId, UserState>
```

### 5b. Commands

Add these commands (matching Ozark v5):
- `/model` — List available models with numbered selection
- `/model <name>` — Switch to a specific model
- `/think` — Toggle showing thinking/reasoning tags
- `/tools` — List available tools
- `/status` — Show current model, history length, uptime

### 5c. Thinking Tag Handling

Port the `strip_thinking()` function from Ozark v5:
```rust
fn strip_thinking(text: &str) -> String {
    // Remove <think>...</think> and <Think>...</Think> blocks
    // Remove special tokens like <|im_start|>, <|endoftext|>, etc.
    // Extract content after final<|message|> if present
}
```

---

## PHASE 6: Sessions & Memory Upgrade

### 6a. Session Manager — `src/sessions/mod.rs`

Port from Ozark v5's `src/sessions/mod.rs` (182 lines):
- SQLite tables: `sessions` and `session_messages`
- CRUD: create, list, get, get_messages, add_message, delete, clear, search
- Connect to the agent so chat history is persisted

### 6b. Memory Upgrade

Crustaison already has a `MemoryEngine` (SQLite) but needs:
- **Journal** — Port from Ozark v5's `src/memory/journal.rs` (daily markdown files)
- **Context Store** — Named markdown files for persistent context
- **Vector Store** — Already partially exists; add cosine similarity search

Connect the `MemoryManager` pattern from Ozark v5:
- `remember()` writes to both journal and vector store
- `recall()` searches vector store, falls back to keyword search
- `get_recent_context()` returns recent journal + active contexts

### 6c. Memory Tool — `src/tools/memory.rs`

Give the LLM a tool to remember and recall things:
```
Tool: memory
Operations:
  - remember: Store a fact/note with key
  - recall: Search memory by query
  - journal: Write to today's journal
  - context: Save/load named context files
```

---

## PHASE 7: CLI Subcommands

Add these subcommands to match Ozark v5's functionality:

```rust
enum Commands {
    // Existing
    Tui,
    Daemon { port: Option<u16> },
    Telegram,
    Check,
    Version,

    // New
    Setup,                    // Interactive first-run wizard
    Config { action: ... },   // get/set/edit/path
    Doctor,                   // System health check
    Models { action: ... },   // list/switch/pull
    Memory { action: ... },   // journal/remember/recall/contexts/reindex
    Sessions { action: ... }, // list/show/delete/clear/search/export
    Edit { file: String },    // Edit soul/agents/principles via $EDITOR
    Status,                   // Show current config, provider, uptime
    Security,                 // Run security audit
}
```

---

## Build & Test Workflow

For each phase:

```bash
cd /home/sean/crustaison

# 1. Make changes
# 2. Check compilation
cargo check

# 3. Build
cargo build --release

# 4. Test the specific feature
./target/release/crustaison check          # Config OK
./target/release/crustaison tui            # TUI works
./target/release/crustaison daemon &       # API works
curl -X POST http://localhost:18790/chat \
  -H "Content-Type: application/json" \
  -d '{"message":"what is the weather in eldon","source":"test"}'

# 5. Restart telegram if needed
pkill -f "crustaison telegram"
nohup ./target/release/crustaison telegram > /tmp/crustaison.log 2>&1 &
```

---

## Files to Create/Modify

### CREATE (New Files)
| File | Phase | Lines (est.) | Purpose |
|------|-------|------|---------|
| `src/tools/mod.rs` | 1 | 15 | Module exports |
| `src/tools/tool.rs` | 1 | 55 | Tool trait and types |
| `src/tools/registry.rs` | 1 | 60 | Tool registry |
| `src/tools/exec.rs` | 1 | 200 | Shell execution (port from ozark) |
| `src/tools/files.rs` | 1 | 330 | File operations (port from ozark) |
| `src/tools/web.rs` | 1 | 350 | Web/weather/search (port from ozark) |
| `src/tools/memory_tool.rs` | 6 | 100 | Memory tool for LLM |
| `src/providers/provider.rs` | 3 | 40 | Provider trait |
| `src/providers/ollama.rs` | 3 | 300 | Ollama provider (port from ozark) |
| `src/providers/nexa.rs` | 3 | 270 | Nexa provider (port from ozark) |
| `src/tui/mod.rs` | 4 | 10 | Module exports |
| `src/tui/app.rs` | 4 | 600 | TUI app (port from ozark) |
| `src/sessions/mod.rs` | 6 | 180 | Session persistence (port from ozark) |
| `src/memory/mod.rs` | 6 | 10 | Module exports |
| `src/memory/journal.rs` | 6 | 96 | Daily journal (port from ozark) |
| `src/memory/context.rs` | 6 | 73 | Named contexts (port from ozark) |
| `src/memory/manager.rs` | 6 | 89 | Memory orchestrator (port from ozark) |

### MODIFY (Existing Files)
| File | Phase | Changes |
|------|-------|---------|
| `src/agent.rs` | 2 | Add tool_registry, agent loop, tool call parsing |
| `src/main.rs` | 1-7 | Wire tools, providers, new subcommands |
| `src/lib.rs` | 1 | Add `pub mod tools;` |
| `src/telegram/mod.rs` | 5 | Per-user state, /model, /think, /tools commands |
| `src/providers/mod.rs` | 3 | Add ollama, nexa, provider trait |
| `src/providers/minimax.rs` | 3 | Implement LlmProvider trait |
| `src/config/config.rs` | 3 | Add provider configs section |
| `Cargo.toml` | 1-4 | Add async-trait, regex, ratatui, crossterm |
| `~/.config/crustaison/config.toml` | 3 | Add provider settings |

---

## What Crustaison Will Have That Ozark v5 Doesn't

After completion, Crustaison keeps its unique advantages:

1. **Immutable Authority Layer** — Gateway, executor, policy can't be bypassed by the agent
2. **Git-Backed Audit Ledger** — Every action committed, history can't be rewritten
3. **Doctrine System** — soul.md + agents.md + principles.md (richer than ozark's 2-file system)
4. **Planner** — Plan generation before execution (stub now, wire to LLM later)
5. **Reflection Engine** — Self-assessment after tasks (stub now, wire to LLM later)
6. **Policy-Enforced Execution** — Tools route through executor with allow/deny lists
7. **Rate Limiting** — Built into the gateway at the architecture level
8. **Run Logs** — Daily JSONL audit of all executions
9. **Working Memory** — Separate JSON-based cognitive state with goals and tasks

---

## Important Rules

1. **Work incrementally** — One phase at a time. Build, test, then move on.
2. **Don't modify authority layer** — `gateway.rs`, `executor.rs`, `policy.rs` are IMMUTABLE.
3. **Route tools through executor** — The exec tool MUST check policy before running commands.
4. **Port, don't reinvent** — Copy working code from Ozark v5 and adapt it. The implementations are tested.
5. **Test at each step** — `cargo check` after every file change. Don't accumulate errors.
6. **Keep the architecture** — Authority/cognition/runtime/ledger separation is the whole point.
7. **Report progress** — Send updates with [START], [PROGRESS], [DONE], [ERROR] prefixes.

---

## Estimated Effort

| Phase | New Lines | Ported From | Difficulty |
|-------|-----------|-------------|------------|
| 1. Tools | ~650 | ozark-v5/src/tools/ | Medium |
| 2. Agent Loop | ~150 | ozark-v5/src/agent/runtime.rs | Medium |
| 3. Providers | ~650 | ozark-v5/src/providers/ | Easy |
| 4. TUI | ~600 | ozark-v5/src/tui/app.rs | Medium |
| 5. Telegram | ~200 | ozark-v5/src/channels/telegram.rs | Easy |
| 6. Sessions+Memory | ~450 | ozark-v5/src/sessions/ + memory/ | Medium |
| 7. CLI | ~300 | ozark-v5/src/main.rs | Easy |
| **Total** | **~3,000** | | |

This will bring Crustaison from ~1,651 lines to ~4,651 lines — still leaner than Ozark v5's 8,479 but with MORE capability due to the safety architecture.

---

## Success Criteria

When all phases are complete:

1. `./crustaison tui` — Full TUI with tool execution, slash commands
2. `./crustaison telegram` — Bot responds intelligently, executes tools, switches models
3. `./crustaison daemon` — REST API with tool-augmented chat
4. Ask "what's the weather in Eldon" → Gets real weather via Open-Meteo
5. Ask "list files in /home/sean" → Returns directory listing
6. Ask "search the web for Rust async" → Returns DuckDuckGo results
7. Ask "remember that Sean likes fishing" → Stores in memory
8. Ask "what does Sean like?" → Recalls from memory
9. All tool executions go through the policy layer
10. All actions are logged to the git ledger
11. `./crustaison doctor` — Shows system health
12. `./crustaison security` — Shows security audit
