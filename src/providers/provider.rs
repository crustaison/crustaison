//! LLM Provider Trait
//!
//! Defines the interface for all LLM providers.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt;

/// A chat message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// Provider response
#[derive(Debug, Clone)]
pub struct ProviderResponse {
    pub content: String,
    pub usage: Option<UsageInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageInfo {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Model information
#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub name: String,
    pub version: Option<String>,
    pub context_length: usize,
}

/// Provider errors
#[derive(Debug, Clone, thiserror::Error)]
pub enum ProviderError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    
    #[error("API error: {0}")]
    ApiError(String),
    
    #[error("Model not found: {0}")]
    ModelNotFound(String),
    
    #[error("Request timeout")]
    Timeout,
    
    #[error("Invalid response: {0}")]
    InvalidResponse(String),
    
    #[error("Not implemented: {0}")]
    NotImplemented(String),
}

/// Result wrapper for provider operations
pub type ProviderResult<T> = Result<T, ProviderError>;

/// Trait for all LLM providers
#[async_trait]
pub trait Provider: Send + Sync {
    /// Get provider name
    fn name(&self) -> &str;
    
    /// Get model information
    async fn model_info(&self) -> ProviderResult<ModelInfo>;
    
    /// Send chat messages and get a response
    async fn chat(
        &self,
        messages: Vec<ChatMessage>,
        system_prompt: Option<String>,
    ) -> ProviderResult<ProviderResponse>;
    
    /// Check if provider is available
    async fn is_available(&self) -> bool;
    
    /// Get the default model name
    fn default_model(&self) -> &str;
}
