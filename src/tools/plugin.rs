//! Plugin System - Script-based runtime tool extensions
//!
//! Allows Crusty to extend itself at runtime by writing script plugins.
//! Each plugin is a directory containing a manifest.json and a script file.
//!
//! Plugin structure:
//!   ~/.config/crustaison/plugins/
//!   ├── my_tool/
//!   │   ├── manifest.json
//!   │   └── plugin.py (or plugin.sh, plugin.js, etc.)
//!
//! Manifest format:
//! {
//!   "name": "my_tool",
//!   "description": "What this tool does",
//!   "script": "plugin.py",
//!   "interpreter": "python3",
//!   "parameters": { ... JSON Schema ... }
//! }
//!
//! Scripts receive JSON args on stdin and must print JSON result to stdout:
//! { "success": true, "output": "result text" }
//! or
//! { "success": false, "error": "error message" }

use crate::tools::tool::{Tool, ToolResult};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Plugin manifest describing a script-based tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub description: String,
    pub script: String,
    #[serde(default = "default_interpreter")]
    pub interpreter: String,
    #[serde(default = "default_parameters")]
    pub parameters: serde_json::Value,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub author: String,
}

fn default_interpreter() -> String {
    "python3".to_string()
}

fn default_parameters() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "input": {
                "type": "string",
                "description": "Input for the plugin"
            }
        }
    })
}

/// A tool backed by an external script
pub struct ScriptTool {
    manifest: PluginManifest,
    plugin_dir: PathBuf,
}

impl ScriptTool {
    pub fn new(manifest: PluginManifest, plugin_dir: PathBuf) -> Self {
        Self { manifest, plugin_dir }
    }

    fn script_path(&self) -> PathBuf {
        self.plugin_dir.join(&self.manifest.script)
    }
}

#[async_trait]
impl Tool for ScriptTool {
    fn name(&self) -> &str {
        &self.manifest.name
    }

    fn description(&self) -> &str {
        &self.manifest.description
    }

    fn parameters(&self) -> serde_json::Value {
        self.manifest.parameters.clone()
    }

    async fn call(&self, args: serde_json::Value) -> ToolResult {
        let script_path = self.script_path();

        if !script_path.exists() {
            return ToolResult::err(format!("Plugin script not found: {}", script_path.display()));
        }

        // Serialize args to JSON for stdin
        let args_json = serde_json::to_string(&args).unwrap_or_else(|_| "{}".to_string());

        // Run the script with interpreter
        let output = tokio::process::Command::new(&self.manifest.interpreter)
            .arg(&script_path)
            .current_dir(&self.plugin_dir)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn();

        let mut child = match output {
            Ok(c) => c,
            Err(e) => return ToolResult::err(format!("Failed to start plugin: {}", e)),
        };

        // Write args to stdin
        if let Some(mut stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            let _ = stdin.write_all(args_json.as_bytes()).await;
            drop(stdin);
        }

        // Wait for completion with timeout
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            child.wait_with_output(),
        ).await;

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();

                if !output.status.success() {
                    return ToolResult::err(format!(
                        "Plugin exited with code {}: {}",
                        output.status.code().unwrap_or(-1),
                        if stderr.is_empty() { &stdout } else { &stderr }
                    ));
                }

                // Try to parse JSON output from the script
                if let Ok(result) = serde_json::from_str::<serde_json::Value>(&stdout) {
                    let success = result.get("success").and_then(|v| v.as_bool()).unwrap_or(true);
                    if success {
                        let output_text = result.get("output")
                            .and_then(|v| v.as_str())
                            .unwrap_or(&stdout);
                        ToolResult::ok(output_text.to_string())
                    } else {
                        let error = result.get("error")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Plugin returned failure");
                        ToolResult::err(error.to_string())
                    }
                } else {
                    // Script didn't return JSON — treat raw stdout as output
                    if stdout.trim().is_empty() {
                        ToolResult::ok("[Plugin completed with no output]".to_string())
                    } else {
                        ToolResult::ok(stdout.trim().to_string())
                    }
                }
            }
            Ok(Err(e)) => ToolResult::err(format!("Plugin execution failed: {}", e)),
            Err(_) => ToolResult::err("Plugin timed out after 30 seconds"),
        }
    }
}

/// Load all plugins from the plugins directory
pub fn load_plugins(plugins_dir: &Path) -> Vec<ScriptTool> {
    let mut plugins = Vec::new();

    if !plugins_dir.exists() {
        // Create the plugins directory
        let _ = std::fs::create_dir_all(plugins_dir);
        tracing::info!("Created plugins directory: {}", plugins_dir.display());
        return plugins;
    }

    let entries = match std::fs::read_dir(plugins_dir) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!("Failed to read plugins directory: {}", e);
            return plugins;
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let manifest_path = path.join("manifest.json");
        if !manifest_path.exists() {
            continue;
        }

        match std::fs::read_to_string(&manifest_path) {
            Ok(content) => {
                match serde_json::from_str::<PluginManifest>(&content) {
                    Ok(manifest) => {
                        let script_path = path.join(&manifest.script);
                        if script_path.exists() {
                            tracing::info!("Loaded plugin: {} ({})", manifest.name, manifest.description);
                            plugins.push(ScriptTool::new(manifest, path.clone()));
                        } else {
                            tracing::warn!(
                                "Plugin '{}' script not found: {}",
                                manifest.name,
                                script_path.display()
                            );
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to parse plugin manifest {}: {}",
                            manifest_path.display(),
                            e
                        );
                    }
                }
            }
            Err(e) => {
                tracing::warn!("Failed to read manifest {}: {}", manifest_path.display(), e);
            }
        }
    }

    tracing::info!("Loaded {} plugin(s)", plugins.len());
    plugins
}
