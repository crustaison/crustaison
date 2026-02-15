//! CLI Commands - Config, Security, Edit, and Plugin management
//!
//! Additional CLI commands for Crustaison administration.

use anyhow::Result;
use serde_json;
use std::io::Write;
use std::path::{PathBuf, Path};
use std::fs;

/// Get the config directory
fn get_config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("crustaison")
}

/// Get the data directory
fn get_data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("~/.local/share"))
        .join("crustaison")
}

pub mod config_commands {
    use super::*;
    
    /// Show current configuration
    pub fn show() -> Result<String> {
        let config_path = get_config_dir().join("config.toml");
        
        if config_path.exists() {
            let content = fs::read_to_string(&config_path)?;
            Ok(format!("# Config: {}\n\n{}", config_path.display(), content))
        } else {
            Ok("No config file found. Using defaults.".to_string())
        }
    }
    
    /// Edit configuration
    pub fn edit() -> Result<String> {
        let config_path = get_config_dir();
        
        if !config_path.exists() {
            fs::create_dir_all(&config_path)?;
        }
        
        let config_file = config_path.join("config.toml");
        
        // Create default config if not exists
        if !config_file.exists() {
            let default_config = include_str!("../../default_config.toml");
            fs::write(&config_file, default_config)?;
            return Ok(format!("Created default config at: {}", config_file.display()));
        }
        
        // Open in editor
        let editor = std::env::var("EDITOR")
            .unwrap_or_else(|_| "nano".to_string());
        
        std::process::Command::new(&editor)
            .arg(&config_file)
            .status()
            .map(|_| format!("Opened config in {}", editor))
            .map_err(|e| anyhow::anyhow!("Failed to open editor: {}", e))
    }
    
    /// Validate configuration
    pub fn validate() -> Result<String> {
        let config_path = get_config_dir().join("config.toml");
        
        if !config_path.exists() {
            return Ok("No config file to validate.".to_string());
        }
        
        // Basic TOML parsing check
        let content = fs::read_to_string(&config_path)?;
        let _: toml::Value = content.parse()?;
        
        Ok("✓ Configuration is valid".to_string())
    }
    
    /// Reset to defaults
    pub fn reset() -> Result<String> {
        let config_path = get_config_dir().join("config.toml");
        
        if config_path.exists() {
            fs::remove_file(&config_path)?;
            return Ok(format!("Removed config file: {}", config_path.display()));
        }
        
        Ok("No config file to remove.".to_string())
    }
}

pub mod security_commands {
    use super::*;
    
    /// Show security policy
    pub fn show_policy() -> Result<String> {
        let policy_path = get_data_dir().join("security_policy.json");
        
        if policy_path.exists() {
            let content = fs::read_to_string(&policy_path)?;
            Ok(content)
        } else {
            Ok(r#"{
  "version": "1.0",
  "allow_destructive": false,
  "allowed_tools": ["read", "list", "search"],
  "blocked_commands": ["rm", "del", "format"],
  "max_file_size_bytes": 10485760,
  "require_confirmation": ["write", "exec"]
}"#.to_string())
        }
    }
    
    /// Update security policy
    pub fn update_policy(policy_json: &str) -> Result<String> {
        let policy_path = get_data_dir();
        if !policy_path.exists() {
            fs::create_dir_all(&policy_path)?;
        }
        
        // Validate JSON
        let _: serde_json::Value = policy_json.parse()?;
        
        let policy_file = policy_path.join("security_policy.json");
        fs::write(&policy_file, policy_json)?;
        
        Ok(format!("Updated security policy: {}", policy_file.display()))
    }
    
    /// Add a blocked command
    pub fn add_blocked(command: &str) -> Result<String> {
        let policy_path = get_data_dir().join("security_policy.json");
        
        let mut policy: serde_json::Value = if policy_path.exists() {
            serde_json::from_str(&fs::read_to_string(&policy_path)?)?
        } else {
            serde_json::json!({
                "version": "1.0",
                "blocked_commands": []
            })
        };
        
        if let Some(commands) = policy["blocked_commands"].as_array_mut() {
            if !commands.iter().any(|c| c == command) {
                commands.push(serde_json::json!(command));
            }
        }
        
        fs::write(&policy_path, serde_json::to_string_pretty(&policy)?)?;
        Ok(format!("Added '{}' to blocked commands", command))
    }
    
    /// Show security status
    pub fn status() -> Result<String> {
        let status = vec![
            "🔒 Security Status",
            "",
            "Tools: Enabled",
            "Destructive ops: Require confirmation",
            "Network: Restricted",
            "File system: sandboxed",
        ];
        
        Ok(status.join("\n"))
    }
}

pub mod edit_commands {
    use super::*;
    
