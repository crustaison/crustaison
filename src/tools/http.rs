//! HTTP Tool - Make HTTP requests
//!
//! Execute HTTP GET and POST requests for REST APIs.

use crate::tools::{Tool, ToolResult};
use serde::{Deserialize, Serialize};
use reqwest::{Client, Method};
use std::sync::Arc;
use async_trait::async_trait;

/// Configuration for HTTP tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpConfig {
    /// Allowed base URLs (prefix matching)
    pub allowed_urls: Vec<String>,
    /// Default timeout (seconds)
    pub timeout: u64,
    /// User-Agent header
    pub user_agent: String,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            allowed_urls: vec![
                "http://localhost".to_string(),
                "http://127.0.0.1".to_string(),
                "http://localhost:8080".to_string(),
            ],
            timeout: 30,
            user_agent: "Crustaison/1.0".to_string(),
        }
    }
}

/// Request input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpRequest {
    /// URL to request
    pub url: String,
    /// HTTP method (GET, POST, PUT, DELETE)
    #[serde(default = "default_get")]
    pub method: String,
    /// JSON body (for POST/PUT)
    pub body: Option<serde_json::Value>,
    /// Headers to add
    pub headers: Option<std::collections::HashMap<String, String>>,
}

fn default_get() -> String {
    "GET".to_string()
}

/// HTTP tool result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpResult {
    pub status: u16,
    pub body: Option<String>,
    pub headers: std::collections::HashMap<String, String>,
}

/// HTTP tool - make GET/POST requests
pub struct HttpTool {
    client: Arc<Client>,
    config: HttpConfig,
}

impl HttpTool {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn with_config(config: HttpConfig) -> Self {
        Self {
            client: Arc::new(Client::builder()
                .timeout(std::time::Duration::from_secs(config.timeout))
                .build()
                .expect("HTTP client")),
            config,
        }
    }
}

impl Default for HttpTool {
    fn default() -> Self {
        Self {
            client: Arc::new(Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("HTTP client")),
            config: HttpConfig::default(),
        }
    }
}

#[async_trait]
impl Tool for HttpTool {
    fn name(&self) -> &str {
        "http"
    }
    
    fn description(&self) -> &str {
        "Make HTTP GET and POST requests to REST APIs. Supports JSON body and custom headers."
    }
    
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to request"
                },
                "method": {
                    "type": "string",
                    "description": "HTTP method (GET, POST, PUT, DELETE)",
                    "enum": ["GET", "POST", "PUT", "DELETE"],
                    "default": "GET"
                },
                "body": {
                    "type": "object",
                    "description": "JSON body for POST/PUT requests"
                },
                "headers": {
                    "type": "object",
                    "description": "Additional headers"
                }
            },
            "required": ["url"]
        })
    }
    
    async fn call(&self, args: serde_json::Value) -> ToolResult {
        let req_result: Result<HttpRequest, String> = serde_json::from_value(args)
            .map_err(|e| format!("Invalid request: {}", e));
        
        let req = match req_result {
            Ok(r) => r,
            Err(e) => return ToolResult::err(e),
        };
        
        // Validate URL
        let allowed = self.config.allowed_urls.iter().any(|u| req.url.starts_with(u));
        if !allowed {
            return ToolResult::err(format!("URL not allowed: {}. Only localhost URLs are permitted.", req.url));
        }
        
        let method_result = Method::from_bytes(req.method.as_bytes())
            .map_err(|e| format!("Invalid method: {}", e));
        
        let method = match method_result {
            Ok(m) => m,
            Err(e) => return ToolResult::err(e),
        };
        
        let mut request = self.client.request(method, &req.url);
        
        // Add headers
        if let Some(headers) = req.headers {
            for (k, v) in headers {
                request = request.header(k, v);
            }
        }
        
        // Add body if present
        if let Some(body) = req.body {
            request = request.json(&body);
        }
        
        let response_result = request.send().await
            .map_err(|e| format!("Request failed: {}", e));
        
        let response = match response_result {
            Ok(r) => r,
            Err(e) => return ToolResult::err(e),
        };
        
        let status = response.status().as_u16();
        let headers: std::collections::HashMap<String, String> = response
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();
        
        let body_result = response.text().await
            .map_err(|e| format!("Read body failed: {}", e));
        
        let body = match body_result {
            Ok(b) => b,
            Err(e) => return ToolResult::err(e),
        };
        
        let result = HttpResult {
            status,
            body: Some(body),
            headers,
        };
        
        ToolResult::ok(serde_json::to_string(&result).unwrap_or_else(|e| format!("{}", e)))
    }
}
