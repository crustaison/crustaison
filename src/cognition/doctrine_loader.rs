//! Doctrine Loader - Loads Identity and Rules
//!
//! Loads soul.md, agents.md, and principles.md as cognitive input.
//! These are markdown files that inform the planner but are NOT authoritative state.

use anyhow::{Context, Result};
use std::path::PathBuf;

/// Doctrine document
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Doctrine {
    pub soul: Option<String>,
    pub agents: Option<String>,
    pub principles: Option<String>,
}

/// Loader for doctrine documents
#[derive(Clone)]
pub struct DoctrineLoader {
    doctrine_path: PathBuf,
}

impl DoctrineLoader {
    pub fn new(doctrine_path: PathBuf) -> Self {
        Self { doctrine_path }
    }
    
    /// Load all doctrine documents
    pub async fn load(&self) -> Result<Doctrine> {
        let soul_path = self.doctrine_path.join("soul.md");
        let agents_path = self.doctrine_path.join("agents.md");
        let principles_path = self.doctrine_path.join("principles.md");
        
        Ok(Doctrine {
            soul: self.read_if_exists(&soul_path).await,
            agents: self.read_if_exists(&agents_path).await,
            principles: self.read_if_exists(&principles_path).await,
        })
    }
    
    async fn read_if_exists(&self, path: &PathBuf) -> Option<String> {
        match tokio::fs::read_to_string(path).await {
            Ok(content) => Some(content),
            Err(_) => None,
        }
    }
}
