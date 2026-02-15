//! Gateway - Authentication and Message Normalization
//!
//! The gateway is the IMMUTABLE boundary between the external world
//! and the cognition layer. It handles:
//! - Authentication
//! - Role mapping
//! - Message normalization
//! - Rate limiting
//!
//! CRITICAL: This module CANNOT be modified by the agent.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Authenticated identity from the gateway
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResult {
    pub identity: String,
    pub roles: Vec<String>,
    pub capabilities: Vec<String>,
    pub rate_limit_remaining: u32,
}

/// Incoming message to the gateway
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayMessage {
    pub raw: String,
    pub source: String,
    pub timestamp: i64,
    pub metadata: serde_json::Value,
}

/// Normalized message ready for cognition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedMessage {
    pub identity: String,
    pub roles: Vec<String>,
    pub content: String,
    pub intent: Option<String>,
    pub context: serde_json::Value,
    pub timestamp: i64,
}

/// Rate limiter state per identity
struct RateLimit {
    requests: Vec<Instant>,
    max_requests: u32,
    window: Duration,
}

impl RateLimit {
    fn new(max_requests: u32, window_seconds: u64) -> Self {
        Self {
            requests: Vec::new(),
            max_requests,
            window: Duration::from_secs(window_seconds),
        }
    }
    
    fn allow(&mut self) -> bool {
        let now = Instant::now();
        self.requests.retain(|t| now.duration_since(*t) < self.window);
        
        if self.requests.len() < self.max_requests as usize {
            self.requests.push(now);
            true
        } else {
            false
        }
    }
    
    fn remaining(&mut self) -> u32 {
        let now = Instant::now();
        self.requests.retain(|t| now.duration_since(*t) < self.window);
        self.max_requests.saturating_sub(self.requests.len() as u32)
    }
}

/// The Gateway - immutable safety boundary
pub struct Gateway {
    rate_limits: Arc<RwLock<HashMap<String, RateLimit>>>,
    max_requests: u32,
    window_seconds: u64,
}

impl Gateway {
    /// Create a new gateway with configuration
    pub fn new(max_requests: u32, window_seconds: u64) -> Self {
        Self {
            rate_limits: Arc::new(RwLock::new(HashMap::new())),
            max_requests,
            window_seconds,
        }
    }
    
    /// Process incoming message - authenticate and normalize
    pub async fn process(&self, message: GatewayMessage) -> Result<NormalizedMessage, String> {
        // 1. Rate limiting check
        let remaining = {
            let mut limits = self.rate_limits.write().await;
            let limit = limits.entry(message.source.clone()).or_insert_with(|| {
                RateLimit::new(self.max_requests, self.window_seconds)
            });
            
            if !limit.allow() {
                return Err("Rate limit exceeded".to_string());
            }
            limit.remaining()
        };
        
        // 2. Authenticate (simplified - extract identity from source)
        let identity = self.authenticate(&message).await?;
        
        // 3. Normalize message
        let normalized = NormalizedMessage {
            identity,
            roles: vec!["user".to_string()],
            content: message.raw.clone(),
            intent: self.extract_intent(&message.raw).await,
            context: message.metadata,
            timestamp: message.timestamp,
        };
        
        // 4. Log auth result for audit
        tracing::info!(
            source = %message.source,
            identity = %normalized.identity,
            intent = ?normalized.intent,
            rate_limit_remaining = %remaining,
            "Gateway processed message"
        );
        
        Ok(normalized)
    }
    
    /// Authenticate message source (simplified - returns identity)
    async fn authenticate(&self, message: &GatewayMessage) -> Result<String, String> {
        // In real implementation: verify Telegram ID, API key, etc.
        // For now: use source as identity
        Ok(message.source.clone())
    }
    
    /// Extract intent from message content
    async fn extract_intent(&self, content: &str) -> Option<String> {
        let content = content.to_lowercase();
        
        if content.contains("weather") {
            Some("get_weather".to_string())
        } else if content.contains("search") || content.contains("find") {
            Some("search".to_string())
        } else if content.contains("remember") || content.contains("note") {
            Some("store_memory".to_string())
        } else if content.contains("run") || content.contains("execute") {
            Some("execute".to_string())
        } else if content.contains("list") || content.contains("show") {
            Some("list".to_string())
        } else {
            Some("chat".to_string())
        }
    }
    
    /// Get rate limit status for an identity
    pub async fn get_rate_limit_status(&self, identity: &str) -> u32 {
        let limits = self.rate_limits.read().await;
        if let Some(limit) = limits.get(identity) {
            let now = Instant::now();
            let used = limit.requests.iter()
                .filter(|t| now.duration_since(**t) < limit.window)
                .count();
            limit.max_requests.saturating_sub(used as u32)
        } else {
            self.max_requests
        }
    }
}

impl Default for Gateway {
    fn default() -> Self {
        Self::new(100, 60)
    }
}
