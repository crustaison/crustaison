# Heartbeat Implementation: Nexa Watchdog for Crusty

## Overview

Implement a lightweight heartbeat system that uses the **local Nexa provider** (NPU-accelerated, zero token cost) to periodically monitor system state. When the Nexa model determines something needs attention, it alerts Crusty's Telegram chat — only then does MiniMax get involved.

**Goal**: Save MiniMax tokens by using free local inference for routine monitoring.

## Architecture

```
┌─────────────────────────────────────────────┐
│              Heartbeat Service               │
│                                              │
│  ┌──────────┐    ┌─────────────────────┐    │
│  │  Timer    │───>│  Check Functions     │    │
│  │ (60-300s) │    │  - system_health()   │    │
│  │           │    │  - service_check()   │    │
│  │           │    │  - disk_memory()     │    │
│  │           │    │  - pending_tasks()   │    │
│  └──────────┘    └──────────┬──────────┘    │
│                              │               │
│                    Raw check results         │
│                              │               │
│                   ┌──────────▼──────────┐    │
│                   │   Nexa Provider      │    │
│                   │   (local, free)      │    │
│                   │                      │    │
│                   │  "Analyze these      │    │
│                   │   check results.     │    │
│                   │   Is anything wrong  │    │
│                   │   or noteworthy?"    │    │
│                   └──────────┬──────────┘    │
│                              │               │
│                  ┌───────────┴────────┐      │
│                  │                    │      │
│              ALL_CLEAR          ALERT_NEEDED │
│              (do nothing)            │      │
│                              ┌───────▼────┐ │
│                              │  Telegram   │ │
│                              │  Alert Msg  │ │
│                              │  to Sean    │ │
│                              └────────────┘ │
└─────────────────────────────────────────────┘
```

## Implementation Steps

### Step 1: Create the Heartbeat Runner (`src/runtime/heartbeat.rs`)

Replace the existing stub with a real execution engine. Keep `HeartbeatConfig` and `HeartbeatTask` but add:

```rust
pub struct HeartbeatRunner {
    nexa: NexaProvider,
    interval_secs: u64,          // Default: 300 (5 min)
    checks: Vec<Box<dyn Check>>,
    alert_tx: tokio::sync::mpsc::Sender<String>,
    last_alerts: HashMap<String, Instant>,  // Rate limiting
}
```

**Key methods:**
- `start()` — Spawns a tokio task with `tokio::time::interval`
- `run_checks()` — Executes all check functions, collects results
- `analyze()` — Sends collected results to Nexa with analysis prompt
- `alert()` — Sends message via the alert channel (Telegram)
- `stop()` — Cancellation via `tokio_util::sync::CancellationToken`

### Step 2: Define Check Functions

Create `src/runtime/checks.rs` with a simple trait:

```rust
#[async_trait]
pub trait Check: Send + Sync {
    fn name(&self) -> &str;
    async fn run(&self) -> CheckResult;
}

pub struct CheckResult {
    pub name: String,
    pub status: CheckStatus,  // Ok, Warning, Critical
    pub message: String,
    pub value: Option<f64>,   // For numeric metrics
}

pub enum CheckStatus {
    Ok,
    Warning,
    Critical,
}
```

**Built-in checks to implement:**

1. **DiskSpaceCheck** — `df -h /` parsed, warn if >85% used
2. **MemoryCheck** — `/proc/meminfo` parsed, warn if <10% free
3. **ServiceCheck** — Check if key services are running:
   - Nexa server (localhost:18181)
   - Ollama (localhost:11434)
   - Telegram bot (self — just a heartbeat timestamp)
4. **DockerCheck** — `docker ps` to verify expected containers are healthy
5. **UptimeCheck** — Report system uptime, warn if recent reboot
6. **LoadCheck** — System load average, warn if >4.0 on Pi

Each check should be fast (no network calls except service pings) and return within 5 seconds.

### Step 3: Nexa Analysis Prompt

After collecting check results, format them and send to Nexa:

```
You are a system monitoring assistant. Analyze these health check results and determine if anything needs human attention.

Check Results:
- Disk Space: OK (62% used)
- Memory: OK (45% used, 1.2GB free)
- Nexa Service: OK (responding on :18181)
- Ollama Service: WARNING (not responding on :11434)
- Docker: OK (6/6 containers running)
- System Load: OK (1.2 average)
- Uptime: OK (14 days)

Respond with EXACTLY one of:
1. "ALL_CLEAR" if everything looks normal
2. "ALERT: <brief description>" if something needs attention

Be concise. Only alert for actual problems, not minor fluctuations.
```

**Parse the Nexa response:**
- If starts with "ALL_CLEAR" → log and continue
- If starts with "ALERT:" → extract message, send to Telegram

### Step 4: Telegram Alert Integration

The heartbeat sends alerts directly to Telegram without going through Agent/MiniMax.

**Direct Telegram API call (no MiniMax tokens):**

