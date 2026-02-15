//! Nexa Provider
//!
//! Connects to local Nexa AI instance (localhost:11434).
//! Nexa is a lightweight LLM provider with OpenAI-compatible API.

use crate::providers::provider::{Provider, ProviderResult, ProviderError, ChatMessage, ProviderResponse, ModelInfo, UsageInfo};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Nexa configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NexaConfig {
    pub host: String,
    pub port: u16,
    pub model: String,
    pub timeout: u64,
}

impl Default for NexaConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 11434,
            model: "llama3".to_string(),
            timeout: 120,
        }
    }
}

/// Nexa provider implementation
pub struct NexaProvider {
    client: reqwest::Client,
    base_url: String,
    model: String,
    timeout: Duration,
}

impl NexaProvider {
    /// Create a new Nexa provider
    pub fn new(host: String, port: u16, model: String) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(120))
                .build()
                .unwrap_or_default(),
            base_url: format!("http://{}:{}", host, port),
            model,
            timeout: Duration::from_secs(120),
        }
    }
    
    /// Create with custom config
    pub fn with_config(config: NexaConfig) -> Self {
        Self::new(config.host, config.port, config.model)
    }
    
    /// Get available models from Nexa
    pub async fn list_models(&self) -> ProviderResult<Vec<String>> {
        // Nexa uses /v1/models endpoint (OpenAI-compatible)
        let url = format!("{}/v1/models", self.base_url);
        
        let response = self.client.get(&url)
            .send()
            .await
            .map_err(|e| ProviderError::ConnectionFailed(e.to_string()))?;
        
        if !response.status().is_success() {
            return Err(ProviderError::ApiError(format!("Status: {}", response.status())));
        }
        
        #[derive(Deserialize)]
        struct ModelsResponse {
            data: Vec<ModelData>,
        }
        
        #[derive(Deserialize)]
        struct ModelData {
            id: String,
        }
        
        let models: ModelsResponse = response
            .json()
            .await
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;
        
        Ok(models.data.into_iter().map(|m| m.id).collect())
    }
}

#[async_trait::async_trait]
impl Provider for NexaProvider {
    fn name(&self) -> &str {
        "nexa"
    }
    
    async fn model_info(&self) -> ProviderResult<ModelInfo> {
        Ok(ModelInfo {
            name: self.model.clone(),
            version: None,
            context_length: 8192,
        })
    }
    
    async fn chat(
        &self,
        messages: Vec<ChatMessage>,
        system_prompt: Option<String>,
    ) -> ProviderResult<ProviderResponse> {
        // Nexa uses OpenAI-compatible /v1/chat/completions endpoint
        let url = format!("{}/v1/chat/completions", self.base_url);
        
        // Build messages - prepend system if provided
        let mut all_messages = Vec::new();
        
        if let Some(system) = system_prompt {
            all_messages.push(NexaMessage {
                role: "system".to_string(),
                content: system,
            });
        }
        
        for msg in &messages {
            all_messages.push(NexaMessage {
                role: msg.role.clone(),
                content: msg.content.clone(),
            });
        }
        
        let request = NexaChatRequest {
            model: self.model.clone(),
            messages: all_messages,
            max_tokens: Some(2048),
            temperature: Some(0.7),
            stream: false,
        };
        
        let response = self.client.post(&url)
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| ProviderError::ConnectionFailed(e.to_string()))?;
        
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ProviderError::ApiError(format!("{}: {}", status, body)));
        }
        
        let nexa_resp: NexaChatResponse = response
            .json()
            .await
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;
        
        let choice = nexa_resp.choices
            .first()
            .ok_or_else(|| ProviderError::InvalidResponse("No choices in response".to_string()))?;
        
        let usage = nexa_resp.usage.map(|u| UsageInfo {
            prompt_tokens: u.prompt_tokens,
            completion_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
        });
        
        Ok(ProviderResponse {
            content: choice.message.content.clone(),
            usage,
        })
    }
    
    async fn is_available(&self) -> bool {
        let url = format!("{}/v1/models", self.base_url);
        
        if let Ok(response) = self.client.get(&url).send().await {
            return response.status().is_success();
        }
        
        false
    }
    
    fn default_model(&self) -> &str {
        &self.model
    }
}

// Internal Nexa types

#[derive(Serialize)]
struct NexaChatRequest {
    model: String,
    messages: Vec<NexaMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    stream: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NexaMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct NexaChatResponse {
    id: String,
    object: String,
    created: u64,
    model: String,
    choices: Vec<NexaChoice>,
    usage: Option<NexaUsage>,
}

#[derive(Deserialize)]
struct NexaChoice {
    index: u32,
    message: NexaMessage,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct NexaUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

