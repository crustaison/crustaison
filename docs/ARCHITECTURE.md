# Architecture

## Overview

Crustaison follows a layered architecture with clear separation of concerns:

```
┌─────────────────────────────────────────────┐
│              TUI / Telegram                 │  ← Interface Layer
├─────────────────────────────────────────────┤
│              API / Routes                   │  ← REST API Layer
├─────────────────────────────────────────────┤
│              Agent (Brain)                  │  ← Core Logic
├─────────────┬─────────────┬───────────────┤
│   Provider  │   Tools     │   Memory      │  ← Capabilities
├─────────────┴─────────────┴───────────────┤
│            Authority (Safety)               │  ← Immutable
├─────────────────────────────────────────────┤
│            Ledger (Audit)                  │  ← Immutable
└─────────────────────────────────────────────┘
```

## Layers

### 1. Interface Layer
Entry points for user interaction:
- **TUI**: Terminal user interface with ratatui
- **Telegram**: Bot integration via teloxide
- **REST API**: HTTP endpoints via warp

### 2. API Layer
Request routing and middleware:
- Authentication
- Rate limiting
- Request validation
- Response formatting

### 3. Core Logic (Agent)
The agent's brain:
- **Provider**: LLM connection (MiniMax, Ollama, Nexa)
- **Tools**: Executable capabilities (exec, files, web)
- **Memory**: Working memory for current session

### 4. Capabilities
Extended features:
- **Sessions**: SQLite-backed conversation history
- **Memory**: Journal and named contexts
- **Vector Store**: Embeddings for semantic search
- **RAG**: Retrieval-Augmented Generation
- **Plugins**: Dynamic extension system
- **Webhooks**: Event notifications

### 5. Safety Layer (Authority)
Immutable safety mechanisms:
- **Gateway**: Rate limiting and auth
- **Executor**: Tool execution with policy checks
- **Policy**: Allowed/denied actions

### 6. Audit Layer (Ledger)
Immutable audit trail:
- Git-backed ledger
- All actions recorded
- Tamper-evident

## Key Modules

### Authority
```
authority/
├── gateway.rs       # Rate limiting, auth
├── executor.rs      # Tool execution
└── policy.rs       # Security policies
```

### Cognition
```
cognition/
├── planner.rs           # Plan generation
├── memory_engine.rs    # Structured memory
├── doctrine_loader.rs  # Load soul.md, agents.md
└── reflection.rs       # Self-assessment
```

### Providers
```
providers/
├── minimax.rs     # Cloud API
├── ollama.rs      # Local Ollama
└── nexa.rs       # Local Nexa
```

### Tools
```
tools/
├── exec.rs        # Shell commands
├── files.rs       # File operations
└── web.rs        # Web search
```

## Data Flow

```
User Input
    ↓
[Interface] → Validate → Route
    ↓
[Agent] → LLM → Parse → Execute Tools
    ↓          ↓
    ├─→ Tools → Authority → Ledger
    ↓
[Memory] → Store Context
    ↓
Response → User
```

## Safety First

1. **Tool Whitelist**: Only registered tools can execute
2. **Policy Checks**: Each tool call validated against policy
3. **Destructive Ops**: Require explicit confirmation
4. **Audit Trail**: All actions logged to ledger
5. **Sandbox**: File system access controlled

## Memory Architecture

```
┌─────────────────────────────────────┐
│           Working Memory            │  ← In-memory (memory.json)
├─────────────────────────────────────┤
│         Session Memory              │  ← SQLite (sessions/)
├─────────────────────────────────────┤
│           Journal                   │  ← Files (journal/YYYY-MM-DD.md)
├─────────────────────────────────────┤
│           Contexts                 │  ← Files (contexts/*.md)
├─────────────────────────────────────┤
│        Vector Store                │  ← JSON (embeddings.json)
└─────────────────────────────────────┘
```

## Configuration Layers

```
config.toml (user)
    ↓
defaults (code)
    ↓
environment variables (override)
    ↓
CLI args (override)
```

## Extension Points

1. **Providers**: Add new LLM backends
2. **Tools**: Register new capabilities
3. **Plugins**: Dynamic loading
4. **Middleware**: Add API middleware