```rust
impl HeartbeatRunner {
    async fn send_telegram_alert(&self, message: &str) {
        let url = format!(
            "https://api.telegram.org/bot{}/sendMessage",
            self.bot_token
        );
        let payload = serde_json::json!({
            "chat_id": self.alert_chat_id,
            "text": format!("🔔 *Heartbeat Alert*\n\n{}", message),
            "parse_mode": "Markdown"
        });
        reqwest::Client::new()
            .post(&url)
            .json(&payload)
            .send()
            .await
            .ok();
    }
}
```

Alternatively, use an mpsc channel so the Telegram bot task handles the actual sending.

### Step 5: Wire Into Main

In `main.rs`, when starting the Telegram bot:

```rust
// After agent initialization but before telegram polling

// Create Nexa provider for heartbeat (local, free)
let nexa_provider = NexaProvider::new(
    "localhost".to_string(),
    18181,
    "qwen2.5-3b".to_string(),
);

// Alert channel: heartbeat -> telegram
let (alert_tx, mut alert_rx) = tokio::sync::mpsc::channel::<String>(32);

// Build heartbeat with checks
let mut heartbeat_runner = HeartbeatRunner::new(nexa_provider, 300, alert_tx);
heartbeat_runner.add_check(Box::new(DiskSpaceCheck::new(85)));
heartbeat_runner.add_check(Box::new(MemoryCheck::new(90)));
heartbeat_runner.add_check(Box::new(ServiceCheck::new(vec![
    ("Nexa", "localhost", 18181),
    ("Ollama", "localhost", 11434),
])));
heartbeat_runner.add_check(Box::new(LoadCheck::new(4.0)));
heartbeat_runner.add_check(Box::new(UptimeCheck::new()));

// Spawn heartbeat loop
tokio::spawn(async move {
    heartbeat_runner.start().await;
});

// Spawn alert forwarder to Telegram
let bot_clone = bot.clone();
let alert_chat_id = /* Sean's Telegram chat ID from config */;
tokio::spawn(async move {
    while let Some(alert_msg) = alert_rx.recv().await {
        let text = format!("🔔 Heartbeat Alert\n\n{}", alert_msg);
        bot_clone.send_message(ChatId(alert_chat_id), text)
            .await
            .ok();
    }
});
```

### Step 6: Configuration

Add to the config struct and toml:

```toml
[heartbeat]
enabled = true
interval_secs = 300
nexa_host = "localhost"
nexa_port = 18181
nexa_model = "qwen2.5-3b"
alert_cooldown_secs = 3600   # Don't repeat same alert within 1 hour

[heartbeat.checks]
disk_space = true
memory = true
services = true
docker = true
uptime = true
load = true

[heartbeat.thresholds]
disk_warn_percent = 85
memory_warn_percent = 90
load_warn = 4.0
```

### Step 7: Telegram Commands

Add these to the Telegram handler's command matcher:

- `/heartbeat` — Force an immediate check cycle, show all results
- `/heartbeat status` — Show last check time, all results, next scheduled
- `/heartbeat on` / `/heartbeat off` — Enable/disable at runtime

## File Changes Summary

| File | Action | Description |
|------|--------|-------------|
| `src/runtime/heartbeat.rs` | **Rewrite** | Replace stub with HeartbeatRunner execution engine |
| `src/runtime/checks.rs` | **New file** | Check trait + 6 built-in check implementations |
| `src/runtime/mod.rs` | **Edit** | Export new checks module |
| `src/main.rs` | **Edit** | Wire heartbeat into telegram startup, spawn alert channel |
| `src/telegram/mod.rs` | **Edit** | Add /heartbeat command handlers |
| `src/config/config.rs` | **Edit** | Add HeartbeatConfig section to config struct |

## Important Notes

1. **Nexa must be running** on localhost:18181 for AI analysis to work. If Nexa is down, fall back to simple threshold-based alerting (no AI analysis needed — just check if values exceed thresholds directly).

2. **Keep prompts SHORT** — Nexa's qwen2.5-3b has 8K context. The analysis prompt + check results should stay under 500 tokens total.

3. **Rate limit alerts** — Don't spam Telegram. Track `last_alert_time` per check name. Same alert only fires once per `alert_cooldown_secs` (default 1 hour).

4. **The heartbeat runs alongside the telegram bot** — same process, separate tokio tasks. No separate binary needed.

5. **Token savings**:
   - 288 checks/day (every 5 min) x ~200 tokens = ~57,600 Nexa tokens/day (FREE, local)
   - MiniMax only used when Sean actually messages Crusty
   - Zero cloud cost for monitoring

6. **Nexa port**: The existing `nexa.rs` has port 11434 as default (Ollama's port). For the heartbeat, use port **18181** which is the actual Nexa server on ryz.local. Either update the config or pass it explicitly.

## Future Extensions

Once basic heartbeat works:
- **Ozark Marina checks**: Ping loto.serverpit.com API, verify Docker containers on base.local
- **Daily summaries**: "3 reservations tomorrow, all drivers confirmed" — sent at 8 PM
- **Custom checks via Telegram**: `/heartbeat add "docker ps | grep ozark"`
- **Escalation to Crusty**: For actionable alerts, inject into Agent context so Crusty can use tools to investigate/fix automatically
