//! Webhook System - HTTP callbacks and event handling
//!
//! Supports incoming webhooks and outbound webhook calls.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::collections::HashMap;
use std::time::Duration;

/// Webhook configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    pub enabled: bool,
    pub path: String,
    pub secret: Option<String>,
    pub events: Vec<String>,
}

/// Outbound webhook definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboundWebhook {
    pub name: String,
    pub url: String,
    pub events: Vec<String>,
    pub headers: HashMap<String, String>,
    pub timeout_seconds: u64,
}

/// Webhook event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookEvent {
    pub id: String,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub timestamp: i64,
    pub source: String,
}

/// Incoming webhook handler
pub struct WebhookServer {
    path: PathBuf,
    config: WebhookConfig,
    handlers: HashMap<String, Box<dyn Fn(WebhookEvent) + Send + Sync>>,
}

impl WebhookServer {
    /// Create new webhook server
    pub fn new(path: PathBuf, config: WebhookConfig) -> Self {
        Self {
            path,
            config,
            handlers: HashMap::new(),
        }
    }
    
    /// Register an event handler
    pub fn on(&mut self, event: &str, handler: Box<dyn Fn(WebhookEvent) + Send + Sync>) {
        self.handlers.insert(event.to_string(), handler);
    }
    
    /// Dispatch an event
    pub fn dispatch(&self, event_type: &str, payload: serde_json::Value, source: &str) {
        let event = WebhookEvent {
            id: uuid::Uuid::new_v4().to_string(),
            event_type: event_type.to_string(),
            payload,
            timestamp: chrono::Utc::now().timestamp_millis(),
            source: source.to_string(),
        };
        
        if let Some(handler) = self.handlers.get(event_type) {
            handler(event.clone());
        }
        
        // Also call wildcard handlers
        if let Some(handler) = self.handlers.get("*") {
            handler(event);
        }
    }
    
    /// Get config
    pub fn config(&self) -> &WebhookConfig {
        &self.config
    }
}

/// Outbound webhook client
pub struct WebhookClient {
    client: reqwest::Client,
    webhooks: Vec<OutboundWebhook>,
}

impl WebhookClient {
    /// Create new webhook client
    pub fn new(timeout_seconds: u64) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_seconds))
            .build()
            .unwrap();
        
        Self {
            client,
            webhooks: Vec::new(),
        }
    }
    
    /// Register an outbound webhook
    pub fn register(&mut self, webhook: OutboundWebhook) {
        self.webhooks.push(webhook);
    }
    
    /// Trigger an event on all registered webhooks
    pub async fn trigger(&self, event_type: &str, payload: &serde_json::Value) {
        for webhook in &self.webhooks {
            if webhook.events.contains(&event_type.to_string()) {
                let _ = self.send(&webhook, payload).await;
            }
        }
    }
    
    /// Send webhook request
    async fn send(&self, webhook: &OutboundWebhook, payload: &serde_json::Value) -> Result<(), String> {
        let mut request = self.client.post(&webhook.url)
            .json(payload);
        
        // Add custom headers
        for (key, value) in &webhook.headers {
            request = request.header(key, value);
        }
        
        let response = request.send().await
            .map_err(|e| e.to_string())?;
        
        if response.status().is_success() {
            Ok(())
        } else {
            Err(format!("Webhook failed: {}", response.status()))
        }
    }
    
    /// Test a webhook URL
    pub async fn test(&self, url: &str) -> Result<bool, String> {
        let response = self.client.get(url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        
        Ok(response.status().is_success())
    }
}

/// Webhook event types
pub mod events {
    use super::*;
    
    /// Message received event
    pub fn message_received(source: &str, message: &str) -> WebhookEvent {
        WebhookEvent {
            id: uuid::Uuid::new_v4().to_string(),
            event_type: "message.received".to_string(),
            payload: serde_json::json!({
                "message": message,
                "source": source
            }),
            timestamp: chrono::Utc::now().timestamp_millis(),
            source: source.to_string(),
        }
    }
    
    /// Session created event
    pub fn session_created(session_id: &str) -> WebhookEvent {
        WebhookEvent {
            id: uuid::Uuid::new_v4().to_string(),
            event_type: "session.created".to_string(),
            payload: serde_json::json!({
                "session_id": session_id
            }),
            timestamp: chrono::Utc::now().timestamp_millis(),
            source: "crustaison".to_string(),
        }
    }
    
    /// Tool execution event
    pub fn tool_executed(tool_name: &str, success: bool, duration_ms: u64) -> WebhookEvent {
        WebhookEvent {
            id: uuid::Uuid::new_v4().to_string(),
            event_type: "tool.executed".to_string(),
            payload: serde_json::json!({
                "tool": tool_name,
                "success": success,
                "duration_ms": duration_ms
            }),
            timestamp: chrono::Utc::now().timestamp_millis(),
            source: "crustaison".to_string(),
        }
    }
}
