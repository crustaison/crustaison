//! Plugin System - Dynamic loading and management
//!
//! Supports loading Rust plugins at runtime using dynamic libraries.

use serde::{Deserialize, Serialize};
use std::path::{PathBuf, Path};
use std::fs;
use std::collections::HashMap;

/// Plugin metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: Option<String>,
    pub dependencies: Vec<String>,
}

/// Plugin state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginState {
    pub metadata: PluginMetadata,
    pub enabled: bool,
    pub loaded_at: i64,
}

/// Plugin manifest (plugin.json)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: Option<String>,
    pub main: String,
    pub permissions: Vec<String>,
}

/// Plugin manager
pub struct PluginManager {
    plugins_dir: PathBuf,
    plugins: HashMap<String, PluginState>,
}

impl PluginManager {
    /// Create new plugin manager
    pub fn new(plugins_dir: PathBuf) -> Self {
        if !plugins_dir.exists() {
            let _ = fs::create_dir_all(&plugins_dir);
        }
        
        let mut manager = Self {
            plugins_dir,
            plugins: HashMap::new(),
        };
        
        // Load existing plugins
        manager.discover_plugins();
        
        manager
    }
    
    /// Discover and load plugins from plugins directory
    fn discover_plugins(&mut self) {
        if let Ok(entries) = fs::read_dir(&self.plugins_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let _ = self.load_plugin(&path);
                }
            }
        }
    }
    
    /// Load a plugin from directory
    pub fn load_plugin(&mut self, path: &Path) -> Result<PluginState, String> {
        let manifest_path = path.join("plugin.json");
        
        if !manifest_path.exists() {
            return Err("No plugin.json found".to_string());
        }
        
        let manifest: PluginManifest = {
            let content = fs::read_to_string(&manifest_path)
                .map_err(|e| e.to_string())?;
            serde_json::from_str(&content).map_err(|e| e.to_string())?
        };
        
        let state = PluginState {
            metadata: PluginMetadata {
                name: manifest.name,
                version: manifest.version,
                description: manifest.description,
                author: manifest.author,
                dependencies: manifest.permissions,
            },
            enabled: false, // Disabled by default
            loaded_at: chrono::Utc::now().timestamp_millis(),
        };
        
        self.plugins.insert(state.metadata.name.clone(), state.clone());
        Ok(state)
    }
    
    /// Enable a plugin
    pub fn enable(&mut self, name: &str) -> Result<(), String> {
        if let Some(state) = self.plugins.get_mut(name) {
            state.enabled = true;
            self.save_state(name);
            Ok(())
        } else {
            Err(format!("Plugin '{}' not found", name))
        }
    }
    
    /// Disable a plugin
    pub fn disable(&mut self, name: &str) -> Result<(), String> {
        if let Some(state) = self.plugins.get_mut(name) {
            state.enabled = false;
            self.save_state(name);
            Ok(())
        } else {
            Err(format!("Plugin '{}' not found", name))
        }
    }
    
    /// List all plugins
    pub fn list(&self) -> Vec<&PluginState> {
        self.plugins.values().collect()
    }
    
    /// Get enabled plugins
    pub fn enabled(&self) -> Vec<&PluginState> {
        self.plugins.values()
            .filter(|s| s.enabled)
            .collect()
    }
    
    /// Get plugin state
    pub fn get(&self, name: &str) -> Option<&PluginState> {
        self.plugins.get(name)
    }
    
    /// Unload a plugin
    pub fn unload(&mut self, name: &str) -> Result<(), String> {
        if self.plugins.remove(name).is_some() {
            // Remove state file
            let state_path = self.plugins_dir.join(format!("{}.state.json", name));
            if state_path.exists() {
                let _ = fs::remove_file(&state_path);
            }
            Ok(())
        } else {
            Err(format!("Plugin '{}' not found", name))
        }
    }
    
    /// Save plugin state
    fn save_state(&self, name: &str) {
        if let Some(state) = self.plugins.get(name) {
            let state_path = self.plugins_dir.join(format!("{}.state.json", name));
            if let Ok(content) = serde_json::to_string(state) {
                let _ = fs::write(&state_path, content);
            }
        }
    }
}

/// Plugin trait for implementing plugins
#[async_trait::async_trait]
pub trait Plugin: Send + Sync {
    /// Plugin metadata
    fn metadata(&self) -> &PluginMetadata;
    
    /// Initialize the plugin
    async fn initialize(&mut self) -> Result<(), String>;
    
    /// Shutdown the plugin
    async fn shutdown(&mut self) -> Result<(), String>;
    
    /// Handle a message
    async fn handle_message(&self, message: &str) -> Result<String, String>;
    
    /// Get command handlers
    fn commands(&self) -> Vec<&str> {
        vec![]
    }
}

/// Simple plugin context
pub struct PluginContext {
    pub agent: Option<()>, // Placeholder for agent reference
}

impl PluginContext {
    pub fn new() -> Self {
        Self { agent: None }
    }
}
