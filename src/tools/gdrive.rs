//! Google Drive Tool - File operations via rclone
//!
//! Requires rclone configured with a named remote (default: gdrive-crusty).

use crate::tools::tool::{Tool, ToolResult};
use async_trait::async_trait;

pub struct GoogleDriveTool {
    remote: String,
}

impl GoogleDriveTool {
    pub fn new(remote: &str) -> Self {
        Self { remote: remote.to_string() }
    }
}

#[async_trait]
impl Tool for GoogleDriveTool {
    fn name(&self) -> &str { "google_drive" }

    fn description(&self) -> &str {
        "Access Google Drive. Actions: \
         'list' (list files in a folder), \
         'read' (read a text file's content), \
         'upload' (upload a local file to Drive), \
         'download' (download a Drive file to local path), \
         'mkdir' (create a folder), \
         'delete' (delete a file or folder), \
         'search' (search for files by name), \
         'about' (show Drive quota/usage info)."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["list", "read", "upload", "download", "mkdir", "delete", "search", "about"],
                    "description": "Action to perform"
                },
                "path": {
                    "type": "string",
                    "description": "Path on Google Drive (e.g. 'folder/file.txt' or '' for root)"
                },
                "local_path": {
                    "type": "string",
                    "description": "Local filesystem path (for upload/download)"
                },
                "query": {
                    "type": "string",
                    "description": "Search query (for search action)"
                },
                "remote": {
                    "type": "string",
                    "description": "Which Drive account to use: 'gdrive-crusty' (default, crusty\'s account) or 'gdrive-clyde' (clyde\'s account)"
                }
            },
            "required": ["action"]
        })
    }

    async fn call(&self, args: serde_json::Value) -> ToolResult {
        let action = match args.get("action").and_then(|v| v.as_str()) {
            Some(a) => a.to_string(),
            None => return ToolResult::err("Missing 'action' parameter"),
        };

        match action.as_str() {
            "list"     => self.list(&args).await,
            "read"     => self.read_file(&args).await,
            "upload"   => self.upload(&args).await,
            "download" => self.download(&args).await,
            "mkdir"    => self.mkdir(&args).await,
            "delete"   => self.delete(&args).await,
            "search"   => self.search(&args).await,
            "about"    => self.about(&args).await,
            _          => ToolResult::err(format!("Unknown action: {}", action)),
        }
    }
}

impl GoogleDriveTool {
    fn remote_path(&self, path: &str) -> String {
        self.remote_path_with(&self.remote, path)
    }

    fn remote_path_with(&self, remote: &str, path: &str) -> String {
        let clean = path.trim_start_matches('/');
        if clean.is_empty() {
            format!("{}:", remote)
        } else {
            format!("{}:{}", remote, clean)
        }
    }

    fn get_remote<'a>(&'a self, args: &'a serde_json::Value) -> &'a str {
        match args.get("remote").and_then(|v| v.as_str()) {
            Some("gdrive-clyde") => "gdrive-clyde",
            _ => &self.remote,
        }
    }

