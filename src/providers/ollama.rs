//! Ollama Provider
//!
//! Connects to local Ollama instance (localhost:11434).
//! Supports llama3, mistral, codellama, and other models.

use crate::providers::provider::{Provider, ProviderResult, ProviderError, ChatMessage, ProviderResponse, ModelInfo, UsageInfo};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Ollama configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaConfig {
    pub host: String,
    pub port: u16,
    pub model: String,
    pub timeout: u64,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 11434,
            model: "llama3".to_string(),
            timeout: 120,
        }
    }
}

/// Ollama provider implementation
pub struct OllamaProvider {
    client: reqwest::Client,
    base_url: String,
    model: String,
    timeout: Duration,
}

impl OllamaProvider {
    /// Create a new Ollama provider
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
    pub fn with_config(config: OllamaConfig) -> Self {
        Self::new(config.host, config.port, config.model)
    }
    
    /// Get available models from Ollama
    pub async fn list_models(&self) -> ProviderResult<Vec<String>> {
        let url = format!("{}/api/tags", self.base_url);
        
        let response = self.client.get(&url)
            .send()
            .await
            .map_err(|e| ProviderError::ConnectionFailed(e.to_string()))?;
        
        if !response.status().is_success() {
            return Err(ProviderError::ApiError(format!("Status: {}", response.status())));
        }
        
        #[derive(Deserialize)]
        struct TagsResponse {
            models: Vec<ModelTag>,
        }
        
        #[derive(Deserialize)]
        struct ModelTag {
            name: String,
        }
        
        let tags: TagsResponse = response
            .json()
            .await
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;
        
        Ok(tags.models.into_iter().map(|m| m.name).collect())
    }
    
    /// Pull a model if not present
    pub async fn pull_model(&self) -> ProviderResult<()> {
        let url = format!("{}/api/pull", self.base_url);
        
        let request = serde_json::json!({
            "name": self.model,
            "stream": false
        });
        
        let response = self.client.post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| ProviderError::ConnectionFailed(e.to_string()))?;
        
        if !response.status().is_success() {
            return Err(ProviderError::ApiError(format!("Status: {}", response.status())));
        }
        
        Ok(())
    }
}

#[async_trait::async_trait]
impl Provider for OllamaProvider {
    fn name(&self) -> &str {
        "ollama"
    }
    
    async fn model_info(&self) -> ProviderResult<ModelInfo> {
        // Try to get model info from Ollama
        let url = format!("{}/api/show", self.base_url);
        
        let request = serde_json::json!({
            "name": self.model
        });
        
        let response = self.client.post(&url)
            .json(&request)
            .send()
            .await;
        
        match response {
            Ok(r) if r.status().is_success() => {
                #[derive(Deserialize)]
                struct ShowResponse {
                    details: Option<ModelDetails>,
                }
                
                #[derive(Deserialize)]
                struct ModelDetails {
                    format: Option<String>,
                    family: Option<String>,
                    families: Option<Vec<String>>,
                    parameter_size: Option<String>,
                    quantization_level: Option<String>,
                }
                
                match r.json::<ShowResponse>().await {
                    Ok(show) => Ok(ModelInfo {
                        name: self.model.clone(),
                        version: show.details.and_then(|d| d.family),
                        context_length: 8192, // Ollama default
                    }),
                    Err(_) => Ok(ModelInfo {
                        name: self.model.clone(),
                        version: None,
                        context_length: 8192,
                    }),
                }
            }
            _ => Ok(ModelInfo {
                name: self.model.clone(),
                version: None,
                context_length: 8192,
            }),
        }
    }
    
    async fn chat(
        &self,
        messages: Vec<ChatMessage>,
        system_prompt: Option<String>,
    ) -> ProviderResult<ProviderResponse> {
        let url = format!("{}/api/chat", self.base_url);
        
        // Build messages - prepend system if provided
        let mut all_messages = Vec::new();
        
        if let Some(system) = system_prompt {
            all_messages.push(OllamaMessage {
                role: "system".to_string(),
                content: system,
            });
        }
        
        for msg in &messages {
            all_messages.push(OllamaMessage {
                role: msg.role.clone(),
                content: msg.content.clone(),
            });
        }
        
        let request = OllamaChatRequest {
            model: self.model.clone(),
            messages: all_messages,
            stream: false,
            options: serde_json::json!({
                "num_predict": 2048,
                "temperature": 0.7
            }),
        };
        
        let response = self.client.post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| ProviderError::ConnectionFailed(e.to_string()))?;
        
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ProviderError::ApiError(format!("{}: {}", status, body)));
        }
        
        let ollama_resp: OllamaChatResponse = response
            .json()
            .await
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;
        
        let choice = ollama_resp.message;
        
        let usage = ollama_resp.eval_count.map(|eval| UsageInfo {
            prompt_tokens: ollama_resp.prompt_eval_count.unwrap_or(0) as u32,
            completion_tokens: eval as u32,
            total_tokens: (ollama_resp.prompt_eval_count.unwrap_or(0) + eval as usize) as u32,
        });
        
        Ok(ProviderResponse {
            content: choice.content,
            usage,
        })
    }
    
    async fn is_available(&self) -> bool {
        let url = format!("{}/api/tags", self.base_url);
        
        if let Ok(response) = self.client.get(&url).send().await {
            return response.status().is_success();
        }
        
        false
    }
    
    fn default_model(&self) -> &str {
        &self.model
    }
}

// Internal Ollama types

#[derive(Serialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    stream: bool,
    options: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OllamaMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct OllamaChatResponse {
    model: String,
    message: OllamaMessage,
    done: bool,
    #[serde(default)]
    eval_count: Option<usize>,
    #[serde(default)]
    prompt_eval_count: Option<usize>,
    #[serde(default)]
    eval_duration: Option<u64>,
}

