# Heartbeat Task Scheduler: Crusty's Autonomous Timer

## Overview

Evolve the heartbeat from a health-only monitor into Crusty's **autonomous task scheduler**. When a user says "check the weather in 20 minutes" or "remind me about X at 5pm", Crusty writes a scheduled task to a queue. The heartbeat picks it up when it's due and executes it — sending results to Telegram automatically.

Health checks become just one type of scheduled task.

## Architecture

```
User: "check weather in 20 min"
        │
        ▼
┌─────────────────────┐
│  Crusty (MiniMax)    │
│  Parses intent:      │
│  action=weather      │
│  due=now+20min       │
│                      │
│  Uses 'schedule'     │
│  tool to queue task  │
└────────┬────────────┘
         │ writes
         ▼
┌─────────────────────┐
│  Task Queue          │
│  ~/.config/          │
│  crustaison/         │
│  scheduled_tasks.json│
└────────┬────────────┘
         │ checked every cycle
         ▼
┌─────────────────────────────────────┐
│  Heartbeat Runner (every 5 min)     │
│                                     │
│  1. Run health checks (existing)    │
│  2. Check scheduled task queue  ◄── NEW
│     - Any tasks past due_at?        │
│     - Execute them                  │
│     - Send results to Telegram      │
│     - Mark complete                 │
│                                     │
│  3. Analyze with Nexa (existing)    │
└─────────────────────────────────────┘
```

## Implementation Steps

