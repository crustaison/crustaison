//! Memory Tool - Persistent journal and named context store

use crate::tools::tool::{Tool, ToolResult};
use crate::memory::MemoryManager;
use async_trait::async_trait;
use std::sync::Arc;

pub struct MemoryTool {
    manager: Arc<MemoryManager>,
}

impl MemoryTool {
    pub fn new(manager: Arc<MemoryManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl Tool for MemoryTool {
    fn name(&self) -> &str { "memory" }

    fn description(&self) -> &str {
        "Persistent memory — journal and named context store. \
         Use this to remember things across conversations, save notes, or recall saved information. \
         Actions: \
         'journal_write' — append a note to today's journal; \
         'journal_read' — read today's journal (or a specific date with 'date' param, YYYY-MM-DD); \
         'journal_list' — list all journal dates; \
         'context_save' — save a named blob of text (name + content); \
         'context_load' — load a named context by name; \
         'context_list' — list all saved context names; \
         'context_delete' — delete a named context."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": [
                        "journal_write", "journal_read", "journal_list",
                        "context_save", "context_load", "context_list", "context_delete"
                    ],
                    "description": "The memory operation to perform"
                },
                "content": {
                    "type": "string",
                    "description": "Text to write — required for journal_write and context_save"
                },
                "name": {
                    "type": "string",
                    "description": "Context name — required for context_save, context_load, context_delete"
                },
                "date": {
                    "type": "string",
                    "description": "Date string YYYY-MM-DD — optional for journal_read (defaults to today)"
                }
            },
            "required": ["action"]
        })
    }

    async fn call(&self, args: serde_json::Value) -> ToolResult {
        let action = match args.get("action").and_then(|v| v.as_str()) {
            Some(a) => a,
            None => return ToolResult::err("Missing 'action' parameter"),
        };

        match action {
            "journal_write" => {
                let content = match args.get("content").and_then(|v| v.as_str()) {
                    Some(c) => c,
                    None => return ToolResult::err("journal_write requires 'content'"),
                };
                match self.manager.journal_write(content).await {
                    Ok(entry) => ToolResult::ok(format!("Wrote to journal for {}", entry.date)),
                    Err(e) => ToolResult::err(format!("Failed to write journal: {}", e)),
                }
            }

            "journal_read" => {
                let result = if let Some(date) = args.get("date").and_then(|v| v.as_str()) {
                    self.manager.journal_read(date).await
                } else {
                    self.manager.journal_read_today().await
                };
                match result {
                    Ok(Some(entry)) => ToolResult::ok(format!("Journal [{}]:\n{}", entry.date, entry.content)),
                    Ok(None) => ToolResult::ok("No journal entry found for that date."),
                    Err(e) => ToolResult::err(format!("Failed to read journal: {}", e)),
                }
            }

            "journal_list" => {
                match self.manager.journal_list().await {
                    Ok(dates) if dates.is_empty() => ToolResult::ok("No journal entries yet."),
                    Ok(dates) => ToolResult::ok(format!("Journal dates ({} total):\n{}", dates.len(), dates.join("\n"))),
                    Err(e) => ToolResult::err(format!("Failed to list journal: {}", e)),
                }
            }

            "context_save" => {
                let name = match args.get("name").and_then(|v| v.as_str()) {
                    Some(n) => n,
                    None => return ToolResult::err("context_save requires 'name'"),
                };
                let content = match args.get("content").and_then(|v| v.as_str()) {
                    Some(c) => c,
                    None => return ToolResult::err("context_save requires 'content'"),
                };
                match self.manager.context_save(name, content).await {
                    Ok(_) => ToolResult::ok(format!("Saved context '{}'", name)),
                    Err(e) => ToolResult::err(format!("Failed to save context: {}", e)),
                }
            }

            "context_load" => {
                let name = match args.get("name").and_then(|v| v.as_str()) {
                    Some(n) => n,
                    None => return ToolResult::err("context_load requires 'name'"),
                };
                match self.manager.context_load(name).await {
                    Ok(Some(ctx)) => ToolResult::ok(format!("Context '{}':\n{}", ctx.name, ctx.content)),
                    Ok(None) => ToolResult::ok(format!("No context named '{}'", name)),
                    Err(e) => ToolResult::err(format!("Failed to load context: {}", e)),
                }
            }

            "context_list" => {
                match self.manager.context_list().await {
                    Ok(names) if names.is_empty() => ToolResult::ok("No saved contexts."),
                    Ok(names) => ToolResult::ok(format!("Saved contexts:\n{}", names.join("\n"))),
                    Err(e) => ToolResult::err(format!("Failed to list contexts: {}", e)),
                }
            }

            "context_delete" => {
                let name = match args.get("name").and_then(|v| v.as_str()) {
                    Some(n) => n,
                    None => return ToolResult::err("context_delete requires 'name'"),
                };
                match self.manager.context_delete(name).await {
                    Ok(_) => ToolResult::ok(format!("Deleted context '{}'", name)),
                    Err(e) => ToolResult::err(format!("Failed to delete context: {}", e)),
                }
            }

            other => ToolResult::err(format!("Unknown action: '{}'", other)),
        }
    }
}