    async fn rclone(&self, args: Vec<&str>) -> Result<String, String> {
        let output = tokio::process::Command::new("rclone")
            .args(&args)
            .output()
            .await
            .map_err(|e| format!("rclone not found: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if output.status.success() {
            Ok(stdout)
        } else {
            Err(if !stderr.is_empty() { stderr } else { stdout })
        }
    }

    async fn list(&self, args: &serde_json::Value) -> ToolResult {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let remote = self.remote_path_with(self.get_remote(args), path);

        match self.rclone(vec!["lsl", "--max-depth", "1", &remote]).await {
            Ok(out) => {
                if out.trim().is_empty() {
                    ToolResult::ok(format!("Drive folder '{}' is empty.", if path.is_empty() { "root" } else { path }))
                } else {
                    ToolResult::ok(format!("Contents of '{}':\n{}", if path.is_empty() { "root" } else { path }, out))
                }
            }
            Err(e) => ToolResult::err(format!("List failed: {}", e)),
        }
    }

    async fn read_file(&self, args: &serde_json::Value) -> ToolResult {
        let path = match args.get("path").and_then(|v| v.as_str()) {
            Some(p) if !p.is_empty() => p.to_string(),
            _ => return ToolResult::err("'path' is required for read action"),
        };
        let remote = self.remote_path_with(self.get_remote(args), &path);
        let tmp = format!("/tmp/crusty_drive_{}", uuid::Uuid::new_v4());

        match self.rclone(vec!["copyto", &remote, &tmp]).await {
            Ok(_) => {
                match tokio::fs::read_to_string(&tmp).await {
                    Ok(content) => {
                        let _ = tokio::fs::remove_file(&tmp).await;
                        let truncated = if content.len() > 4000 {
                            format!("{}...\n[truncated at 4000 chars]", &content[..4000])
                        } else {
                            content
                        };
                        ToolResult::ok(truncated)
                    }
                    Err(e) => {
                        let _ = tokio::fs::remove_file(&tmp).await;
                        ToolResult::err(format!("Failed to read downloaded file: {}", e))
                    }
                }
            }
            Err(e) => ToolResult::err(format!("Download for read failed: {}", e)),
        }
    }

    async fn upload(&self, args: &serde_json::Value) -> ToolResult {
        let local = match args.get("local_path").and_then(|v| v.as_str()) {
            Some(p) => p.to_string(),
            None => return ToolResult::err("'local_path' required for upload"),
        };
        let drive_path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let remote = self.remote_path_with(self.get_remote(args), drive_path);

        match self.rclone(vec!["copyto", &local, &remote]).await {
            Ok(_) => ToolResult::ok(format!("Uploaded '{}' to Drive path '{}'", local, drive_path)),
            Err(e) => ToolResult::err(format!("Upload failed: {}", e)),
        }
    }

    async fn download(&self, args: &serde_json::Value) -> ToolResult {
        let drive_path = match args.get("path").and_then(|v| v.as_str()) {
            Some(p) if !p.is_empty() => p.to_string(),
            _ => return ToolResult::err("'path' required for download"),
        };
        let local = match args.get("local_path").and_then(|v| v.as_str()) {
            Some(p) => p.to_string(),
            None => return ToolResult::err("'local_path' required for download"),
        };
        let remote = self.remote_path(&drive_path);

        match self.rclone(vec!["copyto", &remote, &local]).await {
            Ok(_) => ToolResult::ok(format!("Downloaded Drive '{}' to '{}'", drive_path, local)),
            Err(e) => ToolResult::err(format!("Download failed: {}", e)),
        }
    }

    async fn mkdir(&self, args: &serde_json::Value) -> ToolResult {
        let path = match args.get("path").and_then(|v| v.as_str()) {
            Some(p) if !p.is_empty() => p.to_string(),
            _ => return ToolResult::err("'path' required for mkdir"),
        };
        let remote = self.remote_path_with(self.get_remote(args), &path);

        match self.rclone(vec!["mkdir", &remote]).await {
            Ok(_) => ToolResult::ok(format!("Created folder '{}'", path)),
            Err(e) => ToolResult::err(format!("mkdir failed: {}", e)),
        }
    }

    async fn delete(&self, args: &serde_json::Value) -> ToolResult {
        let path = match args.get("path").and_then(|v| v.as_str()) {
            Some(p) if !p.is_empty() => p.to_string(),
            _ => return ToolResult::err("'path' required for delete"),
        };
        let remote = self.remote_path_with(self.get_remote(args), &path);

        // Try deletefile first, fallback to purge for directories
        match self.rclone(vec!["deletefile", &remote]).await {
            Ok(_) => ToolResult::ok(format!("Deleted '{}'", path)),
            Err(_) => match self.rclone(vec!["purge", &remote]).await {
                Ok(_) => ToolResult::ok(format!("Deleted folder '{}'", path)),
                Err(e) => ToolResult::err(format!("Delete failed: {}", e)),
            }
        }
    }

    async fn search(&self, args: &serde_json::Value) -> ToolResult {
        let query = match args.get("query").and_then(|v| v.as_str()) {
            Some(q) => q.to_string(),
            None => return ToolResult::err("'query' required for search"),
        };
        let remote = self.remote_path_with(self.get_remote(args), "");
        let filter = format!("*{}*", query);

        match self.rclone(vec!["lsl", "--include", &filter, "--max-depth", "5", &remote]).await {
            Ok(out) => {
                if out.trim().is_empty() {
                    ToolResult::ok(format!("No files found matching '{}'", query))
                } else {
                    ToolResult::ok(format!("Files matching '{}':\n{}", query, out))
                }
            }
            Err(e) => ToolResult::err(format!("Search failed: {}", e)),
        }
    }

    async fn about(&self, args: &serde_json::Value) -> ToolResult {
        let remote = format!("{}:", self.get_remote(args));
        match self.rclone(vec!["about", &remote]).await {
            Ok(out) => ToolResult::ok(out),
            Err(e) => ToolResult::err(format!("about failed: {}", e)),
        }
    }
}
