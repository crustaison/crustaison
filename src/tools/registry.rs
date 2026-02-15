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
