// Configuration Module
//
// Loads and validates configuration from TOML file.
// Supports environment variable overrides.

pub mod config;

pub use config::{Config, ConfigResult, EmailConfig, GitHubConfig};
