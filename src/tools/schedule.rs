//! Schedule Tool - Lets Crusty queue tasks for the heartbeat to execute later

use crate::tools::tool::{Tool, ToolResult};
use crate::runtime::scheduler::{TaskQueue, ScheduledTask, TaskAction, TaskStatus};
use async_trait::async_trait;
use chrono::{Utc, Duration};
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
                    "enum": ["weather", "command", "reminder", "web_fetch"],
                    "description": "CRITICAL: Use 'weather' to fetch live weather (requires 'location'). Use 'reminder' ONLY for plain text reminders. Use 'command' to run shell commands. Use 'web_fetch' to fetch a URL."
                },
                "delay_minutes": {
                    "type": "integer",
                    "description": "Minutes from now to execute",
                    "minimum": 1,
                    "maximum": 10080
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
            "required": ["action", "delay_minutes", "description"]
        })
    }

    async fn call(&self, args: serde_json::Value) -> ToolResult {
        let action_type = args.get("action").and_then(|v| v.as_str()).unwrap_or("reminder");
        let delay_min = args.get("delay_minutes").and_then(|v| v.as_i64()).unwrap_or(5);
        let description = args.get("description").and_then(|v| v.as_str()).unwrap_or("Scheduled task");
        let desc_lower = description.to_lowercase();

        let due_at = Utc::now() + Duration::minutes(delay_min);

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
        let due_str = due_local.format("%I:%M %p").to_string();

        let task = ScheduledTask {
            id: task_id.clone(),
            action,
            due_at,
            created_at: Utc::now(),
            chat_id: self.default_chat_id,
            status: TaskStatus::Pending,
            result: None,
            description: description.to_string(),
        };

        match self.queue.add(task).await {
            Ok(_) => ToolResult::ok(format!("Scheduled '{}' for {} (task {})", description, due_str, &task_id[..8])),
            Err(e) => ToolResult::err(format!("Failed to schedule: {}", e)),
        }
    }
}
