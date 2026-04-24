//! Schedule Tool - Lets Crusty queue tasks for the heartbeat to execute later

use crate::tools::tool::{Tool, ToolResult};
use crate::runtime::scheduler::{TaskQueue, ScheduledTask, TaskAction, TaskStatus};
use async_trait::async_trait;
use chrono::{Utc, Duration, NaiveDateTime};
use std::sync::Arc;

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
        "Schedule a task to run later and push results to Telegram automatically. \
         IMPORTANT: For weather requests, you MUST use action='weather' with a 'location' parameter. \
         Do NOT use 'reminder' for weather — reminder just sends a text message, it does NOT fetch weather. \
         Actions: weather (fetches live weather data for a location), command (runs a shell command), \
         reminder (sends a text-only message), web_fetch (fetches a URL)."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["list", "cancel", "weather", "command", "reminder", "web_fetch"],
                    "description": "CRITICAL: Use 'weather' to fetch live weather (requires 'location'). Use 'reminder' ONLY for plain text reminders. Use 'command' to run shell commands. Use 'web_fetch' to fetch a URL."
                },
                "delay_minutes": {
                    "type": "integer",
                    "description": "Minutes from now to execute (use this OR 'time', not both)",
                    "minimum": 1,
                    "maximum": 525600
                },
                "time": {
                    "type": "string",
                    "description": "Absolute datetime to execute, in ISO 8601 format in Central Time e.g. '2026-03-05T09:00:00'. Use this for future dates. Provide either this OR delay_minutes."
                },
                "description": {
                    "type": "string",
                    "description": "Human-readable description of the task"
                },
                "location": {
                    "type": "string",
                    "description": "REQUIRED when action='weather': the location to check weather for (e.g. 'Eldon, Missouri')"
                },
                "command": {
                    "type": "string",
                    "description": "REQUIRED when action='command': shell command to run"
                },
                "message": {
                    "type": "string",
                    "description": "REQUIRED when action='reminder': message text to send"
                },
                "url": {
                    "type": "string",
                    "description": "REQUIRED when action='web_fetch': URL to fetch"
                }
            },
            "required": ["action", "description"]
        })
    }

    async fn call(&self, args: serde_json::Value) -> ToolResult {
        let action_type = args.get("action").and_then(|v| v.as_str()).unwrap_or("reminder");

        // Handle list action immediately — no delay needed
        if action_type == "list" {
            let tasks = self.queue.list_pending().await;
            if tasks.is_empty() {
                return ToolResult::ok("No pending scheduled tasks.".to_string());
            }
            let tz = chrono::FixedOffset::west_opt(6 * 3600).unwrap();
            let mut output = format!("Scheduled tasks ({} pending):\n\n", tasks.len());
            for t in &tasks {
                let due_local = t.due_at.with_timezone(&tz);
                let due_str = due_local.format("%b %d at %I:%M %p").to_string();
                let action_str = match &t.action {
                    TaskAction::Weather { location } => format!("Weather for {}", location),
                    TaskAction::Command { command } => format!("Run: {}", &command[..command.len().min(40)]),
                    TaskAction::Reminder { message } => format!("Remind: {}", &message[..message.len().min(40)]),
                    TaskAction::WebFetch { url } => format!("Fetch: {}", &url[..url.len().min(40)]),
                    TaskAction::Custom { prompt } => format!("Custom: {}", &prompt[..prompt.len().min(40)]),
                };
                output.push_str(&format!("- [{}] {} — due {}\n", &t.id[..8], action_str, due_str));
            }
            return ToolResult::ok(output);
        }
        // Handle cancel action
        if action_type == "cancel" {
            let task_id = args.get("task_id").and_then(|v| v.as_str()).unwrap_or("");
            if task_id.is_empty() {
                return ToolResult::err("Missing 'task_id' to cancel. Use action='list' to see task IDs.".to_string());
            }
            // Mark task as failed/cancelled
            match self.queue.fail_task(task_id, "Cancelled by user".to_string()).await {
                Ok(_) => return ToolResult::ok(format!("Task {} cancelled.", task_id)),
                Err(e) => return ToolResult::err(format!("Failed to cancel task: {}", e)),
            }
        }

        let description = args.get("description").and_then(|v| v.as_str()).unwrap_or("Scheduled task");
        let desc_lower = description.to_lowercase();

        // Determine due_at: prefer absolute 'time' param, fall back to delay_minutes
        let _ = tracing::info!("DEBUG schedule tool received args: {:?}", args);
        let due_at = if let Some(time_str) = args.get("time").and_then(|v| v.as_str()) {
            let _ = tracing::info!("DEBUG time_str received: {}", time_str);
            // Parse ISO datetime as Central Time (UTC-6 CST / UTC-5 CDT)
            // We treat naive datetimes as CST (UTC-6) since that's the user's timezone
            if let Ok(naive) = NaiveDateTime::parse_from_str(time_str, "%Y-%m-%dT%H:%M:%S")
                .or_else(|_| NaiveDateTime::parse_from_str(time_str, "%Y-%m-%dT%H:%M")) {
                // Treat as Central Time (CST = UTC-6): add 6h to get UTC
                naive.and_utc() + Duration::hours(6)
            } else {
                return ToolResult::err(format!("Could not parse 'time' value '{}'. Use ISO format like '2026-03-05T09:00:00'", time_str));
            }
        } else {
            let delay_min = args.get("delay_minutes").and_then(|v| v.as_i64()).unwrap_or(5);
            Utc::now() + Duration::minutes(delay_min)
        };

        // Smart action correction: if LLM said "reminder" but description mentions weather,
        // auto-correct to weather action
        let effective_action = if action_type == "reminder" && (desc_lower.contains("weather") || desc_lower.contains("forecast") || desc_lower.contains("temperature")) {
            tracing::warn!("Auto-correcting action from 'reminder' to 'weather' based on description: {}", description);
            "weather"
        } else {
            action_type
        };

        let action = match effective_action {
            "weather" => {
                // Try to extract location from location param, or from description
                let location = args.get("location")
                    .and_then(|v| v.as_str())
                    .or_else(|| args.get("message").and_then(|v| v.as_str()))
                    .unwrap_or("Eldon, Missouri")
                    .to_string();
                // Clean up location - strip "Weather update for " etc
                let clean_location = location
                    .replace("Weather update for ", "")
                    .replace("Weather for ", "")
                    .replace("weather in ", "")
                    .replace("weather for ", "")
                    .trim()
                    .to_string();
                let final_location = if clean_location.is_empty() { "Eldon, Missouri".to_string() } else { clean_location };
                TaskAction::Weather { location: final_location }
            }
            "command" => TaskAction::Command {
                command: args.get("command").and_then(|v| v.as_str()).unwrap_or("echo done").to_string(),
            },
            "reminder" => TaskAction::Reminder {
                message: args.get("message").and_then(|v| v.as_str()).unwrap_or(description).to_string(),
            },
            "web_fetch" => TaskAction::WebFetch {
                url: args.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            },
            _ => TaskAction::Reminder {
                message: description.to_string(),
            },
        };

        // Generate task ID from timestamp
        let task_id = format!("{:x}", Utc::now().timestamp_millis());
        let due_local = due_at.with_timezone(&chrono::FixedOffset::west_opt(6 * 3600).unwrap());
        let due_str = due_local.format("%b %d at %I:%M %p").to_string();

        let task = ScheduledTask {
            id: task_id.clone(),
            action,
            due_at,
            created_at: Utc::now(),
            chat_id: self.default_chat_id,
            status: TaskStatus::Pending,
            result: None,
            description: description.to_string(),
            interval_secs: None,
        };

        match self.queue.add(task).await {
            Ok(_) => ToolResult::ok(format!("Scheduled '{}' for {} (task {})", description, due_str, &task_id[..8])),
            Err(e) => ToolResult::err(format!("Failed to schedule: {}", e)),
        }
    }
}
