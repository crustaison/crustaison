//! Tool System for Crustaison
//!
//! Provides the LLM with tools to interact with the filesystem, execute commands,
//! search the web, and more. All tools route through the executor policy.

pub mod registry;
pub mod tool;
pub mod builder;

pub mod exec;
pub mod files;
pub mod web;
pub mod browser;
pub mod image;
pub mod schedule;
pub mod email;
pub mod github;
pub mod plugin;
pub mod http;
pub mod gdrive;
pub mod google;
pub mod memory;
pub mod model_switch;
pub mod roster;
pub mod lake;

pub use tool::{Tool, ToolResult};
pub use registry::ToolRegistry;
pub use builder::create_tool_registry;

// Re-export tool implementations
pub use exec::{ExecTool, ExecConfig};
pub use files::{FilesTool, FilesConfig};
pub use web::{WebTool, WebConfig};
pub use browser::BrowserTool;
pub use image::ImageTool;
pub use schedule::ScheduleTool;
pub use email::{EmailTool, EmailConfig};
pub use github::{GitHubTool, GitHubConfig};
pub use plugin::{ScriptTool, PluginManifest, load_plugins};
pub use http::{HttpTool, HttpConfig, HttpRequest, HttpResult};
pub use gdrive::GoogleDriveTool;
pub use google::GoogleTool;
pub use memory::MemoryTool;
pub use model_switch::ModelSwitchTool;
