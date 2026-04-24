//! Tool-call telemetry listener.
//!
//! Subscribes to PostToolUse / PostToolUseFailure antenna signals and appends
//! each event as a JSON line to `~/.local/share/crustaison/tool_calls.jsonl`.
//! Cheap Voyager training data and usage telemetry — reveals which tools are
//! hot, which fail most, and which never get called (removal candidates).

use std::path::PathBuf;

use tokio::io::AsyncWriteExt;

use crate::antennae::{AntennaListener, AntennaOutcome, AntennaSignal};

pub struct TelemetryListener {
    path: PathBuf,
}

impl TelemetryListener {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Default location: ~/.local/share/crustaison/tool_calls.jsonl
    pub fn default_path() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        PathBuf::from(home).join(".local/share/crustaison/tool_calls.jsonl")
    }

    async fn append(&self, entry: &serde_json::Value) {
        if let Some(parent) = self.path.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }
        match tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .await
        {
            Ok(mut f) => {
                let line = format!("{}\n", entry);
                if let Err(e) = f.write_all(line.as_bytes()).await {
                    tracing::warn!("telemetry write failed: {}", e);
                }
            }
            Err(e) => tracing::warn!("telemetry open failed: {}", e),
        }
    }
}

#[async_trait::async_trait]
impl AntennaListener for TelemetryListener {
    fn name(&self) -> &str {
        "tool_call_telemetry"
    }

    async fn receive(&self, signal: &AntennaSignal) -> AntennaOutcome {
        let ts = chrono::Utc::now().to_rfc3339();
        let entry = match signal {
            AntennaSignal::PostToolUse {
                tool, args, success, output,
            } => {
                // Truncate output to keep the JSONL file manageable.
                let out = if output.len() > 500 {
                    format!("{}…[truncated {} bytes]", &output[..500], output.len() - 500)
                } else {
                    output.clone()
                };
                Some(serde_json::json!({
                    "ts": ts,
                    "event": "PostToolUse",
                    "tool": tool,
                    "args": args,
                    "success": success,
                    "output": out,
                }))
            }
            AntennaSignal::PostToolUseFailure { tool, args, error } => {
                Some(serde_json::json!({
                    "ts": ts,
                    "event": "PostToolUseFailure",
                    "tool": tool,
                    "args": args,
                    "error": error,
                }))
            }
            _ => None,
        };
        if let Some(e) = entry {
            self.append(&e).await;
        }
        AntennaOutcome::Continue
    }
}
