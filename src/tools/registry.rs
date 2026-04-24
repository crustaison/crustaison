//! Tool Registry
//!
//! Manages all available tools and provides lookup by name.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::tools::{Tool, ToolResult};

/// Registry of available tools
pub struct ToolRegistry {
    tools: RwLock<HashMap<String, Arc<dyn Tool>>>,
}

impl ToolRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            tools: RwLock::new(HashMap::new()),
        }
    }
    
    /// Register a tool
    pub async fn register<T: Tool + 'static>(&self, tool: T) {
        let name = tool.name().to_string();
        let tool: Arc<dyn Tool> = Arc::new(tool);
        let mut tools = self.tools.write().await;
        tools.insert(name, tool);
    }
    
    /// Get a tool by name
    pub async fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        let tools = self.tools.read().await;
        tools.get(name).cloned()
    }
    
    /// List all tool names
    pub async fn list(&self) -> Vec<String> {
        let tools = self.tools.read().await;
        tools.keys().cloned().collect()
    }
    
    /// Check if a tool exists
    pub async fn has(&self, name: &str) -> bool {
        let tools = self.tools.read().await;
        tools.contains_key(name)
    }

    /// Resolve a possibly-misspelled or hallucinated tool name to a real one.
    ///
    /// Lookup order:
    ///   1. Exact match
    ///   2. Alias table (common model hallucinations)
    ///   3. Case-insensitive substring match (either direction)
    ///   4. Jaro-Winkler similarity >= 0.78
    pub async fn resolve(&self, name: &str) -> Option<String> {
        let tools = self.tools.read().await;

        if tools.contains_key(name) {
            return Some(name.to_string());
        }

        let alias: Option<&str> = match name {
            "gmail_fetch_unread" | "gmail_read" | "gmail_inbox" => Some("gmail"),
            "shell" | "run_bash" | "run_shell" | "execute" => Some("exec"),
            "fs" | "fs_read" | "file_read" | "read_file" => Some("files"),
            "search" | "web" => Some("web_search"),
            _ => None,
        };
        if let Some(a) = alias {
            if tools.contains_key(a) {
                return Some(a.to_string());
            }
        }

        let lname = name.to_lowercase();
        for k in tools.keys() {
            let lk = k.to_lowercase();
            if lk.contains(&lname) || lname.contains(&lk) {
                return Some(k.clone());
            }
        }

        let best = tools.keys()
            .map(|k| (k.clone(), strsim::jaro_winkler(k, name)))
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        if let Some((k, score)) = best {
            if score >= 0.78 {
                return Some(k);
            }
        }

        None
    }
    
    /// Get tool descriptions
    pub async fn descriptions(&self) -> Vec<(String, String, serde_json::Value)> {
        let tools = self.tools.read().await;
        tools.iter()
            .map(|(name, tool)| (name.clone(), tool.description().to_string(), tool.parameters()))
            .collect()
    }
    
    /// Execute a tool by name
    pub async fn execute(&self, name: &str, args: serde_json::Value) -> ToolResult {
        let tools = self.tools.read().await;
        if let Some(tool) = tools.get(name) {
            tool.call(args).await
        } else {
            ToolResult::err(format!("Tool not found: {}", name))
        }
    }
    
    /// Get the schema for all tools (for LLM context)
    pub async fn schema(&self) -> serde_json::Value {
        let tools = self.tools.read().await;
        serde_json::json!({
            "tools": tools.iter().map(|(name, tool)| {
                serde_json::json!({
                    "name": name,
                    "description": tool.description(),
                    "parameters": tool.parameters()
                })
            }).collect::<Vec<_>>()
        })
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
