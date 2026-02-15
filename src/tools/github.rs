//! GitHub Tool - Interact with GitHub repos, issues, and code
//!
//! Uses the GitHub API via PAT for repo management, code push, and issue tracking.

use crate::tools::tool::{Tool, ToolResult};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// GitHub configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubConfig {
    pub username: String,
    pub token: String,
}

pub struct GitHubTool {
    config: GitHubConfig,
    client: reqwest::Client,
}

impl GitHubTool {
    pub fn new(config: GitHubConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_default();
        Self { config, client }
    }

    fn auth_header(&self) -> String {
        format!("Bearer {}", self.config.token)
    }
}

#[async_trait]
impl Tool for GitHubTool {
    fn name(&self) -> &str { "github" }

    fn description(&self) -> &str {
        "Interact with GitHub. Actions: 'create_repo' (create a new repository), \
         'list_repos' (list your repositories), 'create_issue' (open an issue), \
         'list_issues' (list issues on a repo), 'push' (git add, commit, push to a repo), \
         'clone' (clone a repository). Use this for all GitHub operations."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["create_repo", "list_repos", "create_issue", "list_issues", "push", "clone", "api"],
                    "description": "GitHub action to perform"
                },
                "repo": {
                    "type": "string",
                    "description": "Repository name (e.g. 'my-project' or 'owner/repo')"
                },
                "description": {
                    "type": "string",
                    "description": "Description for repo or issue"
                },
                "title": {
                    "type": "string",
                    "description": "Title for issue"
                },
                "body": {
                    "type": "string",
                    "description": "Body text for issue"
                },
                "private": {
                    "type": "boolean",
                    "description": "Whether repo should be private (default: false)"
                },
                "path": {
                    "type": "string",
                    "description": "Local path for clone/push operations"
                },
                "message": {
                    "type": "string",
                    "description": "Commit message for push"
                },
                "endpoint": {
                    "type": "string",
                    "description": "API endpoint for raw API calls (e.g. '/repos/owner/repo')"
                }
            },
            "required": ["action"]
        })
    }

    async fn call(&self, args: serde_json::Value) -> ToolResult {
        if self.config.token.is_empty() {
            return ToolResult::err("GitHub not configured — missing token");
        }

        let action = match args.get("action").and_then(|v| v.as_str()) {
            Some(a) => a,
            None => return ToolResult::err("Missing 'action' parameter"),
        };

        match action {
            "create_repo" => self.create_repo(&args).await,
            "list_repos" => self.list_repos().await,
            "create_issue" => self.create_issue(&args).await,
            "list_issues" => self.list_issues(&args).await,
            "push" => self.push(&args).await,
            "clone" => self.clone_repo(&args).await,
            "api" => self.raw_api(&args).await,
            _ => ToolResult::err(format!("Unknown action: {}", action)),
        }
    }
}

impl GitHubTool {
    async fn create_repo(&self, args: &serde_json::Value) -> ToolResult {
        let name = match args.get("repo").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => return ToolResult::err("Missing 'repo' name"),
        };
        let description = args.get("description").and_then(|v| v.as_str()).unwrap_or("");
        let private = args.get("private").and_then(|v| v.as_bool()).unwrap_or(false);

        let body = serde_json::json!({
            "name": name,
            "description": description,
            "private": private,
            "auto_init": true,
        });

        let resp = self.client.post("https://api.github.com/user/repos")
            .header("Authorization", self.auth_header())
            .header("User-Agent", "Crustaison")
            .header("Accept", "application/vnd.github+json")
            .json(&body)
            .send()
            .await;

