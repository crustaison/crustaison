//! Image Tool - Analyze images using Qwen3-VL-2B vision model
//!
//! Calls the local Nexa serve API at http://localhost:18181 with multimodal messages.

use crate::tools::{Tool, ToolResult};
use base64::Engine as _;

const NEXA_URL: &str = "http://localhost:18181";
const VISION_MODEL: &str = "unsloth/Qwen3-VL-2B-Instruct-GGUF:Q4_0";
const MAX_TOKENS: u32 = 512;

/// Image analysis tool backed by Qwen3-VL-2B
pub struct ImageTool {
    client: reqwest::Client,
}

impl ImageTool {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Call VL-2B with a local file path (base64 encoded)
    async fn analyze_local_file(&self, path: &str, prompt: &str) -> ToolResult {
        let bytes = match tokio::fs::read(path).await {
            Ok(b) => b,
            Err(e) => return ToolResult::err(format!("Cannot read file '{}': {}", path, e)),
        };

        // Detect MIME type from extension
        let mime = if path.ends_with(".png") {
            "image/png"
        } else if path.ends_with(".gif") {
            "image/gif"
        } else if path.ends_with(".webp") {
            "image/webp"
        } else {
            "image/jpeg"
        };

        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
        let data_url = format!("data:{};base64,{}", mime, b64);

        self.call_vision_api(&data_url, prompt).await
    }

    /// Call VL-2B with a remote URL
    async fn analyze_url(&self, url: &str, prompt: &str) -> ToolResult {
        self.call_vision_api(url, prompt).await
    }

    /// POST to Nexa /v1/chat/completions with multimodal content
    async fn call_vision_api(&self, image_url: &str, prompt: &str) -> ToolResult {
        let body = serde_json::json!({
            "model": VISION_MODEL,
            "messages": [{
                "role": "user",
                "content": [
                    {
                        "type": "image_url",
                        "image_url": { "url": image_url }
                    },
                    {
                        "type": "text",
                        "text": prompt
                    }
                ]
            }],
            "max_tokens": MAX_TOKENS,
            "stream": false
        });

        let url = format!("{}/v1/chat/completions", NEXA_URL);

        match self.client.post(&url).json(&body).send().await {
            Ok(resp) => {
                let status = resp.status();
                match resp.json::<serde_json::Value>().await {
                    Ok(json) => {
                        if let Some(content) = json
                            .get("choices")
                            .and_then(|c| c.as_array())
                            .and_then(|arr| arr.first())
                            .and_then(|choice| choice.get("message"))
                            .and_then(|msg| msg.get("content"))
                            .and_then(|c| c.as_str())
                        {
                            // Strip any <think>...</think> from VL output
                            let result = strip_think(content);
                            ToolResult::ok(result)
                        } else if let Some(err) = json.get("error") {
                            ToolResult::err(format!("Vision API error: {}", err))
                        } else {
                            ToolResult::err(format!("Unexpected response ({}): {}", status, json))
                        }
                    }
                    Err(e) => ToolResult::err(format!("Failed to parse response: {}", e)),
                }
            }
            Err(e) => ToolResult::err(format!("Vision API request failed: {}", e)),
        }
    }
}

impl Default for ImageTool {
    fn default() -> Self {
        Self::new()
    }
}

/// Strip <think>...</think> from model output
fn strip_think(text: &str) -> String {
    let mut result = String::new();
    let mut remaining = text;
    while let Some(start) = remaining.find("<think>") {
        result.push_str(&remaining[..start]);
        if let Some(end) = remaining[start..].find("</think>") {
            remaining = &remaining[start + end + 8..];
        } else {
            break;
        }
    }
    result.push_str(remaining);
    result.trim().to_string()
}

#[async_trait::async_trait]
impl Tool for ImageTool {
    fn name(&self) -> &str {
        "image"
    }

    fn description(&self) -> &str {
        "Analyze images using the Qwen3-VL-2B vision model. Supports local file paths and URLs. Actions: describe, extract_text, analyze, info."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["describe", "extract_text", "analyze", "info"],
                    "description": "The image action to perform"
                },
                "source": {
                    "type": "string",
                    "description": "Image URL or local file path"
                },
                "prompt": {
                    "type": "string",
                    "description": "What to look for or extract (for analyze/extract_text actions)"
                }
            },
            "required": ["action", "source"]
        })
    }

    async fn call(&self, args: serde_json::Value) -> ToolResult {
        let action = match args.get("action").and_then(|v| v.as_str()) {
            Some(a) => a,
            None => return ToolResult::err("Missing 'action' parameter"),
        };

        let source = match args.get("source").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => return ToolResult::err("Missing 'source' parameter"),
        };

        let custom_prompt = args.get("prompt").and_then(|v| v.as_str());

        let is_url = source.starts_with("http://") || source.starts_with("https://");

        match action {
            "describe" => {
                let prompt = "Describe this image in detail. What do you see?";
                if is_url {
                    self.analyze_url(source, prompt).await
                } else {
                    self.analyze_local_file(source, prompt).await
                }
            }

            "extract_text" => {
                let prompt = custom_prompt.unwrap_or(
                    "Read and transcribe ALL text visible in this image exactly as written, \
                     including every date, time, name, and detail.",
                );
                if is_url {
                    self.analyze_url(source, prompt).await
                } else {
                    self.analyze_local_file(source, prompt).await
                }
            }

            "analyze" => {
                let prompt = custom_prompt.unwrap_or("Describe and analyze this image.");
                if is_url {
                    self.analyze_url(source, prompt).await
                } else {
                    self.analyze_local_file(source, prompt).await
                }
            }

            "info" => {
                if is_url {
                    ToolResult::ok(format!("URL image: {}", source))
                } else {
                    match std::fs::metadata(source) {
                        Ok(meta) => ToolResult::ok(format!(
                            "Local image: {}\nSize: {} bytes",
                            source,
                            meta.len()
                        )),
                        Err(_) => ToolResult::err(format!("File not found: {}", source)),
                    }
                }
            }

            _ => ToolResult::err(format!("Unknown action: {}", action)),
        }
    }
}
