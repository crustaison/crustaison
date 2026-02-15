# Crustaison 🦀

A self-improving AI agent built in Rust with safety-first architecture.

## Features

- **Multi-Provider LLM Support**: MiniMax (cloud), Ollama (local), Nexa (local)
- **Tool System**: Files, exec, web search with safety checks
- **Memory Management**: Sessions (SQLite), Journal, Contexts, Vector Store
- **RAG**: Retrieval-Augmented Generation for grounded responses
- **Plugins**: Dynamic plugin system with metadata
- **Webhooks**: Outbound webhooks for event notifications
- **CLI & TUI**: Terminal interface and REST API
- **Telegram Bot**: @ryzozarkbot integration

## Quick Start

```bash
# Build
cargo build --release

# Run in TUI mode
./target/release/crustaison tui

# Run as daemon (REST API on port 8080)
./target/release/crustaison daemon

# Run Telegram bot
./target/release/crustaison telegram

# Chat via API
curl -X POST http://localhost:8080/chat \
  -H "Content-Type: application/json" \
  -d '{"message": "Hello, Crustaison!"}'
```

## Architecture

```
Crustaison/
├── authority/      # Immutable safety layer (auth, policy, execution)
├── cognition/      # Self-improving layer (planning, memory, reflection)
├── doctrine/       # Identity rules (soul.md, agents.md)
├── runtime/        # Working state (memory.json, heartbeat, logs)
├── ledger/        # Immutable audit trail (git-backed)
├── providers/      # LLM backends (MiniMax, Ollama, Nexa)
├── tools/         # exec, files, web
├── sessions/      # SQLite session storage
├── memory/        # Journal and contexts
├── vector/        # Vector store for RAG
├── rag/           # Retrieval-Augmented Generation
├── plugins/       # Dynamic plugin system
├── webhooks/      # HTTP callbacks
└── tui/           # Terminal UI
```

## Configuration

Create `~/.config/crustaison/config.toml`:

```toml
[cognition]
model = "minimax"
max_tokens = 8192
temperature = 0.7

[gateway]
port = 8080
rate_limit_requests = 100
rate_limit_window_seconds = 60

[telegram]
bot_token = ""
allowed_users = []

[security]
allow_destructive = false
require_confirmation = ["write", "exec"]
max_file_size_mb = 10
```

## CLI Commands

```bash
# Core
crustaison tui              # Terminal UI mode
crustaison daemon [port]    # REST API server
crustaison telegram         # Telegram bot

# Memory
crustaison memory journal "note"   # Write to journal
crustaison memory today            # Read today's journal
crustaison memory save <name> <content>  # Save context
crustaison memory load <name>       # Load context

# Sessions
crustaison sessions list            # List sessions
crustaison sessions show <id>       # Show session details
crustaison sessions delete <id>      # Delete session

# RAG
crustaison rag index <source> <content>  # Index document
crustaison rag search <query>           # Semantic search
crustaison rag stats                      # Show stats

# Webhooks
crustaison webhook list                    # List webhooks
crustaison webhook add <name> <url> <events...>  # Add webhook
crustaison webhook test <url>             # Test webhook

# Plugins
crustaison plugins list                   # List plugins
crustaison plugins install <name>         # Install plugin
crustaison plugins enable <name>          # Enable plugin
```

## REST API

### Chat
```bash
POST /chat
{
  "message": "Your question here",
  "source": "api"
}
```

### Execute Command
```bash
POST /execute
{
  "command": "read",
  "parameters": {"path": "/some/file.txt"}
}
```

### Memory
```bash
POST /memory/store
{
  "key": "user_pref",
  "value": {"theme": "dark"},
  "record_type": "preference"
}

POST /memory/recall
{
  "key": "user_pref"
}
```

## Environment Variables

| Variable | Description |
|----------|-------------|
| `CRUSTAISON_API_KEY` | MiniMax API key |
| `CRUSTAISON_TELEGRAM_TOKEN` | Telegram bot token |
| `EDITOR` | Preferred editor for `config edit` |

## Safety

- **Allow Destructive**: Controlled by `security.allow_destructive`
- **Tool Whitelist**: Only registered tools can be called
- **Confirmation Required**: Destructive ops need confirmation
- **Audit Trail**: All actions logged to ledger

## Extending

### Add a Tool

```rust
use crate::tools::Tool;

pub struct MyTool;

#[async_trait::async_trait]
impl Tool for MyTool {
    fn name(&self) -> &str {
        "my_tool"
    }
    
    fn description(&self) -> &str {
        "Does something useful"
    }
    
    async fn call(&self, args: &serde_json::Value) -> Result<String, String> {
        // Implementation
        Ok("result".to_string())
    }
}
```

### Add a Provider

Implement the `Provider` trait from `providers::provider`:

```rust
#[async_trait::async_trait]
impl Provider for MyProvider {
    async fn chat(&self, messages: &[ChatMessage]) -> Result<String, ProviderError> {
        // Call your LLM
    }
}
```

## License

MIT
