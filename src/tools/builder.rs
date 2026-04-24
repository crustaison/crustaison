//! Tool Registry Builder
//!
//! Creates and configures the tool registry with all available tools.

use crate::tools::{ToolRegistry, ExecTool, FilesTool, WebTool, BrowserTool, ImageTool, ScheduleTool, HttpTool, GoogleDriveTool, GoogleTool};
use crate::tools::roster::RosterTool;
use crate::tools::lake::LakeTool;
use std::sync::Arc;

/// Create the default tool registry with all standard tools
pub async fn create_tool_registry() -> ToolRegistry {
    let registry = ToolRegistry::new();
    
    // Register all tools
    registry.register(ExecTool::new()).await;
    registry.register(FilesTool::new()).await;
    registry.register(WebTool::new()).await;
    registry.register(BrowserTool::new()).await;
    registry.register(ImageTool::new()).await;
    registry.register(HttpTool::new()).await;
    registry.register(GoogleDriveTool::new("gdrive-crusty")).await;
    registry.register(GoogleTool::new()).await;
    registry.register(RosterTool).await;
    registry.register(LakeTool).await;
    // Note: ScheduleTool is added separately in main.rs with chat_id
    
    registry
}

/// Create a tool registry with custom configuration
pub async fn create_tool_registry_with_config(
    exec_config: Option<super::exec::ExecConfig>,
    files_config: Option<super::files::FilesConfig>,
    web_config: Option<super::web::WebConfig>,
    http_config: Option<super::http::HttpConfig>,
) -> ToolRegistry {
    let registry = ToolRegistry::new();
    
    // Register tools with custom configs
    if let Some(config) = exec_config {
        registry.register(ExecTool::with_config(config)).await;
    } else {
        registry.register(ExecTool::new()).await;
    }
    
    if let Some(config) = files_config {
        registry.register(FilesTool::with_config(config)).await;
    } else {
        registry.register(FilesTool::new()).await;
    }
    
    if let Some(config) = web_config {
        registry.register(WebTool::with_config(config)).await;
    } else {
        registry.register(WebTool::new()).await;
    }
    
    // Add HTTP tool
    if let Some(config) = http_config {
        registry.register(HttpTool::with_config(config)).await;
    } else {
        registry.register(HttpTool::new()).await;
    }
    
    // Add browser and image tools (no config options yet)
    registry.register(BrowserTool::new()).await;
    registry.register(ImageTool::new()).await;
    
    registry
}
