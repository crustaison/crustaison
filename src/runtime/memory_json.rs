//! Memory JSON - Working Memory
//!
//! Working memory for the current session. This is cognitive input,
//! not authoritative state.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Working memory contents
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkingMemory {
    pub current_context: String,
    pub recent_messages: Vec<Message>,
    pub active_goals: Vec<Goal>,
    pub pending_tasks: Vec<Task>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Goal {
    pub id: String,
    pub description: String,
    pub status: String,
    pub priority: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub description: String,
    pub status: String,
}

/// Working memory manager
pub struct MemoryJson {
    path: PathBuf,
    memory: WorkingMemory,
}

impl MemoryJson {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            memory: WorkingMemory::default(),
        }
    }
    
    /// Load working memory from file
    pub async fn load(&mut self) -> Result<()> {
        match tokio::fs::read_to_string(&self.path).await {
            Ok(content) => {
                self.memory = serde_json::from_str(&content)
                    .unwrap_or_default();
                Ok(())
            }
            Err(_) => Ok(()), // File doesn't exist yet
        }
    }
    
    /// Save working memory to file
    pub async fn save(&self) -> Result<()> {
        let content = serde_json::to_string_pretty(&self.memory)
            .context("Failed to serialize working memory")?;
        tokio::fs::write(&self.path, content).await
            .context("Failed to write working memory")?;
        Ok(())
    }
    
    /// Add a message to working memory
    pub fn add_message(&mut self, role: &str, content: &str) {
        self.memory.recent_messages.push(Message {
            role: role.to_string(),
            content: content.to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
        });
    }
    
    /// Get working memory reference
    pub fn get(&self) -> &WorkingMemory {
        &self.memory
    }
    
    /// Get mutable working memory reference
    pub fn get_mut(&mut self) -> &mut WorkingMemory {
        &mut self.memory
    }
}
