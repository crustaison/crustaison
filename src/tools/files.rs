//! Files Tool - File Operations
//!
//! Read, write, list, and search files with safety checks.

use crate::tools::{Tool, ToolResult};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{PathBuf, Path};
use tokio::fs::{File, OpenOptions, read_dir};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use walkdir::WalkDir;

/// Configuration for file operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesConfig {
    /// Allowed base directories
    pub allowed_dirs: Vec<String>,
    /// Blocked patterns (gitignore style)
    pub blocked_patterns: Vec<String>,
    /// Maximum file size to read (bytes)
    pub max_file_size: usize,
}

impl Default for FilesConfig {
    fn default() -> Self {
        Self {
            allowed_dirs: vec![
                "/home/sean".to_string(),
                "/home/sean/clawd".to_string(),
                "/home/sean/crustaison".to_string(),
            ],
            blocked_patterns: vec![
                "*.pem".to_string(),
                "*.key".to_string(),
                "*.secret".to_string(),
                ".env".to_string(),
                "/etc/passwd".to_string(),
                "/etc/shadow".to_string(),
            ],
            max_file_size: 1024 * 1024, // 1MB
        }
    }
}

/// Check if a path is allowed
fn is_allowed(path: &Path, allowed_dirs: &[String]) -> bool {
    for dir in allowed_dirs {
        let allowed = PathBuf::from(dir);
        if path.starts_with(&allowed) || path == allowed {
            return true;
        }
    }
    false
}

/// Check if a path matches blocked patterns
fn is_blocked(path: &Path, patterns: &[String]) -> bool {
    let path_str = path.to_string_lossy().to_lowercase();
    for pattern in patterns {
        if path_str.contains(&pattern.to_lowercase()) {
            return true;
        }
        // Handle glob patterns
        if let Ok(glob) = glob::Pattern::new(pattern) {
            if glob.matches(&path.to_string_lossy()) {
                return true;
            }
        }
    }
    false
}

/// Files tool - read, write, list files
pub struct FilesTool {
    config: FilesConfig,
}

impl FilesTool {
    pub fn new() -> Self {
        Self {
            config: FilesConfig::default(),
        }
    }
    
    pub fn with_config(config: FilesConfig) -> Self {
        Self { config }
    }
}

impl Default for FilesTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Tool for FilesTool {
    fn name(&self) -> &str {
        "files"
    }
    
    fn description(&self) -> &str {
        "Read, write, list, and search files. Supports glob patterns for searching."
    }
    
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["read", "write", "list", "glob", "search", "info", "exists"],
                    "description": "The action to perform"
                },
                "path": {
                    "type": "string",
                    "description": "File or directory path"
                },
                "pattern": {
                    "type": "string",
                    "description": "Glob pattern for glob/search actions"
                },
                "content": {
                    "type": "string", 
                    "description": "Content to write (for write action)"
                },
                "recursive": {
                    "type": "boolean",
                    "description": "Recursive search (default: true)"
                }
            },
            "required": ["action", "path"]
        })
    }
    
    async fn call(&self, args: serde_json::Value) -> ToolResult {
        let action = match args.get("action").and_then(|v| v.as_str()) {
            Some(a) => a,
            None => return ToolResult::err("Missing 'action' parameter"),
        };
        
        let path_str = match args.get("path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ToolResult::err("Missing 'path' parameter"),
        };
        
        let path = PathBuf::from(path_str);
        
        // Safety check
        if !is_allowed(&path, &self.config.allowed_dirs) {
            return ToolResult::err(format!("Path not allowed: {}", path_str));
        }
        if is_blocked(&path, &self.config.blocked_patterns) {
            return ToolResult::err(format!("Path blocked: {}", path_str));
        }
        
        match action {
            "read" => {
                self.read_file(&path).await
            }
            "write" => {
                let content = match args.get("content").and_then(|v| v.as_str()) {
                    Some(c) => c.to_string(),
                    None => return ToolResult::err("Missing 'content' for write"),
                };
                self.write_file(&path, &content).await
            }
            "list" => {
                self.list_dir(&path).await
            }
            "glob" => {
                let pattern = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("*");
                self.glob(&path, pattern).await
            }
            "search" => {
                let query = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
                let recursive = args.get("recursive").and_then(|v| v.as_bool()).unwrap_or(true);
                self.search(&path, query, recursive).await
            }
            "info" => {
                self.file_info(&path).await
            }
            "exists" => {
                self.exists(&path).await
            }
            _ => ToolResult::err(format!("Unknown action: {}", action)),
        }
    }
}

impl FilesTool {
    async fn read_file(&self, path: &PathBuf) -> ToolResult {
        match tokio::fs::metadata(path).await {
            Ok(m) if m.len() > self.config.max_file_size as u64 => {
                return ToolResult::err(format!("File too large: {} bytes (max: {})", 
                    m.len(), self.config.max_file_size));
            }
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return ToolResult::err(format!("File not found: {}", path.display()));
            }
            Err(e) => return ToolResult::err(format!("Failed to read metadata: {}", e)),
        }
        
        let mut file = match File::open(path).await {
            Ok(f) => f,
            Err(e) => return ToolResult::err(format!("Failed to open: {}", e)),
        };
        
        let mut contents = String::new();
        if let Err(e) = file.read_to_string(&mut contents).await {
            return ToolResult::err(format!("Failed to read: {}", e));
        }
        
