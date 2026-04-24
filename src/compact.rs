//! Doctrine compaction tool — on-demand, not cron.
//!
//! Reads one of Crusty's growing append-only doctrine files (MOLTLOG.md,
//! memory.md), asks MiniMax to compress it to the essentials (drop
//! duplicates, fold related lessons, preserve attribution), and writes the
//! result back atomically with a timestamped `.bak` for safety.
//!
//! Invoked explicitly via the `compact_doctrine` tool. The plan proposes a
//! cron; we leave that to a scheduled task definition the user can add
//! later. Invocation: `recall_molt` + judgment + human oversight is the
//! right trigger for now.

use std::path::PathBuf;
use std::sync::Arc;

use crate::providers::provider::{ChatMessage, Provider};
use crate::tools::{Tool, ToolResult};

/// Which doctrine file the user wants compacted.
fn resolve_target(which: &str) -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let base = PathBuf::from(home).join(".config/crustaison/doctrine");
    match which.to_lowercase().as_str() {
        "moltlog" | "molt_log" | "moltlog.md" => Some(base.join("MOLTLOG.md")),
        "memory" | "memory.md" => Some(base.join("memory.md")),
        _ => None,
    }
}

pub struct CompactDoctrineTool {
    provider: Arc<dyn Provider>,
}

impl CompactDoctrineTool {
    pub fn new(provider: Arc<dyn Provider>) -> Self {
        Self { provider }
    }
}

#[async_trait::async_trait]
impl Tool for CompactDoctrineTool {
    fn name(&self) -> &str {
        "compact_doctrine"
    }

    fn description(&self) -> &str {
        "Compact a growing doctrine file (MOLTLOG.md or memory.md) by asking \
         MiniMax to merge duplicates and fold related entries. Writes a \
         timestamped .bak before overwriting the original. Use when the file \
         is getting long enough to bloat the system prompt."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "which": {
                    "type": "string",
                    "enum": ["moltlog", "memory"],
                    "description": "Which doctrine file to compact"
                },
                "dry_run": {
                    "type": "boolean",
                    "description": "If true, returns the compacted draft without writing. Default false.",
                    "default": false
                }
            },
            "required": ["which"]
        })
    }

    async fn call(&self, args: serde_json::Value) -> ToolResult {
        let Some(which) = args.get("which").and_then(|v| v.as_str()) else {
            return ToolResult::err("missing required arg: which");
        };
        let dry_run = args.get("dry_run").and_then(|v| v.as_bool()).unwrap_or(false);

        let Some(path) = resolve_target(which) else {
            return ToolResult::err(format!("unknown doctrine target: {}", which));
        };

        let original = match tokio::fs::read_to_string(&path).await {
            Ok(s) => s,
            Err(e) => return ToolResult::err(format!("read {:?} failed: {}", path, e)),
        };

        if original.lines().count() < 30 {
            return ToolResult::ok(format!(
                "no-op: {:?} has fewer than 30 lines, not worth compacting",
                path
            ));
        }

        let prompt = format!(
            "Compact the following append-only learnings file. Rules:\n\
             1. Merge duplicate or near-duplicate lessons into one entry.\n\
             2. Drop entries that are contradicted by later ones.\n\
             3. Preserve section headers and formatting.\n\
             4. Keep each entry terse — one line when possible.\n\
             5. Keep attribution/dates if present.\n\
             6. Output ONLY the compacted markdown — no preamble, no explanation.\n\
             \n---FILE ({})---\n{}",
            path.file_name().and_then(|n| n.to_str()).unwrap_or("?"),
            original
        );

        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: prompt,
            images: Vec::new(),
        }];

        let compacted = match self.provider.chat(messages, None).await {
            Ok(r) => r.content,
            Err(e) => return ToolResult::err(format!("provider call failed: {}", e)),
        };

        let compacted = compacted.trim().to_string();
        if compacted.is_empty() {
            return ToolResult::err("provider returned empty compacted text");
        }

        let before_lines = original.lines().count();
        let after_lines = compacted.lines().count();

        if dry_run {
            return ToolResult::ok(format!(
                "DRY RUN — {} would shrink from {} to {} lines.\n\n---DRAFT---\n{}",
                path.display(),
                before_lines,
                after_lines,
                compacted
            ));
        }

        // Safety: timestamped backup before overwrite.
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
        let bak = path.with_file_name(format!(
            "{}.bak_{}",
            path.file_name().and_then(|n| n.to_str()).unwrap_or("doctrine"),
            stamp
        ));
        if let Err(e) = tokio::fs::copy(&path, &bak).await {
            return ToolResult::err(format!("backup to {:?} failed: {}", bak, e));
        }
        if let Err(e) = tokio::fs::write(&path, &compacted).await {
            return ToolResult::err(format!("write {:?} failed: {}", path, e));
        }

        ToolResult::ok(format!(
            "compacted {} from {} → {} lines. backup saved at {}",
            path.display(),
            before_lines,
            after_lines,
            bak.display()
        ))
    }
}
