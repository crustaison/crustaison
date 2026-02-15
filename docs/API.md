# API Reference

## Endpoints

### POST /chat
Chat with the agent.

**Request:**
```json
{
  "message": "string",
  "source": "string (optional)"
}
```

**Response:**
```json
{
  "success": true,
  "data": {
    "response": "string",
    "source": "string"
  },
  "error": null
}
```

### POST /execute
Execute a tool command.

**Request:**
```json
{
  "command": "read",
  "parameters": {
    "path": "/path/to/file"
  },
  "context": {} // optional
}
```

**Response:**
```json
{
  "success": true,
  "data": {
    "output": "file contents",
    "duration_ms": 45
  },
  "error": null
}
```

### POST /memory/store
Store a memory record.

**Request:**
```json
{
  "key": "user_preference",
  "value": {
    "theme": "dark",
    "language": "en"
  },
  "record_type": "preference" // optional
}
```

**Response:**
```json
{
  "success": true,
  "data": {
    "key": "user_preference"
  },
  "error": null
}
```

### POST /memory/recall
Recall a memory record.

**Request:**
```json
{
  "key": "user_preference"
}
```

**Response:**
```json
{
  "success": true,
  "data": {
    "id": 123,
    "key": "user_preference",
    "value": { "theme": "dark" },
    "created_at": 1707865200000
  },
  "error": null
}
```

### POST /memory/list
List all memory keys.

**Request:** (empty)

**Response:**
```json
{
  "success": true,
  "data": ["key1", "key2", "key3"],
  "error": null
}
```

### POST /plan
Generate a plan.

**Request:**
```json
{
  "goal": "Build a web scraper",
  "context": {} // optional
}
```

**Response:**
```json
{
  "success": true,
  "data": {
    "id": "plan-123",
    "steps": [
      {"action": "search", "description": "Research scrapers"},
      {"action": "write", "description": "Create scraper.py"},
      {"action": "exec", "description": "Test the scraper"}
    ]
  },
  "error": null
}
```

### POST /reflect
Reflect on recent actions.

**Request:**
```json
{
  "topic": "improve code quality"
}
```

**Response:**
```json
{
  "success": true,
  "data": {
    "reflections": [
      "Consider adding more tests",
      "Error handling could be improved"
    ]
  },
  "error": null
}
```

### GET /health
Health check.

**Response:**
```json
{
  "status": "healthy",
  "uptime_seconds": 3600,
  "memory_usage_mb": 45
}
```

## Rate Limiting

- Default: 100 requests per 60 seconds
- Returns 429 Too Many Requests when exceeded

## Error Responses

```json
{
  "success": false,
  "data": null,
  "error": "Error description"
}
```

Common error codes:
- `400 Bad Request` - Invalid request format
- `401 Unauthorized` - Missing/invalid API key
- `429 Too Many Requests` - Rate limit exceeded
- `500 Internal Server Error` - Agent error
