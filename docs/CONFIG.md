# Example Configurations

## Basic Configuration

`~/.config/crustaison/config.toml`:
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

## Development Configuration

`~/.config/crustaison/config.toml`:
```toml
[cognition]
model = "ollama"
# Local Ollama for development
model_name = "llama2"

[gateway]
port = 8080
rate_limit_requests = 1000  # Relaxed for dev
rate_limit_window_seconds = 60

[security]
allow_destructive = true  # Enabled for development
require_confirmation = []
max_file_size_mb = 100
```

## Production Configuration

`~/.config/crustaison/config.toml`:
```toml
[cognition]
model = "minimax"
max_tokens = 8192
temperature = 0.5  # More deterministic

[gateway]
port = 8080
rate_limit_requests = 50   # Stricter limits
rate_limit_window_seconds = 60

[telegram]
bot_token = "YOUR_BOT_TOKEN"
allowed_users = [7766171845, 123456789]

[security]
allow_destructive = false  # Always false in prod
require_confirmation = ["write", "exec", "rm", "delete"]
max_file_size_mb = 5

[plugins]
enabled = false  # Disable plugins in prod
```

## Security Policy

`~/.local/share/crustaison/security_policy.json`:
```json
{
  "version": "1.0",
  "allow_destructive": false,
  "allowed_tools": ["read", "list", "search", "web_search"],
  "blocked_commands": ["rm", "del", "format", "mkfs"],
  "max_file_size_bytes": 10485760,
  "require_confirmation": ["write", "exec", "append"],
  "sandbox_paths": ["/home/user/projects"],
  "allowed_domains": ["github.com", "docs.rs"]
}
```

## Webhook Configuration

`~/.local/share/crustaison/webhooks.json`:
```json
[
  {
    "name": "slack-alerts",
    "url": "https://hooks.slack.com/services/xxx/yyy/zzz",
    "events": ["tool.executed", "error"],
    "headers": {
      "Content-Type": "application/json"
    },
    "timeout_seconds": 30
  }
]
```

## Plugin Manifest Example

`~/.local/share/crustaison/plugins/my-plugin/plugin.json`:
```json
{
  "name": "my-plugin",
  "version": "0.1.0",
  "description": "A custom plugin for Crustaison",
  "author": "developer@example.com",
  "main": "plugin.so",
  "permissions": ["memory.read", "memory.write"]
}
```

## Environment Variables

```bash
# API Keys
export CRUSTAISON_API_KEY="sk-minimax-xxx"

# Telegram
export CRUSTAISON_TELEGRAM_TOKEN="8302813309:AAGIxxx"

# Editor
export EDITOR="nvim"

# Data Directory
export CRUSTAISON_DATA_DIR="/data/crustaison"
```
