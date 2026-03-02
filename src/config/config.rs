//! Configuration System
//!
//! Loads configuration from TOML file with environment variable overrides.
//! Configuration is split by layer: gateway, cognition, runtime, ledger.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::fs;
use anyhow::Result;
use dirs;

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub gateway: GatewayConfig,
    pub telegram: TelegramConfig,
    pub cognition: CognitionConfig,
    pub runtime: RuntimeConfig,
    pub ledger: LedgerConfig,
    #[serde(default)]
    pub email: Option<EmailConfig>,
    #[serde(default)]
    pub github: Option<GitHubConfig>,
    #[serde(default)]
    pub coordinator: CoordinatorConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            gateway: GatewayConfig::default(),
            telegram: TelegramConfig::default(),
            cognition: CognitionConfig::default(),
            runtime: RuntimeConfig::default(),
            ledger: LedgerConfig::default(),
            email: None,
            github: None,
            coordinator: CoordinatorConfig::default(),
        }
    }
}

/// GitHub configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubConfig {
    pub username: String,
    pub token: String,
}

/// Email configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailConfig {
    pub smtp_host: String,
    pub smtp_port: u16,
    pub imap_host: String,
    pub imap_port: u16,
    pub username: String,
    pub password: String,
    pub from_name: String,
}

impl Default for EmailConfig {
    fn default() -> Self {
        Self {
            smtp_host: "smtp.gmail.com".to_string(),
            smtp_port: 587,
            imap_host: "imap.gmail.com".to_string(),
            imap_port: 993,
            username: String::new(),
            password: String::new(),
            from_name: "Crusty".to_string(),
        }
    }
}

/// Gateway layer configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayConfig {
    pub port: u16,
    pub auth_enabled: bool,
    pub rate_limit_requests: u32,
    pub rate_limit_window_seconds: u64,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            port: 18790,
            auth_enabled: true,
            rate_limit_requests: 100,
            rate_limit_window_seconds: 60,
        }
    }
}

/// Telegram configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramConfig {
    pub bot_token: Option<String>,
    pub allowed_users: Vec<i64>,
}

impl Default for TelegramConfig {
    fn default() -> Self {
        Self {
            bot_token: None,
            allowed_users: vec![], // Empty means all users allowed
        }
    }
}

/// Cognition layer configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CognitionConfig {
    pub model: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub doctrine_path: PathBuf,
    pub memory_db_path: PathBuf,
    pub memory_engine_enabled: bool,
}

impl Default for CognitionConfig {
    fn default() -> Self {
        Self {
            model: "MiniMax-M2.1".to_string(),
            api_key: None,
            base_url: Some("https://api.minimax.io/v1".to_string()),
            doctrine_path: PathBuf::from("~/.config/crustaison/doctrine"),
            memory_db_path: PathBuf::from("~/.config/crustaison/memory.db"),
            memory_engine_enabled: true,
        }
    }
}

/// Runtime configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    pub memory_json_path: PathBuf,
    pub heartbeat_path: PathBuf,
    pub run_logs_path: PathBuf,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            memory_json_path: PathBuf::from("~/.config/crustaison/runtime/memory.json"),
            heartbeat_path: PathBuf::from("~/.config/crustaison/runtime/heartbeat.json"),
            run_logs_path: PathBuf::from("~/.config/crustaison/runtime/run_logs"),
        }
    }
}

/// Ledger configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerConfig {
    pub git_repo_path: PathBuf,
    pub auto_commit: bool,
}

impl Default for LedgerConfig {
    fn default() -> Self {
        Self {
            git_repo_path: PathBuf::from("~/.config/crustaison/ledger"),
            auto_commit: true,
        }
    }
}

/// Configuration result
pub type ConfigResult<T> = Result<T, ConfigError>;

