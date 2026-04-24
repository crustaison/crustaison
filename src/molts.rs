//! Molts — named reusable task recipes (crustacean-ified "skills").
//!
//! A molt is a directory containing a `MOLT.md` file with YAML-style
//! frontmatter + a markdown body. Frontmatter field names match ClawdCode's
//! `SkillFrontmatter` verbatim so a Claude-Code skill drops into Crustaison
//! unchanged. Layout:
//!
//!   ~/.config/crustaison/molts/
//!   ├── deploy-autobet/
//!   │   └── MOLT.md
//!   └── scan-leads/
//!       └── MOLT.md
//!
//! Progressive disclosure: only the frontmatter (metadata) is loaded at
//! startup and shown to the LLM. The full markdown body is read from disk
//! only when `recall_molt` is actually invoked.

use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::tools::{Tool, ToolResult};

/// Frontmatter fields for a molt. Field names match ClawdCode's
/// `SkillFrontmatter` (camelCase) so the file is wire-compatible.
#[derive(Debug, Clone)]
pub struct MoltMetadata {
    pub name: String,
    pub description: String,
    pub when_to_use: Option<String>,
    pub allowed_tools: Vec<String>,
    pub argument_hint: Option<String>,
    pub user_invocable: bool,
    pub disable_model_invocation: bool,
    pub version: Option<String>,
    pub path: PathBuf,
}

impl MoltMetadata {
    /// Parse the frontmatter block at the top of a MOLT.md file.
    /// Returns None if there is no valid frontmatter or required fields are
    /// missing.
    pub fn parse(path: PathBuf, content: &str) -> Option<Self> {
        let content = content.trim_start_matches('\u{feff}').trim_start();
        let rest = content.strip_prefix("---")?;
        let rest = rest.strip_prefix('\n').unwrap_or(rest);
        let end = rest.find("\n---")?;
        let frontmatter = &rest[..end];

        let mut name: Option<String> = None;
        let mut description: Option<String> = None;
        let mut when_to_use: Option<String> = None;
        let mut allowed_tools: Vec<String> = Vec::new();
        let mut argument_hint: Option<String> = None;
        let mut user_invocable = false;
        let mut disable_model_invocation = false;
        let mut version: Option<String> = None;

        for line in frontmatter.lines() {
            let line = line.trim_end();
            if line.is_empty() || line.trim_start().starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once(':') else { continue };
            let key = key.trim();
            let value = value.trim().trim_matches('"').trim_matches('\'');
            match key {
                "name" => name = Some(value.to_string()),
                "description" => description = Some(value.to_string()),
                "whenToUse" | "when_to_use" => when_to_use = Some(value.to_string()),
                "argumentHint" | "argument_hint" => argument_hint = Some(value.to_string()),
                "userInvocable" | "user_invocable" => user_invocable = value == "true",
                "disableModelInvocation" | "disable_model_invocation" => {
                    disable_model_invocation = value == "true"
                }
                "version" => version = Some(value.to_string()),
                "allowedTools" | "allowed_tools" => {
                    let inner = value.trim_start_matches('[').trim_end_matches(']');
                    allowed_tools = inner
                        .split(',')
                        .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
                _ => {}
            }
        }

        Some(Self {
            name: name?,
            description: description?,
            when_to_use,
            allowed_tools,
            argument_hint,
            user_invocable,
            disable_model_invocation,
            version,
            path,
        })
    }
}

/// In-memory registry of known molts, scanned from a filesystem root.
pub struct MoltRegistry {
    molts: RwLock<Vec<MoltMetadata>>,
    root: PathBuf,
}

impl MoltRegistry {
    pub fn new(root: PathBuf) -> Self {
        Self {
            molts: RwLock::new(Vec::new()),
            root,
        }
    }

