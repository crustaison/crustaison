//! Policy - Safety Constraints
//!
//! Policies define what the agent is allowed and not allowed to do.
//! These live in the authority layer and CANNOT be modified by the agent.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Policy types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Policy {
    /// Allow/disallow specific commands
    Command {
        name: String,
        allowed: bool,
        reason: Option<String>,
    },
    /// Rate limits for operations
    RateLimit {
        operation: String,
        max_per_minute: u32,
    },
    /// Restricted file paths
    FileAccess {
        allowed_paths: Vec<String>,
        denied_paths: Vec<String>,
    },
    /// Network restrictions
    Network {
        allowed_domains: Vec<String>,
        denied_domains: Vec<String>,
    },
}

/// Policy engine that evaluates actions against policies
pub struct PolicyEngine {
    policies: Vec<Policy>,
}

impl PolicyEngine {
    pub fn new() -> Self {
        Self {
            policies: vec![
                // Default policies
                Policy::Command {
                    name: "exec".to_string(),
                    allowed: true,
                    reason: Some("Shell execution allowed with safety checks".to_string()),
                },
                Policy::FileAccess {
                    allowed_paths: vec!["~/crustaison".to_string()],
                    denied_paths: vec!["/etc".to_string(), "/root".to_string(), "/System".to_string()],
                },
                Policy::Network {
                    allowed_domains: vec!["*".to_string()],
                    denied_domains: vec!["*.onion".to_string(), "localhost".to_string()],
                },
            ],
        }
    }
    
    /// Check if an action is allowed
    pub fn check(&self, action: &str, context: &HashMap<String, serde_json::Value>) -> bool {
        // Simplified check - in real implementation would evaluate all policies
        match action {
            "exec" => {
                // Would check command against allowed/denied lists
                true
            }
            "read_file" => {
                // Would check path against allowed/denied
                true
            }
            "write_file" => {
                // Would check path against allowed/denied
                true
            }
            _ => true,
        }
    }
}

impl Default for PolicyEngine {
    fn default() -> Self {
        Self::new()
    }
}
