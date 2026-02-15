//! Tool Registry Builder
//!
//! Creates and configures the tool registry with all available tools.

use crate::tools::{ToolRegistry, ExecTool, FilesTool, WebTool, BrowserTool, ImageTool, ScheduleTool};
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
    // Note: ScheduleTool is added separately in main.rs with chat_id
    
    registry
}

/// Create a tool registry with custom configuration
pub async fn create_tool_registry_with_config(
    exec_config: Option<super::exec::ExecConfig>,
    files_config: Option<super::files::FilesConfig>,
    web_config: Option<super::web::WebConfig>,
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
    
    // Add browser and image tools (no config options yet)
    registry.register(BrowserTool::new()).await;
    registry.register(ImageTool::new()).await;
    
    registry
}
