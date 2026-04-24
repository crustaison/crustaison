//! Roster Tool - Unified Missouri Sheriff jail roster scraper
//!
//! Calls ~/crustaison/scripts/sheriff_roster.py
//! Supports Miller, Camden, and Morgan counties.

use crate::tools::{Tool, ToolResult};
use std::time::Duration;
use tokio::process::Command;

pub struct RosterTool;

#[async_trait::async_trait]
impl Tool for RosterTool {
    fn name(&self) -> &str {
        "roster"
    }

    fn description(&self) -> &str {
        "Scrape Missouri county sheriff jail rosters and write to Google Sheets.         Supports Miller, Camden, and Morgan counties.         Use for ANY request involving jail rosters, inmate lists, bookings, or sheriff data.         Pass county=all to update all three counties at once."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "county": {
                    "type": "string",
                    "description": "County to scrape: miller, camden, morgan, or all (default: all)",
                    "enum": ["miller", "camden", "morgan", "all"]
                },
                "mode": {
                    "type": "string",
                    "description": "current, released, or all (default: all)",
                    "enum": ["current", "released", "all"]
                }
            },
            "required": []
        })
    }

    async fn call(&self, args: serde_json::Value) -> ToolResult {
        let county = args.get("county")
            .and_then(|v| v.as_str())
            .unwrap_or("all")
            .to_string();

        let mode = args.get("mode")
            .and_then(|v| v.as_str())
            .unwrap_or("all")
            .to_string();

        let script = "/home/sean/crustaison/scripts/sheriff_roster.py";

        let output = match tokio::time::timeout(
            Duration::from_secs(600),
            Command::new("python3")
                .arg(script)
                .arg(&county)
                .arg(&mode)
                .current_dir("/home/sean")
                .kill_on_drop(true)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .output()
        ).await {
            Ok(Ok(o)) => o,
            Ok(Err(e)) => return ToolResult::err(format!("Failed to run script: {}", e)),
            Err(_) => return ToolResult::err("Roster scrape timed out after 10 minutes"),
        };

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

        if output.status.success() {
            let result = if stderr.is_empty() {
                stdout
            } else {
                format!("{}\n[stderr]: {}", stdout, stderr)
            };
            ToolResult::ok(result)
        } else {
            ToolResult::err(format!(
                "Script failed (exit {:?}):\n{}\n{}",
                output.status.code(), stdout, stderr
            ))
        }
    }
}