        match resp {
            Ok(r) => {
                let status = r.status();
                let text = r.text().await.unwrap_or_default();
                if status.is_success() {
                    let json: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
                    let url = json["html_url"].as_str().unwrap_or("unknown");
                    let clone_url = json["clone_url"].as_str().unwrap_or("unknown");
                    ToolResult::ok(format!("Repository created!\n  URL: {}\n  Clone: {}", url, clone_url))
                } else {
                    let json: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
                    let msg = json["message"].as_str().unwrap_or(&text);
                    ToolResult::err(format!("Failed to create repo: {} — {}", status, msg))
                }
            }
            Err(e) => ToolResult::err(format!("Request failed: {}", e)),
        }
    }

    async fn list_repos(&self) -> ToolResult {
        let resp = self.client.get("https://api.github.com/user/repos?sort=updated&per_page=20")
            .header("Authorization", self.auth_header())
            .header("User-Agent", "Crustaison")
            .header("Accept", "application/vnd.github+json")
            .send()
            .await;

        match resp {
            Ok(r) => {
                if !r.status().is_success() {
                    return ToolResult::err(format!("Failed: {}", r.status()));
                }
                let repos: Vec<serde_json::Value> = r.json().await.unwrap_or_default();
                if repos.is_empty() {
                    ToolResult::ok("No repositories found.")
                } else {
                    let lines: Vec<String> = repos.iter().map(|r| {
                        let name = r["full_name"].as_str().unwrap_or("?");
                        let desc = r["description"].as_str().unwrap_or("");
                        let private = if r["private"].as_bool().unwrap_or(false) { " [private]" } else { "" };
                        format!("- {}{} — {}", name, private, desc)
                    }).collect();
                    ToolResult::ok(format!("Repositories ({}):\n{}", repos.len(), lines.join("\n")))
                }
            }
            Err(e) => ToolResult::err(format!("Request failed: {}", e)),
        }
    }

    async fn create_issue(&self, args: &serde_json::Value) -> ToolResult {
        let repo = match args.get("repo").and_then(|v| v.as_str()) {
            Some(r) => {
                if r.contains('/') { r.to_string() } else { format!("{}/{}", self.config.username, r) }
            }
            None => return ToolResult::err("Missing 'repo'"),
        };
        let title = match args.get("title").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => return ToolResult::err("Missing 'title'"),
        };
        let body = args.get("body").and_then(|v| v.as_str()).unwrap_or("");

        let payload = serde_json::json!({
            "title": title,
            "body": body,
        });

        let url = format!("https://api.github.com/repos/{}/issues", repo);
        let resp = self.client.post(&url)
            .header("Authorization", self.auth_header())
            .header("User-Agent", "Crustaison")
            .header("Accept", "application/vnd.github+json")
            .json(&payload)
            .send()
            .await;

        match resp {
            Ok(r) => {
                let status = r.status();
                let text = r.text().await.unwrap_or_default();
                if status.is_success() {
                    let json: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
                    let url = json["html_url"].as_str().unwrap_or("unknown");
                    let number = json["number"].as_u64().unwrap_or(0);
                    ToolResult::ok(format!("Issue #{} created: {}", number, url))
                } else {
                    ToolResult::err(format!("Failed: {} — {}", status, text))
                }
            }
            Err(e) => ToolResult::err(format!("Request failed: {}", e)),
        }
    }

    async fn list_issues(&self, args: &serde_json::Value) -> ToolResult {
        let repo = match args.get("repo").and_then(|v| v.as_str()) {
            Some(r) => {
                if r.contains('/') { r.to_string() } else { format!("{}/{}", self.config.username, r) }
            }
            None => return ToolResult::err("Missing 'repo'"),
        };

        let url = format!("https://api.github.com/repos/{}/issues?state=open&per_page=10", repo);
        let resp = self.client.get(&url)
            .header("Authorization", self.auth_header())
            .header("User-Agent", "Crustaison")
            .header("Accept", "application/vnd.github+json")
            .send()
            .await;

        match resp {
            Ok(r) => {
                if !r.status().is_success() {
                    return ToolResult::err(format!("Failed: {}", r.status()));
                }
                let issues: Vec<serde_json::Value> = r.json().await.unwrap_or_default();
                if issues.is_empty() {
                    ToolResult::ok("No open issues.")
                } else {
                    let lines: Vec<String> = issues.iter().map(|i| {
                        let num = i["number"].as_u64().unwrap_or(0);
                        let title = i["title"].as_str().unwrap_or("?");
                        format!("- #{}: {}", num, title)
                    }).collect();
                    ToolResult::ok(format!("Open issues ({}):\n{}", issues.len(), lines.join("\n")))
                }
            }
            Err(e) => ToolResult::err(format!("Request failed: {}", e)),
        }
    }

    async fn clone_repo(&self, args: &serde_json::Value) -> ToolResult {
        let repo = match args.get("repo").and_then(|v| v.as_str()) {
            Some(r) => {
                if r.contains('/') { r.to_string() } else { format!("{}/{}", self.config.username, r) }
            }
            None => return ToolResult::err("Missing 'repo'"),
        };
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let clone_url = format!("https://github.com/{}.git", repo);

        let mut cmd = tokio::process::Command::new("git");
        cmd.arg("clone").arg(&clone_url);
        if !path.is_empty() {
            cmd.arg(path);
        }

        let output = cmd.output().await;
        match output {
            Ok(o) => {
                if o.status.success() {
                    ToolResult::ok(format!("Cloned {} successfully", repo))
                } else {
                    let stderr = String::from_utf8_lossy(&o.stderr);
                    ToolResult::err(format!("Clone failed: {}", stderr))
                }
            }
            Err(e) => ToolResult::err(format!("Failed to run git: {}", e)),
        }
    }

    async fn push(&self, args: &serde_json::Value) -> ToolResult {
        let path = match args.get("path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ToolResult::err("Missing 'path' (local repo directory)"),
        };
        let message = args.get("message").and_then(|v| v.as_str()).unwrap_or("Update from Crusty");

        // git add, commit, push
        let script = format!(
            "cd '{}' && git add -A && git commit -m '{}' && git push",
            path, message.replace('\'', "'\\''")
        );

        let output = tokio::process::Command::new("bash")
            .arg("-c")
            .arg(&script)
            .output()
            .await;

        match output {
            Ok(o) => {
                let stdout = String::from_utf8_lossy(&o.stdout);
                let stderr = String::from_utf8_lossy(&o.stderr);
                if o.status.success() {
                    ToolResult::ok(format!("Pushed successfully:\n{}", stdout))
                } else {
                    ToolResult::err(format!("Push failed:\n{}\n{}", stdout, stderr))
                }
            }
            Err(e) => ToolResult::err(format!("Failed to run git: {}", e)),
        }
    }

    async fn raw_api(&self, args: &serde_json::Value) -> ToolResult {
        let endpoint = match args.get("endpoint").and_then(|v| v.as_str()) {
            Some(e) => e,
            None => return ToolResult::err("Missing 'endpoint' (e.g. '/repos/owner/repo')"),
        };

        let url = if endpoint.starts_with("https://") {
            endpoint.to_string()
        } else {
            format!("https://api.github.com{}", endpoint)
        };

        let resp = self.client.get(&url)
            .header("Authorization", self.auth_header())
            .header("User-Agent", "Crustaison")
            .header("Accept", "application/vnd.github+json")
            .send()
            .await;

        match resp {
            Ok(r) => {
                let status = r.status();
                let text = r.text().await.unwrap_or_default();
                if status.is_success() {
                    // Truncate long responses
                    let truncated = if text.len() > 3000 {
                        format!("{}...\n[truncated]", &text[..3000])
                    } else {
                        text
                    };
                    ToolResult::ok(truncated)
                } else {
                    ToolResult::err(format!("API error {}: {}", status, text))
                }
            }
            Err(e) => ToolResult::err(format!("Request failed: {}", e)),
        }
    }
}