        ToolResult::ok(contents)
    }
    
    async fn write_file(&self, path: &PathBuf, content: &str) -> ToolResult {
        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            if let Err(e) = tokio::fs::create_dir_all(parent).await {
                return ToolResult::err(format!("Failed to create directories: {}", e));
            }
        }
        
        let mut file = match OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path).await
        {
            Ok(f) => f,
            Err(e) => return ToolResult::err(format!("Failed to open for writing: {}", e)),
        };
        
        if let Err(e) = file.write_all(content.as_bytes()).await {
            return ToolResult::err(format!("Failed to write: {}", e));
        }
        
        ToolResult::ok(format!("Wrote {} bytes to {}", content.len(), path.display()))
    }
    
    async fn list_dir(&self, path: &PathBuf) -> ToolResult {
        let mut entries = String::new();
        entries.push_str(&format!("Contents of {}:\n", path.display()));
        
        let mut dir = match read_dir(path).await {
            Ok(d) => d,
            Err(e) => return ToolResult::err(format!("Failed to list directory: {}", e)),
        };
        
        while let Some(entry) = dir.next_entry().await.unwrap_or(None) {
            let path = entry.path();
            let name = entry.file_name().into_string().unwrap_or_else(|_| "[invalid]".to_string());
            let is_dir = entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false);
            let prefix = if is_dir { "[DIR]  " } else { "[FILE] " };
            entries.push_str(&format!("{}{}\n", prefix, name));
        }
        
        ToolResult::ok(entries)
    }
    
    async fn glob(&self, base: &PathBuf, pattern: &str) -> ToolResult {
        let search_base = if base.is_file() {
            base.parent().unwrap_or(base)
        } else {
            base
        };
        
        let full_pattern = format!("{}/**/{}", search_base.display(), pattern);
        let mut matches = Vec::new();
        
        for entry in WalkDir::new(search_base)
            .follow_links(false)
            .max_depth(5)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            let path_str = path.to_string_lossy();
            let file_name = path.file_name().unwrap_or_default().to_string_lossy();
            
            // Simple pattern matching (not full glob)
            if pattern.contains('*') {
                let glob_pattern = pattern.replace("**/*", "").replace("**/", "");
                if glob_pattern.is_empty() || file_name.contains(&glob_pattern.replace("*", "")) {
                    if is_allowed(path, &self.config.allowed_dirs) 
                        && !is_blocked(path, &self.config.blocked_patterns) {
                        matches.push(path_str.to_string());
                    }
                }
            } else if file_name == pattern {
                matches.push(path_str.to_string());
            }
        }
        
        if matches.is_empty() {
            ToolResult::ok(format!("No matches for '{}' in {}", pattern, search_base.display()))
        } else {
            matches.sort();
            ToolResult::ok(format!("Found {} matches:\n{}", matches.len(), matches.join("\n")))
        }
    }
    
    async fn search(&self, base: &PathBuf, query: &str, recursive: bool) -> ToolResult {
        if query.is_empty() {
            return ToolResult::err("Missing search query");
        }
        
        let query = query.to_lowercase();
        let max_depth = if recursive { 10 } else { 1 };
        let mut results = Vec::new();
        
        for entry in WalkDir::new(base)
            .max_depth(max_depth)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if !is_allowed(path, &self.config.allowed_dirs) {
                continue;
            }
            if is_blocked(path, &self.config.blocked_patterns) {
                continue;
            }
            
            if let Ok(content) = fs::read_to_string(path) {
                if content.to_lowercase().contains(&query) {
                    if let Ok(metadata) = fs::metadata(path) {
                        let modified: chrono::DateTime<chrono::Local> = metadata.modified()
                            .unwrap_or_else(|_| std::time::SystemTime::UNIX_EPOCH)
                            .into();
                        results.push(format!(
                            "{}:{} (modified: {})",
                            path.display(),
                            content.lines().position(|l| l.to_lowercase().contains(&query))
                                .map(|i| i + 1)
                                .unwrap_or(0),
                            modified.format("%Y-%m-%d %H:%M")
                        ));
                    }
                }
            }
        }
        
        if results.is_empty() {
            ToolResult::ok(format!("No matches for '{}' in {}", query, base.display()))
        } else {
            ToolResult::ok(format!("Found {} matches:\n{}", results.len(), results.join("\n")))
        }
    }
    
    async fn file_info(&self, path: &PathBuf) -> ToolResult {
        match tokio::fs::metadata(path).await {
            Ok(metadata) => {
                // Convert modified time to ISO string
                let modified = metadata.modified()
                    .ok()
                    .map(|t| chrono::DateTime::<chrono::Local>::from(t).format("%Y-%m-%d %H:%M:%S").to_string())
                    .unwrap_or("unknown".to_string());
                
                let info = serde_json::json!({
                    "path": path.display().to_string(),
                    "size": metadata.len(),
                    "is_file": metadata.is_file(),
                    "is_dir": metadata.is_dir(),
                    "readable": metadata.permissions().readonly() == false,
                    "modified": modified,
                });
                ToolResult::ok(serde_json::to_string_pretty(&info).unwrap_or_default())
            }
            Err(e) => ToolResult::err(format!("Failed to get info: {}", e)),
        }
    }
    
    async fn exists(&self, path: &PathBuf) -> ToolResult {
        let exists = path.exists();
        ToolResult::ok(format!("{} exists: {}", path.display(), exists))
    }
}
