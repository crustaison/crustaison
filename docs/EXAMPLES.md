# Usage Examples

## Basic Chat

```bash
# Using curl
curl -X POST http://localhost:8080/chat \
  -H "Content-Type: application/json" \
  -d '{"message": "What is Rust?"}'
```

Response:
```json
{
  "success": true,
  "data": {
    "response": "Rust is a systems programming language that focuses on safety and performance...",
    "source": "minimax"
  },
  "error": null
}
```

## Using Tools

```bash
# Read a file
curl -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{
    "command": "read",
    "parameters": {"path": "/etc/hostname"}
  }'
```

Response:
```json
{
  "success": true,
  "data": {
    "output": "my-server\n",
    "duration_ms": 2
  },
  "error": null
}
```

## Memory Operations

```bash
# Store a preference
curl -X POST http://localhost:8080/memory/store \
  -H "Content-Type: application/json" \
  -d '{
    "key": "favorite_language",
    "value": {"language": "Rust", "level": "expert"},
    "record_type": "preference"
  }'

# Recall it
curl -X POST http://localhost:8080/memory/recall \
  -H "Content-Type: application/json" \
  -d '{
    "key": "favorite_language"
  }'
```

## Session Management

```bash
# Create session (returns session_id)
curl -X POST http://localhost:8080/sessions \
  -H "Content-Type: application/json" \
  -d '{"name": "Project Discussion"}'

# Add message to session
curl -X POST http://localhost:8080/sessions/message \
  -H "Content-Type: application/json" \
  -d '{
    "session_id": "abc123",
    "role": "user",
    "content": "Tell me about vector databases"
  }'

# List sessions
curl http://localhost:8080/sessions
```

## RAG - Document Indexing

```bash
# Index a document
./crustaison rag index "docs/README.md" \
  "$(cat docs/README.md)"

# Search for relevant content
./crustaison rag search "How do I configure the agent?"

# Build context for a query
./crustaison rag context "What is the safety architecture?"
```

## Telegram Bot Commands

```
/help          - Show help
/clear         - Clear conversation history
/model         - Show current model
/model ollama  - Switch to Ollama
```

## Webhook Setup

```bash
# Add a webhook
./crustaison webhook add slack-notify \
  "https://hooks.slack.com/services/xxx" \
  "tool.executed"

# Test it
./crustaison webhook test "https://httpbin.org/post"

# List configured webhooks
./crustaison webhook list
```

## Plugin Management

```bash
# List plugins
./crustaison plugins list

# Install a plugin
./crustaison plugins install my-plugin

# Enable it
./crustaison plugins enable my-plugin

# Disable
./crustaison plugins disable my-plugin
```

## Planning

```bash
# Generate a plan
curl -X POST http://localhost:8080/plan \
  -H "Content-Type: application/json" \
  -d '{
    "goal": "Build a REST API in Rust"
  }'
```

Response:
```json
{
  "success": true,
  "data": {
    "id": "plan-xyz",
    "steps": [
      {"action": "search", "description": "Research Actix-web"},
      {"action": "write", "description": "Create main.rs with routes"},
      {"action": "exec", "description": "Test API endpoints"}
    ]
  },
  "error": null
}
```

## Integration Scripts

### Python API Client

```python
import requests

class CrustaisonClient:
    def __init__(self, base_url="http://localhost:8080"):
        self.base_url = base_url
    
    def chat(self, message):
        return requests.post(f"{self.base_url}/chat", json={
            "message": message
        }).json()
    
    def execute(self, command, params=None):
        return requests.post(f"{self.base_url}/execute", json={
            "command": command,
            "parameters": params or {}
        }).json()
    
    def remember(self, key, value, record_type=None):
        return requests.post(f"{self.base_url}/memory/store", json={
            "key": key,
            "value": value,
            "record_type": record_type
        }).json()
    
    def recall(self, key):
        return requests.post(f"{self.base_url}/memory/recall", json={
            "key": key
        }).json()
```

### Shell Aliases

```bash
# Add to ~/.bashrc or ~/.zshrc
alias crust="curl -s -X POST http://localhost:8080/chat -H 'Content-Type: application/json' -d"
alias crust-exec="curl -s -X POST http://localhost:8080/execute -H 'Content-Type: application/json' -d"

# Usage
crust '{"message": "What'\''s the weather?"}'
```

## Monitoring

```bash
# Health check
curl http://localhost:8080/health

# Get run logs
curl http://localhost:8080/logs

# View reflection history
curl http://localhost:8080/reflections
```