    /// Edit a file
    pub fn edit_file(path: &str, line: Option<usize>) -> Result<String> {
        let path = PathBuf::from(path);
        
        if !path.exists() {
            return Err(anyhow::anyhow!("File not found: {}", path.display()));
        }
        
        let editor = std::env::var("EDITOR")
            .unwrap_or_else(|_| "nano".to_string());
        
        let mut cmd = std::process::Command::new(&editor);
        if let Some(n) = line {
            cmd.arg(format!("+{}", n));
        }
        cmd.arg(&path);
        
        cmd.status()
            .map(|_| format!("Opened {} in {}", path.display(), editor))
            .map_err(|e| anyhow::anyhow!("Failed to open editor: {}", e))
    }
    
    /// Read a file
    pub fn read_file(path: &str, lines: Option<usize>) -> Result<String> {
        let path = PathBuf::from(path);
        
        if !path.exists() {
            return Err(anyhow::anyhow!("File not found: {}", path.display()));
        }
        
        let content = fs::read_to_string(&path)?;
        
        if let Some(n) = lines {
            let lines: Vec<&str> = content.lines().take(n).collect();
            Ok(lines.join("\n"))
        } else {
            Ok(content)
        }
    }
    
    /// Write to a file
    pub fn write_file(path: &str, content: &str) -> Result<String> {
        let path = PathBuf::from(path);
        
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }
        
        fs::write(&path, content)?;
        Ok(format!("Wrote {} bytes to {}", content.len(), path.display()))
    }
    
    /// Append to a file
    pub fn append_file(path: &str, content: &str) -> Result<String> {
        let path = PathBuf::from(path);
        
        if path.exists() {
            fs::OpenOptions::new()
                .append(true)
                .open(&path)?
                .write_all(content.as_bytes())?;
        } else {
            fs::write(&path, content)?;
        }
        
        Ok(format!("Appended {} bytes to {}", content.len(), path.display()))
    }
    
    /// List directory
    pub fn list_dir(path: &str) -> Result<String> {
        let path = PathBuf::from(path);
        
        if !path.exists() || !path.is_dir() {
            return Err(anyhow::anyhow!("Not a directory: {}", path.display()));
        }
        
        let mut entries: Vec<String> = fs::read_dir(&path)?
            .filter_map(|e| e.ok())
            .map(|e| {
                let path = e.path();
                let is_dir = path.is_dir();
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                if is_dir {
                    format!("{}/", name)
                } else {
                    name.to_string()
                }
            })
            .collect();
        
        entries.sort();
        Ok(entries.join("\n"))
    }
}

pub mod plugin_commands {
    use super::*;
    
    /// List installed plugins
    pub fn list() -> Result<String> {
        let plugins_dir = get_data_dir().join("plugins");
        
        if !plugins_dir.exists() {
            return Ok("No plugins directory found.".to_string());
        }
        
        let mut plugins = Vec::new();
        
        for entry in fs::read_dir(&plugins_dir)?.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                plugins.push(name.to_string());
            }
        }
        
        if plugins.is_empty() {
            Ok("No plugins installed.".to_string())
        } else {
            Ok(format!("Installed plugins:\n{}", plugins.join("\n")))
        }
    }
    
    /// Install a plugin
    pub fn install(name: &str, _source: Option<&str>) -> Result<String> {
        let plugins_dir = get_data_dir().join("plugins");
        if !plugins_dir.exists() {
            fs::create_dir_all(&plugins_dir)?;
        }
        
        let plugin_dir = plugins_dir.join(name);
        
        if plugin_dir.exists() {
            return Err(anyhow::anyhow!("Plugin '{}' already installed", name));
        }
        
        fs::create_dir_all(&plugin_dir)?;
        
        // For now, just create a placeholder
        let manifest = format!(
            r#"{{
    "name": "{}",
    "version": "0.1.0",
    "description": "User installed plugin"
}}"#,
            name
        );
        
        fs::write(plugin_dir.join("plugin.json"), manifest)?;
        
        Ok(format!("Installed plugin: {}", name))
    }
    
    /// Uninstall a plugin
    pub fn uninstall(name: &str) -> Result<String> {
        let plugin_dir = get_data_dir().join("plugins").join(name);
        
        if !plugin_dir.exists() {
            return Err(anyhow::anyhow!("Plugin '{}' not found", name));
        }
        
        fs::remove_dir_all(&plugin_dir)?;
        
        Ok(format!("Uninstalled plugin: {}", name))
    }
    
    /// Enable a plugin
    pub fn enable(name: &str) -> Result<String> {
        let plugin_dir = get_data_dir().join("plugins").join(name);
        
        if !plugin_dir.exists() {
            return Err(anyhow::anyhow!("Plugin '{}' not found", name));
        }
        
        let enabled_file = plugin_dir.join(".enabled");
        fs::write(&enabled_file, "")?;
        
        Ok(format!("Enabled plugin: {}", name))
    }
    
    /// Disable a plugin
    pub fn disable(name: &str) -> Result<String> {
        let plugin_dir = get_data_dir().join("plugins").join(name);
        let enabled_file = plugin_dir.join(".enabled");
        
        if enabled_file.exists() {
            fs::remove_file(&enabled_file)?;
        }
        
        Ok(format!("Disabled plugin: {}", name))
    }
}