/// Configuration error types
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]
    ReadError(#[from] std::io::Error),
    
    #[error("Failed to parse TOML: {0}")]
    ParseError(String),
    
    #[error("Failed to serialize TOML: {0}")]
    SerializeError(String),
    
    #[error("Missing required field: {0}")]
    MissingField(String),
    
    #[error("Invalid value for {field}: {message}")]
    ValidationError { field: String, message: String },
}

impl Config {
    /// Load configuration from file
    pub fn load(path: Option<PathBuf>) -> ConfigResult<Self> {
        let path = path.unwrap_or_else(|| {
            let mut path = dirs::config_dir()
                .unwrap_or_else(|| PathBuf::from("~/.config"));
            path.push("crustaison");
            path.push("config.toml");
            path
        });
        
        // Try to read config file, use defaults if not found
        let config_str = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                tracing::info!("Config file not found, using defaults: {}", path.display());
                return Ok(Config::default());
            }
            Err(e) => return Err(ConfigError::ReadError(e)),
        };
        
        // Parse TOML
        let mut config: Config = toml::from_str(&config_str)
            .map_err(|e| ConfigError::ParseError(e.to_string()))?;
        
        // Expand tildes in paths
        config.expand_tildes();
        
        // Validate
        config.validate()?;
        
        Ok(config)
    }
    
    /// Expand tilde (~) to home directory in all paths
    fn expand_tildes(&mut self) {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/home/sean"));
        
        self.cognition.doctrine_path = expand_tilde(&self.cognition.doctrine_path, &home);
        self.cognition.memory_db_path = expand_tilde(&self.cognition.memory_db_path, &home);
        self.runtime.memory_json_path = expand_tilde(&self.runtime.memory_json_path, &home);
        self.runtime.heartbeat_path = expand_tilde(&self.runtime.heartbeat_path, &home);
        self.runtime.run_logs_path = expand_tilde(&self.runtime.run_logs_path, &home);
        self.ledger.git_repo_path = expand_tilde(&self.ledger.git_repo_path, &home);
    }
    
    /// Validate configuration
    fn validate(&self) -> ConfigResult<()> {
        if self.gateway.port == 0 {
            return Err(ConfigError::ValidationError {
                field: "gateway.port".to_string(),
                message: "Port must be non-zero".to_string(),
            });
        }
        
        if self.cognition.model.is_empty() {
            return Err(ConfigError::ValidationError {
                field: "cognition.model".to_string(),
                message: "Model cannot be empty".to_string(),
            });
        }
        
        Ok(())
    }
    
    /// Save configuration to file
    pub fn save(&self, path: &PathBuf) -> ConfigResult<()> {
        let config_str = toml::to_string_pretty(self)
            .map_err(|e| ConfigError::SerializeError(e.to_string()))?;
        fs::write(path, config_str)
            .map_err(ConfigError::ReadError)?;
        Ok(())
    }
}

/// Expand tilde in path to home directory
fn expand_tilde(path: &PathBuf, home: &PathBuf) -> PathBuf {
    let path_str = path.to_string_lossy();
    if path_str.starts_with("~/") {
        let relative = &path_str[2..];
        let mut expanded = home.clone();
        expanded.push(relative);
        expanded
    } else {
        path.clone()
    }
}

/// Coordinator configuration for 5-model multi-agent stack
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinatorConfig {
    pub enabled: bool,
    pub router_model: String,
    pub local_model: String,
    pub vision_model: String,
    pub reason_model: String,
    pub embedding_model: String,
    pub nexa_url: String,
}

impl Default for CoordinatorConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            router_model: "unsloth/Qwen3-1.7B-GGUF:Q4_0".to_string(),
            local_model: "unsloth/Qwen3-1.7B-GGUF:Q4_0".to_string(),
            vision_model: "unsloth/Qwen3-VL-2B-Instruct-GGUF:Q4_0".to_string(),
            reason_model: "unsloth/Qwen3.5-35B-A3B-GGUF:Q4_K_M".to_string(),
            embedding_model: "Qwen/Qwen3-Embedding-0.6B-GGUF:F16".to_string(),
            nexa_url: "http://localhost:18181".to_string(),
        }
    }
}
