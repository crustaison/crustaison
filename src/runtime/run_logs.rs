//! Run Logs - Execution Logs
//!
//! Immutable execution logs for audit and debugging.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::io::Write;
use std::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use chrono::Utc;

/// Execution log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: i64,
    pub level: String,
    pub module: String,
    pub message: String,
    pub metadata: Option<serde_json::Value>,
}

/// Run logs manager
pub struct RunLogs {
    logs_dir: PathBuf,
}

impl RunLogs {
    pub fn new(logs_dir: PathBuf) -> Self {
        Self { logs_dir }
    }
    
    /// Initialize logs directory
    pub async fn init(&self) -> Result<(), std::io::Error> {
        tokio::fs::create_dir_all(&self.logs_dir).await
    }
    
    /// Log an entry
    pub async fn log(&self, level: &str, module: &str, message: &str) {
        let timestamp = Utc::now().timestamp_millis();
        let filename = format!("{}.jsonl", Utc::now().format("%Y-%m-%d"));
        let path = self.logs_dir.join(filename);
        
        let entry = LogEntry {
            timestamp,
            level: level.to_string(),
            module: module.to_string(),
            message: message.to_string(),
            metadata: None,
        };
        
        if let Ok(content) = serde_json::to_string(&entry) {
            let mut file = tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .await
                .ok();
            
            if let Some(ref mut f) = file {
                let _ = f.write_all((content + "\n").as_bytes()).await;
            }
        }
    }
    
    pub fn info(&self, module: &str, message: &str) {
        // Blocking write for simplicity
        let _ = std::fs::create_dir_all(&self.logs_dir);
        let timestamp = Utc::now().timestamp_millis();
        let filename = format!("{}.jsonl", chrono::Local::now().format("%Y-%m-%d"));
        let path = self.logs_dir.join(filename);
        
        let entry = LogEntry {
            timestamp,
            level: "INFO".to_string(),
            module: module.to_string(),
            message: message.to_string(),
            metadata: None,
        };
        
        if let Ok(content) = serde_json::to_string(&entry) {
            let _ = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .map(|mut f| f.write_all(format!("{}\n", content).as_bytes()));
        }
    }
}
