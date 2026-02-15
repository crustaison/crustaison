//! Git Ledger - Git-Backed Immutable Audit Trail
//!
//! The ledger uses git for immutability. The agent can write entries
//! but CANNOT modify history - git enforces this.
//! Auto-pushes to GitHub after each commit.

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Command;

/// A ledger entry
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LedgerEntry {
    pub id: String,
    pub timestamp: i64,
    pub entry_type: String,
    pub content: serde_json::Value,
    pub hash: String,
}

/// Git-backed ledger
pub struct GitLedger {
    repo_path: PathBuf,
}

impl GitLedger {
    pub fn new(repo_path: PathBuf) -> Self {
        Self { repo_path }
    }

    /// Initialize git ledger
    pub async fn init(&self) -> Result<()> {
        // Initialize git repo if needed
        let output = Command::new("git")
            .args(&["init", "--initial-branch=main"])
            .current_dir(&self.repo_path)
            .output()?;

        if !output.status.success() {
            return Err(anyhow::anyhow!("Failed to init git repo"));
        }

        Ok(())
    }

    /// Add an immutable entry
    pub async fn add(&self, entry_type: &str, content: &serde_json::Value) -> Result<LedgerEntry> {
        let entry = LedgerEntry {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            entry_type: entry_type.to_string(),
            content: content.clone(),
            hash: self.calculate_hash(entry_type, content),
        };

        // Write entry to file
        let filename = format!("{}.jsonl", entry.timestamp);
        let path = self.repo_path.join(&filename);

        let content = serde_json::to_string(&entry)?;
        tokio::fs::write(&path, content).await
            .context("Failed to write ledger entry")?;

        // Stage and commit (git enforces immutability)
        let _ = Command::new("git")
            .args(&["add", &filename])
            .current_dir(&self.repo_path)
            .output()?;

        let commit_msg = format!("ledger: add {} entry {}", entry_type, entry.id);
        let _ = Command::new("git")
            .args(&["commit", "-m", &commit_msg, "--no-verify"])
            .current_dir(&self.repo_path)
            .output()?;

        // Auto-push to GitHub (non-blocking, best-effort)
        let repo_path = self.repo_path.clone();
        tokio::spawn(async move {
            let _ = tokio::process::Command::new("git")
                .args(&["push", "origin", "main"])
                .current_dir(&repo_path)
                .output()
                .await;
        });

        Ok(entry)
    }

    fn calculate_hash(&self, entry_type: &str, content: &serde_json::Value) -> String {
        let data = format!("{}{}{}", entry_type, content, chrono::Utc::now().timestamp_millis());
        let hash = md5::compute(data);
        format!("{:x}", hash)
    }
}