    /// Scan the root for `*/MOLT.md` and parse each frontmatter.
    /// Returns how many molts were loaded. Missing directory is not an error.
    pub async fn scan(&self) -> anyhow::Result<usize> {
        let mut found: Vec<MoltMetadata> = Vec::new();
        if !self.root.exists() {
            let _ = tokio::fs::create_dir_all(&self.root).await;
            *self.molts.write().await = found;
            return Ok(0);
        }
        let mut entries = tokio::fs::read_dir(&self.root).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let molt_path = path.join("MOLT.md");
            if !molt_path.exists() {
                continue;
            }
            let content = match tokio::fs::read_to_string(&molt_path).await {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!("failed to read {:?}: {}", molt_path, e);
                    continue;
                }
            };
            match MoltMetadata::parse(molt_path.clone(), &content) {
                Some(meta) => {
                    tracing::info!("molt loaded: {} — {}", meta.name, meta.description);
                    found.push(meta);
                }
                None => {
                    tracing::warn!("invalid MOLT.md frontmatter: {:?}", molt_path);
                }
            }
        }
        let count = found.len();
        *self.molts.write().await = found;
        Ok(count)
    }

    pub async fn list(&self) -> Vec<MoltMetadata> {
        self.molts.read().await.clone()
    }

    pub async fn find(&self, name: &str) -> Option<MoltMetadata> {
        self.molts
            .read()
            .await
            .iter()
            .find(|m| m.name == name)
            .cloned()
    }

    /// Load the markdown body of a molt. Progressive disclosure — reads from
    /// disk only on invocation. Returns None if the molt is unknown or the
    /// file can't be read.
    pub async fn load_body(&self, name: &str) -> Option<String> {
        let meta = self.find(name).await?;
        let content = tokio::fs::read_to_string(&meta.path).await.ok()?;
        let content = content.trim_start_matches('\u{feff}').trim_start();
        if let Some(rest) = content.strip_prefix("---") {
            let rest = rest.strip_prefix('\n').unwrap_or(rest);
            if let Some(end) = rest.find("\n---") {
                let body_start = end + "\n---".len();
                let body = &rest[body_start..];
                let body = body.strip_prefix('\n').unwrap_or(body);
                return Some(body.trim().to_string());
            }
        }
        Some(content.to_string())
    }

    /// Render a short block listing metadata for injection into the system
    /// prompt. Only `description` + `whenToUse` are surfaced eagerly to keep
    /// the prompt cheap; the body is fetched via `recall_molt`.
    pub async fn render_for_prompt(&self) -> Option<String> {
        let molts = self.list().await;
        if molts.is_empty() {
            return None;
        }
        let mut out = String::new();
        out.push_str("## Molts — Named Reusable Recipes\n");
        out.push_str("These are task recipes you've learned. Call `recall_molt` with the name to load the full step-by-step body when one applies.\n\n");
        for m in &molts {
            out.push_str(&format!("- **{}** — {}", m.name, m.description));
            if let Some(w) = &m.when_to_use {
                out.push_str(&format!(" _(when to use: {})_", w));
            }
            out.push('\n');
        }
        out.push('\n');
        Some(out)
    }
}

/// `recall_molt` meta-tool — loads a molt body on demand.
pub struct RecallMoltTool {
    registry: Arc<MoltRegistry>,
}

impl RecallMoltTool {
    pub fn new(registry: Arc<MoltRegistry>) -> Self {
        Self { registry }
    }
}

#[async_trait::async_trait]
impl Tool for RecallMoltTool {
    fn name(&self) -> &str {
        "recall_molt"
    }

    fn description(&self) -> &str {
        "Recall a named molt (reusable task recipe). Pass the molt's name; \
         returns the full markdown body with step-by-step instructions. Use \
         this when the user's request matches a molt's description or \
         'whenToUse' hint shown in the system prompt. If the name is unknown, \
         the error lists all available molts."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Name of the molt to recall (e.g. 'deploy-autobet')"
                }
            },
            "required": ["name"]
        })
    }

    async fn call(&self, args: serde_json::Value) -> ToolResult {
        let Some(name) = args.get("name").and_then(|v| v.as_str()) else {
            return ToolResult::err("missing required arg: name");
        };
        match self.registry.load_body(name).await {
            Some(body) => ToolResult::ok(body),
            None => {
                let available = self
                    .registry
                    .list()
                    .await
                    .iter()
                    .map(|m| m.name.clone())
                    .collect::<Vec<_>>()
                    .join(", ");
                let msg = if available.is_empty() {
                    format!("no molt named '{}'. no molts are registered yet", name)
                } else {
                    format!("no molt named '{}'. available: {}", name, available)
                };
                ToolResult::err(msg)
            }
        }
    }
}
