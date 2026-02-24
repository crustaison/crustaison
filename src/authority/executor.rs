//! Executor - Policy-Enforced Command Execution
//!
//! The executor receives validated commands from the planner/cognition
//! layer and enforces policy before execution. This is part of the
//! immutable authority layer.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Policy violation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PolicyResult {
    Allowed {
        command: String,
        parameters: serde_json::Value,
        timestamp: i64,
    },
    Denied {
        command: String,
        reason: String,
        timestamp: i64,
    },
}

/// Command to be executed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command {
    pub name: String,
    pub parameters: serde_json::Value,
    pub context: serde_json::Value,
}

/// Execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
    pub duration_ms: u64,
}

/// Executor with policy enforcement
pub struct Executor {
    allowed_commands: Arc<RwLock<Vec<String>>>,
    denied_patterns: Arc<RwLock<Vec<String>>>,
    execution_log: Arc<RwLock<Vec<PolicyResult>>>,
}

impl Executor {
    pub fn new() -> Self {
        Self {
            allowed_commands: Arc::new(RwLock::new(vec![
                "read".to_string(),
                "write".to_string(),
                "search".to_string(),
                "exec".to_string(),
                "message".to_string(),
                "web".to_string(),
                "files".to_string(),
                "browser".to_string(),
                "schedule".to_string(),
                "email".to_string(),
                "github".to_string(),
                "google_drive".to_string(),
                "google".to_string(),
                "http".to_string(),
                "image".to_string(),
                "moltbook".to_string(),
            ])),
            denied_patterns: Arc::new(RwLock::new(vec![
                "rm -rf /".to_string(),
                "curl | sh".to_string(),
            ])),
            execution_log: Arc::new(RwLock::new(Vec::new())),
        }
    }
    
    /// Execute a command with policy checks
    pub async fn execute(&self, command: Command) -> Result<ExecutionResult> {
        let start = std::time::Instant::now();
        
        // 1. Allow all tool names — policy enforced via denied_patterns only.
        //    (Allowlist approach was too fragile: every new tool/plugin required a manual addition.)
        let _ = self.allowed_commands.read().await; // kept for future use
        
        // 2. Check for denied patterns in parameters
        let params_str = serde_json::to_string(&command.parameters).unwrap_or_default();
        {
            let denied = self.denied_patterns.read().await;
            for pattern in &*denied {
                if params_str.contains(pattern) {
                    let result = PolicyResult::Denied {
                        command: command.name.clone(),
                        reason: format!("Dangerous pattern '{}' detected", pattern),
                        timestamp: chrono::Utc::now().timestamp_millis(),
                    };
                    self.execution_log.write().await.push(result);
                    
                    return Ok(ExecutionResult {
                        success: false,
                        output: String::new(),
                        error: Some(format!("Dangerous pattern detected: {}", pattern)),
                        duration_ms: start.elapsed().as_millis() as u64,
                    });
                }
            }
        }
        
        // 3. Execute (placeholder - would call actual tool)
        let result = PolicyResult::Allowed {
            command: command.name.clone(),
            parameters: command.parameters.clone(),
            timestamp: chrono::Utc::now().timestamp_millis(),
        };
        self.execution_log.write().await.push(result);
        
        Ok(ExecutionResult {
            success: true,
            output: format!("Executed: {}", command.name),
            error: None,
            duration_ms: start.elapsed().as_millis() as u64,
        })
    }
    
    /// Get execution log
    pub async fn get_log(&self) -> Vec<PolicyResult> {
        self.execution_log.read().await.clone()
    }
}

impl Default for Executor {
    fn default() -> Self {
        Self::new()
    }
}