### Step 1: Define ScheduledTask (`src/runtime/scheduler.rs` — NEW FILE)

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledTask {
    pub id: String,                    // UUID
    pub action: TaskAction,            // What to do
    pub due_at: DateTime<Utc>,         // When to do it
    pub created_at: DateTime<Utc>,     // When it was scheduled
    pub chat_id: i64,                  // Telegram chat to send results
    pub status: TaskStatus,            // pending, running, completed, failed
    pub result: Option<String>,        // Output after execution
    pub description: String,           // Human-readable "check weather in Eldon"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskAction {
    /// Check weather for a location
    Weather { location: String },
    /// Run a shell command
    Command { command: String },
    /// Send a reminder message
    Reminder { message: String },
    /// Fetch a URL and summarize
    WebFetch { url: String },
    /// Custom — pass to Nexa/agent for interpretation
    Custom { prompt: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

/// Task queue backed by a JSON file
pub struct TaskQueue {
    path: PathBuf,
}

impl TaskQueue {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Load all tasks from disk
    pub async fn load(&self) -> Vec<ScheduledTask> {
        match fs::read_to_string(&self.path).await {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    }

    /// Save all tasks to disk
    pub async fn save(&self, tasks: &[ScheduledTask]) -> std::io::Result<()> {
        let content = serde_json::to_string_pretty(tasks)?;
        fs::write(&self.path, content).await
    }

    /// Add a new task
    pub async fn add(&self, task: ScheduledTask) -> std::io::Result<()> {
        let mut tasks = self.load().await;
        tasks.push(task);
        self.save(&tasks).await
    }

    /// Get all pending tasks that are past due
    pub async fn get_due_tasks(&self) -> Vec<ScheduledTask> {
        let now = Utc::now();
        self.load().await.into_iter()
            .filter(|t| t.status == TaskStatus::Pending && t.due_at <= now)
            .collect()
    }

    /// Mark a task as completed with result
    pub async fn complete_task(&self, task_id: &str, result: String) -> std::io::Result<()> {
        let mut tasks = self.load().await;
        if let Some(task) = tasks.iter_mut().find(|t| t.id == task_id) {
            task.status = TaskStatus::Completed;
            task.result = Some(result);
        }
        self.save(&tasks).await
    }

    /// Mark a task as failed
    pub async fn fail_task(&self, task_id: &str, error: String) -> std::io::Result<()> {
        let mut tasks = self.load().await;
        if let Some(task) = tasks.iter_mut().find(|t| t.id == task_id) {
            task.status = TaskStatus::Failed;
            task.result = Some(error);
        }
        self.save(&tasks).await
    }

    /// List pending tasks (for /schedule list command)
    pub async fn list_pending(&self) -> Vec<ScheduledTask> {
        self.load().await.into_iter()
            .filter(|t| t.status == TaskStatus::Pending)
            .collect()
    }

    /// Cancel a task by ID
    pub async fn cancel(&self, task_id: &str) -> std::io::Result<bool> {
        let mut tasks = self.load().await;
        if let Some(task) = tasks.iter_mut().find(|t| t.id == task_id && t.status == TaskStatus::Pending) {
            task.status = TaskStatus::Failed;
            task.result = Some("Cancelled by user".to_string());
            self.save(&tasks).await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Clean up old completed/failed tasks (keep last 50)
    pub async fn cleanup(&self) -> std::io::Result<()> {
        let mut tasks = self.load().await;
        let pending: Vec<_> = tasks.iter().filter(|t| t.status == TaskStatus::Pending).cloned().collect();
        let mut done: Vec<_> = tasks.iter().filter(|t| t.status != TaskStatus::Pending).cloned().collect();
        done.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        done.truncate(50);
        let mut all = pending;
        all.extend(done);
        self.save(&all).await
    }
}
```

### Step 2: Add 'schedule' Tool (`src/tools/schedule.rs` — NEW FILE)

This is the tool Crusty calls when the user asks for something to happen later.

```rust
use crate::tools::tool::{Tool, ToolResult};
use crate::runtime::scheduler::{TaskQueue, ScheduledTask, TaskAction, TaskStatus};
use async_trait::async_trait;
use chrono::{Utc, Duration};
use std::sync::Arc;
use uuid::Uuid;

pub struct ScheduleTool {
    queue: Arc<TaskQueue>,
    default_chat_id: i64,
}

impl ScheduleTool {
    pub fn new(queue: Arc<TaskQueue>, chat_id: i64) -> Self {
        Self { queue, default_chat_id: chat_id }
    }
}

#[async_trait]
impl Tool for ScheduleTool {
    fn name(&self) -> &str { "schedule" }

    fn description(&self) -> &str {
        "Schedule a task to run later. Use for reminders, delayed weather checks, \
         timed commands, or anything the user wants done in the future. \
         Supports: weather checks, shell commands, reminders, web fetches."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["weather", "command", "reminder", "web_fetch", "custom"],
                    "description": "Type of task to schedule"
                },
                "delay_minutes": {
                    "type": "integer",
                    "description": "Minutes from now to execute (e.g., 20 for '20 minutes')",
                    "minimum": 1,
                    "maximum": 10080
                },
                "description": {
                    "type": "string",
                    "description": "Human-readable description (e.g., 'Check weather in Eldon')"
                },
                "location": {
                    "type": "string",
                    "description": "For weather action: location to check"
                },
                "command": {
                    "type": "string",
                    "description": "For command action: shell command to run"
                },
                "message": {
                    "type": "string",
                    "description": "For reminder action: message to send"
                },
                "url": {
                    "type": "string",
                    "description": "For web_fetch action: URL to fetch"
                },
                "prompt": {
                    "type": "string",
                    "description": "For custom action: prompt for Nexa to interpret"
                }
            },
            "required": ["action", "delay_minutes", "description"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> ToolResult {
        let action_type = args.get("action").and_then(|v| v.as_str()).unwrap_or("reminder");
        let delay_min = args.get("delay_minutes").and_then(|v| v.as_i64()).unwrap_or(5);
        let description = args.get("description").and_then(|v| v.as_str()).unwrap_or("Scheduled task");

        let due_at = Utc::now() + Duration::minutes(delay_min);

        let action = match action_type {
            "weather" => TaskAction::Weather {
                location: args.get("location").and_then(|v| v.as_str()).unwrap_or("Eldon, Missouri").to_string(),
            },
            "command" => TaskAction::Command {
                command: args.get("command").and_then(|v| v.as_str()).unwrap_or("echo 'no command'").to_string(),
            },
            "reminder" => TaskAction::Reminder {
                message: args.get("message").and_then(|v| v.as_str()).unwrap_or(description).to_string(),
            },
            "web_fetch" => TaskAction::WebFetch {
                url: args.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            },
            "custom" => TaskAction::Custom {
                prompt: args.get("prompt").and_then(|v| v.as_str()).unwrap_or(description).to_string(),
            },
            _ => TaskAction::Reminder {
                message: description.to_string(),
            },
        };

        let task = ScheduledTask {
            id: Uuid::new_v4().to_string(),
            action,
            due_at,
            created_at: Utc::now(),
            chat_id: self.default_chat_id,
            status: TaskStatus::Pending,
            result: None,
            description: description.to_string(),
        };

        let task_id = task.id.clone();
        let due_str = due_at.format("%I:%M %p").to_string();

        match self.queue.add(task).await {
            Ok(_) => ToolResult {
                success: true,
                output: format!("Scheduled '{}' for {} (task {})", description, due_str, &task_id[..8]),
                error: None,
                metadata: None,
            },
            Err(e) => ToolResult::err(&format!("Failed to schedule: {}", e)),
        }
    }
}
```

### Step 3: Add Task Execution to HeartbeatRunner (`src/runtime/heartbeat.rs`)

Add to the heartbeat's `run_cycle()` method:

```rust
// In HeartbeatRunner, add a TaskQueue field:
pub struct HeartbeatRunner {
    // ... existing fields ...
    task_queue: Option<Arc<TaskQueue>>,
    bot_token: Option<String>,
}

// In run_cycle(), after health checks:
async fn run_cycle(&mut self) {
    // 1. Run health checks (existing)
    let results = self.run_checks().await;
    let should_alert = self.analyze_with_nexa(&results).await;
    if should_alert {
        let message = self.format_alert(&results);
        self.send_alert(&message).await;
    }

    // 2. Process scheduled tasks (NEW)
    if let Some(ref queue) = self.task_queue {
        let due_tasks = queue.get_due_tasks().await;
        for task in due_tasks {
            let result = self.execute_scheduled_task(&task).await;
            match result {
                Ok(output) => {
                    let _ = queue.complete_task(&task.id, output.clone()).await;
                    // Send result to Telegram
                    let msg = format!("⏰ Scheduled Task Complete\n\n📋 {}\n\n{}", task.description, output);
                    self.send_to_chat(task.chat_id, &msg).await;
                }
                Err(e) => {
                    let _ = queue.fail_task(&task.id, e.to_string()).await;
                    let msg = format!("⏰ Scheduled Task Failed\n\n📋 {}\n\n❌ {}", task.description, e);
                    self.send_to_chat(task.chat_id, &msg).await;
                }
            }
        }
        // Cleanup old tasks periodically
        let _ = queue.cleanup().await;
    }
}

/// Execute a scheduled task based on its action type
async fn execute_scheduled_task(&self, task: &ScheduledTask) -> Result<String, String> {
    match &task.action {
        TaskAction::Weather { location } => {
            // Use wttr.in for weather (same as web tool)
            let url = format!("https://wttr.in/{}?format=3", location);
            match reqwest::get(&url).await {
                Ok(resp) => resp.text().await.map_err(|e| e.to_string()),
                Err(e) => Err(format!("Weather fetch failed: {}", e)),
            }
        }
        TaskAction::Command { command } => {
            let output = tokio::process::Command::new("bash")
                .arg("-c")
                .arg(command)
                .output()
                .await
                .map_err(|e| e.to_string())?;
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            if output.status.success() {
                Ok(stdout.to_string())
            } else {
                Err(format!("{}\n{}", stdout, stderr))
            }
        }
        TaskAction::Reminder { message } => {
            Ok(format!("🔔 Reminder: {}", message))
        }
        TaskAction::WebFetch { url } => {
            match reqwest::get(url).await {
                Ok(resp) => {
                    let text = resp.text().await.map_err(|e| e.to_string())?;
                    // Truncate to reasonable size
                    Ok(text.chars().take(2000).collect())
                }
                Err(e) => Err(format!("Fetch failed: {}", e)),
            }
        }
        TaskAction::Custom { prompt } => {
            // Use Nexa to interpret and respond
            if let Some(ref nexa) = self.nexa {
                let messages = vec![ChatMessage {
                    role: "user".to_string(),
                    content: prompt.clone(),
                }];
                match nexa.chat(messages, None).await {
                    Ok(response) => Ok(response.content),
                    Err(e) => Err(format!("Nexa error: {}", e)),
                }
            } else {
                Ok(format!("Custom task: {}", prompt))
            }
        }
    }
}

/// Send a message to a specific Telegram chat
async fn send_to_chat(&self, chat_id: i64, message: &str) {
    if let Some(ref token) = self.bot_token {
        let url = format!("https://api.telegram.org/bot{}/sendMessage", token);
        let _ = reqwest::Client::new()
            .post(&url)
            .json(&serde_json::json!({
                "chat_id": chat_id,
                "text": message,
            }))
            .send()
            .await;
    } else {
        // Fallback: use alert channel
        let _ = self.alert_tx.send(message.to_string()).await;
    }
}
```

### Step 4: Wire Into Main

In `main.rs`, when creating the heartbeat:

```rust
// Create shared task queue
let task_queue_path = config.runtime.memory_json_path.parent()
    .unwrap_or(&PathBuf::from("~/.config/crustaison"))
    .join("scheduled_tasks.json");
let task_queue = std::sync::Arc::new(TaskQueue::new(task_queue_path));

// Create heartbeat with task queue and bot token
let mut heartbeat_runner = HeartbeatRunner::new(
    heartbeat_config,
    Some(nexa_provider),
    alert_tx,
);
heartbeat_runner.set_task_queue(task_queue.clone());
heartbeat_runner.set_bot_token(bot_token.clone());

// Register schedule tool with the agent's tool registry
let schedule_tool = ScheduleTool::new(task_queue.clone(), allowed_users[0]);
tool_registry.register(Box::new(schedule_tool)).await;
```

### Step 5: Add Telegram Commands

Add to the Telegram handler:

- `/schedule list` — Show all pending scheduled tasks
- `/schedule cancel <id>` — Cancel a pending task
- `/schedule clear` — Cancel all pending tasks

### Step 6: Update `runtime/mod.rs`

```rust
pub mod scheduler;
pub use scheduler::{TaskQueue, ScheduledTask, TaskAction, TaskStatus};
```

### Step 7: Update `tools/mod.rs`

```rust
pub mod schedule;
pub use schedule::ScheduleTool;
```

## File Changes Summary

| File | Action | Description |
|------|--------|-------------|
| `src/runtime/scheduler.rs` | **New** | TaskQueue, ScheduledTask, TaskAction, TaskStatus |
| `src/runtime/mod.rs` | **Edit** | Export scheduler module |
| `src/tools/schedule.rs` | **New** | Schedule tool for the agent |
| `src/tools/mod.rs` | **Edit** | Export schedule tool |
| `src/runtime/heartbeat.rs` | **Edit** | Add task queue processing to run_cycle() |
| `src/main.rs` | **Edit** | Wire task queue into heartbeat + register schedule tool |
| `src/telegram/mod.rs` | **Edit** | Add /schedule commands |
| `Cargo.toml` | **Edit** | Add `uuid` dependency |

## Example Conversations

**User**: "Check the weather in 20 minutes"
**Crusty**: *calls schedule tool*
```json
{"tool": "schedule", "arguments": {"action": "weather", "delay_minutes": 20, "location": "Eldon, Missouri", "description": "Check weather in Eldon"}}
```
**Crusty**: "Done — I'll check the weather at 3:22 PM and send you the update."

20 minutes later, Telegram notification:
> ⏰ Scheduled Task Complete
> 📋 Check weather in Eldon
> Eldon, Missouri: ⛅ 72°F

---

**User**: "Remind me to call the marina at 5pm"
**Crusty**: *calls schedule tool*
```json
{"tool": "schedule", "arguments": {"action": "reminder", "delay_minutes": 120, "message": "Call the marina", "description": "Reminder to call marina"}}
```

At 5pm:
> ⏰ Scheduled Task Complete
> 📋 Reminder to call marina
> 🔔 Reminder: Call the marina

---

**User**: "Run `docker ps` in 10 minutes and tell me what's up"
**Crusty**: *calls schedule tool with command action*

10 minutes later:
> ⏰ Scheduled Task Complete
> 📋 Check docker status
> boats-ozark-api-1  ...  Up 3 hours
> boats-ozark-db-1   ...  Up 3 hours
> ...

## Important Notes

1. **Task queue is a JSON file** — simple, no extra dependencies. Could upgrade to SQLite later but JSON is fine for low-volume scheduling.

2. **Heartbeat interval affects precision** — with 5-minute cycles, tasks execute within 0-5 minutes of their due time. For most use cases (weather, reminders) this is fine. If precision matters, reduce interval to 60 seconds for this feature.

3. **Bot token in heartbeat** — The heartbeat needs the Telegram bot token to send task results directly. Pass it from main.rs when creating the runner.

4. **`uuid` crate needed** — Add `uuid = { version = "1", features = ["v4"] }` to Cargo.toml for task IDs.

5. **Task persistence** — Tasks survive restarts because they're in a JSON file. On startup, any overdue pending tasks execute immediately on the first heartbeat cycle.

6. **Security** — The `command` action type runs arbitrary shell commands on a timer. The executor policy check should still apply. Consider routing scheduled commands through the same policy as live tool calls.
