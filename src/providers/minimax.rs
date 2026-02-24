//! MiniMax LLM Provider
//!
//! Uses the OpenAI-compatible chat completions API at api.minimax.io/v1

use crate::providers::provider::{Provider, ProviderResult, ProviderError, ChatMessage, ProviderResponse, ModelInfo, UsageInfo};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// MiniMax provider configuration
#[derive(Debug, Clone)]
pub struct MiniMaxConfig {
    pub api_key: String,
    pub model: String,
    pub base_url: String,
}

impl Default for MiniMaxConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model: "MiniMax-M2.1".to_string(),
            base_url: "https://api.minimax.io/v1".to_string(),
        }
    }
}

/// MiniMax LLM provider
pub struct MiniMaxProvider {
    client: Client,
    api_key: String,
    model: String,
    base_url: String,
}

impl MiniMaxProvider {
    pub fn new(api_key: String, model: String, base_url: Option<String>) -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(120))
                .build()
                .unwrap_or_default(),
            api_key,
            model,
            base_url: base_url.unwrap_or_else(|| "https://api.minimax.io/v1".to_string()),
        }
    }
    
    pub fn with_config(config: MiniMaxConfig) -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(120))
                .build()
                .unwrap_or_default(),
            api_key: config.api_key,
            model: config.model,
            base_url: config.base_url,
        }
    }
}

#[async_trait::async_trait]
impl Provider for MiniMaxProvider {
    fn name(&self) -> &str {
        "minimax"
    }
    
    async fn model_info(&self) -> ProviderResult<ModelInfo> {
        Ok(ModelInfo {
            name: self.model.clone(),
            version: None,
            context_length: 200000, // MiniMax M2.1 has 200k context
        })
    }
    
    async fn chat(
        &self,
        messages: Vec<ChatMessage>,
        system_prompt: Option<String>,
    ) -> ProviderResult<ProviderResponse> {
        // Build messages array - prepend system message if provided
        let mut all_messages = Vec::new();
        if let Some(system) = &system_prompt {
            tracing::info!("=== SYSTEM PROMPT ({} chars) ===", system.len());
            tracing::info!("{}", if system.len() > 500 { &system[..500] } else { &system[..] });
            tracing::info!("=== END SYSTEM PROMPT ===");
            all_messages.push(ApiMessage {
                role: "system".to_string(),
                content: ApiContent::Text(system.clone()),
            });
        }
        for msg in &messages {
            all_messages.push(to_api_message(msg));
        }

        let request = ApiRequest {
            model: self.model.clone(),
            max_tokens: Some(8192),
            messages: all_messages,
            stream: false,
        };

        let resp = self.client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| ProviderError::ConnectionFailed(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ProviderError::ApiError(format!("{}: {}", status, body)));
        }

        let response: ApiResponse = resp
            .json()
            .await
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

        let choice = response.choices.first()
            .ok_or_else(|| ProviderError::InvalidResponse("No choices in response".to_string()))?;

        let usage = response.usage.map(|u| UsageInfo {
            prompt_tokens: u.prompt_tokens,
            completion_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
        });

        Ok(ProviderResponse {
            content: match &choice.message.content { ApiContent::Text(t) => t.clone(), ApiContent::Parts(parts) => parts.iter().filter_map(|p| if let ContentPart::Text { text } = p { Some(text.as_str()) } else { None }).collect::<Vec<_>>().join(" ") },
            usage,
        })
    }
    
    async fn is_available(&self) -> bool {
        // MiniMax is always "available" if we can reach the API
        // A real check would try a simple request
        true
    }
    
    fn default_model(&self) -> &str {
        &self.model
    }
}

// Internal API types

#[derive(Serialize)]
struct ApiRequest {
    model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    messages: Vec<ApiMessage>,
    stream: bool,
}

/// Content can be a plain string (text-only) or an array of parts (vision)
#[derive(Serialize, Deserialize, Clone)]
#[serde(untagged)]
enum ApiContent {
    Text(String),
    Parts(Vec<ContentPart>),
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
enum ContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: ImageUrlValue },
}

#[derive(Serialize, Deserialize, Clone)]
struct ImageUrlValue {
    url: String,
}

#[derive(Serialize, Deserialize, Clone)]
struct ApiMessage {
    role: String,
    content: ApiContent,
}

fn to_api_message(msg: &crate::providers::provider::ChatMessage) -> ApiMessage {
    if msg.images.is_empty() {
        ApiMessage {
            role: msg.role.clone(),
            content: ApiContent::Text(msg.content.clone()),
        }
    } else {
        let mut parts: Vec<ContentPart> = msg.images.iter().map(|url| {
            ContentPart::ImageUrl {
                image_url: ImageUrlValue { url: url.clone() },
            }
        }).collect();
        if !msg.content.is_empty() {
            parts.push(ContentPart::Text { text: msg.content.clone() });
        }
        ApiMessage {
            role: msg.role.clone(),
            content: ApiContent::Parts(parts),
        }
    }
}

#[derive(Deserialize)]
struct ApiResponse {
    id: String,
    choices: Vec<ApiChoice>,
    usage: Option<ApiUsage>,
}

#[derive(Deserialize)]
struct ApiChoice {
    index: u32,
    message: ApiMessage,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct ApiUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

