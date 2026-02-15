//! Memory System - Journal and Context Store
//!
//! Markdown-based persistent memory: daily journal and named contexts.

use serde::{Deserialize, Serialize};
use chrono::{DateTime, Local};
use std::path::{PathBuf, Path};
use std::fs;
use std::io;

/// A journal entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEntry {
    pub date: String,
    pub content: String,
    pub created_at: i64,
}

/// A named context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Context {
    pub name: String,
    pub content: String,
    pub updated_at: i64,
}

/// Memory manager
pub struct MemoryManager {
    base_path: PathBuf,
}

impl MemoryManager {
    /// Create new memory manager
    pub fn new(base_path: PathBuf) -> Self {
        // Ensure directories exist
        let journal_path = base_path.join("journal");
        let context_path = base_path.join("contexts");
        
        if !journal_path.exists() {
            let _ = fs::create_dir_all(&journal_path);
        }
        if !context_path.exists() {
            let _ = fs::create_dir_all(&context_path);
        }
        
        Self { base_path }
    }
    
    /// Get today's date string
    fn today() -> String {
        Local::now().format("%Y-%m-%d").to_string()
    }
    
    // === Journal ===
    
    /// Write to today's journal
    pub async fn journal_write(&self, content: &str) -> Result<JournalEntry, io::Error> {
        let date = Self::today();
        let path = self.base_path.join("journal").join(format!("{}.md", date));
        
        let timestamp = Local::now().timestamp_millis();
        
        // Append to journal
        let entry = format!("\n## {}\n\n{}", 
            Local::now().format("%H:%M"),
            content
        );
        
        // Read existing content, append new entry
        let mut existing = fs::read_to_string(&path).unwrap_or_default();
        existing.push_str(&entry);
        fs::write(&path, &existing)?;
        
        Ok(JournalEntry {
            date,
            content: content.to_string(),
            created_at: timestamp,
        })
    }
    
    /// Read today's journal
    pub async fn journal_read_today(&self) -> Result<Option<JournalEntry>, io::Error> {
        let date = Self::today();
        self.journal_read(&date).await
    }
    
    /// Read a journal entry by date
    pub async fn journal_read(&self, date: &str) -> Result<Option<JournalEntry>, io::Error> {
        let path = self.base_path.join("journal").join(format!("{}.md", date));
        
        if path.exists() {
            let content = fs::read_to_string(&path)?;
            Ok(Some(JournalEntry {
                date: date.to_string(),
                content,
                created_at: 0,
            }))
        } else {
            Ok(None)
        }
    }
    
    /// List all journal entries
    pub async fn journal_list(&self) -> Result<Vec<String>, io::Error> {
        let journal_path = self.base_path.join("journal");
        let mut entries = Vec::new();
        
        if let Ok(entries_iter) = fs::read_dir(&journal_path) {
            for entry in entries_iter.flatten() {
                if entry.path().extension().map(|e| e.to_string_lossy() == "md").unwrap_or(false) {
                    if let Some(name) = entry.path().file_stem().map(|n| n.to_string_lossy().to_string()) {
                        entries.push(name);
                    }
                }
            }
        }
        
        entries.sort_by(|a, b| b.cmp(a)); // Most recent first
        Ok(entries)
    }
    
    // === Contexts ===
    
    /// Save a named context
    pub async fn context_save(&self, name: &str, content: &str) -> Result<Context, io::Error> {
        let path = self.base_path.join("contexts").join(format!("{}.md", name));
        let timestamp = Local::now().timestamp_millis();
        
        fs::write(&path, content)?;
        
        Ok(Context {
            name: name.to_string(),
            content: content.to_string(),
            updated_at: timestamp,
        })
    }
    
    /// Load a named context
    pub async fn context_load(&self, name: &str) -> Result<Option<Context>, io::Error> {
        let path = self.base_path.join("contexts").join(format!("{}.md", name));
        
        if path.exists() {
            let content = fs::read_to_string(&path)?;
            let metadata = fs::metadata(&path)?;
            let modified: DateTime<Local> = metadata.modified()?.into();
            
            Ok(Some(Context {
                name: name.to_string(),
                content,
                updated_at: modified.timestamp_millis(),
            }))
        } else {
            Ok(None)
        }
    }
    
    /// List all contexts
    pub async fn context_list(&self) -> Result<Vec<String>, io::Error> {
        let context_path = self.base_path.join("contexts");
        let mut names = Vec::new();
        
        if let Ok(entries) = fs::read_dir(&context_path) {
            for entry in entries.flatten() {
                if entry.path().extension().map(|e| e.to_string_lossy() == "md").unwrap_or(false) {
                    if let Some(name) = entry.path().file_stem().map(|n| n.to_string_lossy().to_string()) {
                        names.push(name);
                    }
                }
            }
        }
        
        names.sort();
        Ok(names)
    }
    
    /// Delete a context
    pub async fn context_delete(&self, name: &str) -> Result<(), io::Error> {
        let path = self.base_path.join("contexts").join(format!("{}.md", name));
        
        if path.exists() {
            fs::remove_file(&path)?;
        }
        
        Ok(())
    }
}
