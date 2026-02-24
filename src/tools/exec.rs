//! Exec Tool - Shell Command Execution
//!
//! Executes shell commands with safety checks through the authority layer.

use crate::tools::{Tool, ToolResult};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;
use tokio::process::Command;

/// Safety configuration for command execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecConfig {
    /// Allowed working directories
    pub allowed_dirs: Vec<String>,
    /// Blocked commands
    pub blocked_commands: Vec<String>,
    /// Maximum output size (bytes)
    pub max_output: usize,
    /// Timeout for commands (seconds)
    pub timeout: u64,
    /// Shell to use (None = no shell, direct exec)
    pub shell: Option<String>,
}

impl Default for ExecConfig {
    fn default() -> Self {
        Self {
            allowed_dirs: vec!["/home/sean".to_string()],
            blocked_commands: vec![
                "rm -rf /".to_string(),
                "rm -rf /*".to_string(),
                "rm -r /".to_string(),
                "dd if=/dev/zero".to_string(),
                "mkfs".to_string(),
                ":(){ :|:& };:".to_string(),
                "DROP TABLE".to_string(),
                "DROP DATABASE".to_string(),
                "shutdown".to_string(),
                "reboot".to_string(),
                "> /dev/sd".to_string(),
            ],
            max_output: 1024 * 1024, // 1MB
            timeout: 60,
            shell: Some("/bin/bash".to_string()),
        }
    }
}

/// Exec tool - executes shell commands
pub struct ExecTool {
    config: ExecConfig,
}

impl ExecTool {
    pub fn new() -> Self {
        Self {
            config: ExecConfig::default(),
        }
    }
    
    /// Create with custom config
    pub fn with_config(config: ExecConfig) -> Self {
        Self { config }
    }
    
    /// Check if a command is safe to execute
    fn check_safety(&self, cmd: &str, cwd: Option<&str>) -> Result<(), String> {
        // Check for blocked commands
        let lower_cmd = cmd.to_lowercase();
        for blocked in &self.config.blocked_commands {
            if lower_cmd.contains(&blocked.to_lowercase()) {
                return Err(format!("Blocked command: {}", blocked));
            }
        }
        
        // Check working directory
        if let Some(dir) = cwd {
            let dir_path = PathBuf::from(dir);
            let mut allowed = false;
            for allowed_dir in &self.config.allowed_dirs {
                let allowed_path = PathBuf::from(allowed_dir);
                if dir_path.starts_with(&allowed_path) || dir_path == allowed_path {
                    allowed = true;
                    break;
                }
            }
            if !allowed {
                return Err(format!("Directory not allowed: {}", dir));
            }
        }
        
        Ok(())
    }
}

impl Default for ExecTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Tool for ExecTool {
    fn name(&self) -> &str {
        "exec"
    }
    
    fn description(&self) -> &str {
        "Execute a shell command and return the output. Use for system operations, running scripts, etc."
    }
    
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The command to execute"
                },
                "timeout": {
                    "type": "integer",
                    "description": "Timeout in seconds (default: 60)",
                    "minimum": 1,
                    "maximum": 300
                },
                "working_dir": {
                    "type": "string", 
                    "description": "Working directory (default: /home/sean)"
                }
            },
            "required": ["command"]
        })
    }
    
    async fn call(&self, args: serde_json::Value) -> ToolResult {
        let command = match args.get("command").and_then(|v| v.as_str()) {
            Some(c) => c.to_string(),
            None => return ToolResult::err("Missing 'command' parameter"),
        };
        
        let timeout: u64 = args.get("timeout")
            .and_then(|v| v.as_u64())
            .unwrap_or(self.config.timeout);
            
        let working_dir = args.get("working_dir")
            .and_then(|v| v.as_str())
            .unwrap_or("/home/sean");
        
        // Safety check
        if let Err(e) = self.check_safety(&command, Some(working_dir)) {
            return ToolResult::err(e);
        }
        
        // Build command
        let mut cmd = if let Some(shell) = &self.config.shell {
            let mut c = Command::new(&shell);
            c.arg("-c").arg(&command);
            c
        } else {
            // Split command for direct execution
            let parts = match shellwords::split(&command) {
                Ok(p) => p,
                Err(e) => return ToolResult::err(format!("Failed to parse command: {}", e)),
            };
            if parts.is_empty() {
                return ToolResult::err("Empty command");
            }
            let mut c = Command::new(&parts[0]);
            for arg in &parts[1..] {
                c.arg(arg);
            }
            c
        };
        
        cmd.current_dir(working_dir)
            .kill_on_drop(true)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        
        // Execute with timeout
        let output = match tokio::time::timeout(
            Duration::from_secs(timeout),
            cmd.output()
        ).await {
            Ok(Ok(output)) => output,
            Ok(Err(e)) => return ToolResult::err(format!("Command failed: {}", e)),
            Err(_) => return ToolResult::err(format!("Command timed out after {}s", timeout)),
        };
        
        // Truncate output if too large
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        
        let truncated = stdout.len() > self.config.max_output || stderr.len() > self.config.max_output;
        let stdout = if stdout.len() > self.config.max_output {
            format!("{}... (truncated {} bytes)", 
                &stdout[..self.config.max_output], 
                stdout.len() - self.config.max_output)
        } else {
            stdout
        };
        
        let metadata = serde_json::json!({
            "exit_code": output.status.code(),
            "success": output.status.success(),
            "truncated": truncated,
            "working_dir": working_dir,
            "timeout": timeout,
        });
        
        let mut result = if output.status.success() {
            ToolResult::ok(stdout)
        } else {
            ToolResult::err(format!("Exit code: {:?}", output.status.code()))
        };
        
        result.metadata = Some(metadata);
        
        // Add stderr to output if present
        if !stderr.is_empty() {
            result.output.push_str(&format!("\n[stderr]:\n{}", stderr));
        }
        
        result
    }
}
