//! Tool Trait and Types
//!
//! Defines the interface all tools must implement.

use serde::{Deserialize, Serialize};
use std::fmt;

/// A tool that the LLM can invoke
#[async_trait::async_trait]
pub trait Tool: Send + Sync {
    /// The tool's name
    fn name(&self) -> &str;
    
    /// A brief description of what the tool does
    fn description(&self) -> &str;
    
    /// Parameter schema (JSON Schema format)
    fn parameters(&self) -> serde_json::Value;
    
    /// Execute the tool with the given arguments
    async fn call(&self, args: serde_json::Value) -> ToolResult;
}

/// Result from a tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub success: bool,
    pub output: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

impl fmt::Display for ToolResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.success {
            write!(f, "{}", self.output)
        } else {
            write!(f, "Error: {}", self.error.as_ref().unwrap_or(&"Unknown error".to_string()))
        }
    }
}

impl ToolResult {
    pub fn ok(output: impl Into<String>) -> Self {
        Self {
            success: true,
            output: output.into(),
            error: None,
            metadata: None,
        }
    }
    
    pub fn err(error: impl Into<String>) -> Self {
        Self {
            success: false,
            output: String::new(),
            error: Some(error.into()),
            metadata: None,
        }
    }
}

/// Errors that can occur during tool execution
#[derive(Debug, Clone, thiserror::Error)]
pub enum ToolError {
    #[error("Invalid arguments: {0}")]
    InvalidArguments(String),
    
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),
    
    #[error("Tool not found: {0}")]
    NotFound(String),
    
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
    
    #[error("IO error: {0}")]
    IoError(String),
}

impl From<ToolError> for ToolResult {
    fn from(e: ToolError) -> Self {
        Self::err(e.to_string())
    }
}
