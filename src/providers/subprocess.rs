//! Subprocess Provider - Wraps `nexa infer` for stable 35B model inference
//!
//! Spawns a fresh `nexa infer` process per request, which avoids the context
//! corruption issues that occur when using 35B via `nexa serve`.

use crate::providers::provider::{Provider, ProviderResult, ProviderError, ChatMessage, ProviderResponse, ModelInfo};

/// Subprocess provider wrapping `nexa infer`
pub struct SubprocessProvider {
    model: String,
    binary: String,
}

impl SubprocessProvider {
    pub fn new(model: String) -> Self {
        Self {
            model,
            binary: "nexa".to_string(),
        }
    }
}

#[async_trait::async_trait]
impl Provider for SubprocessProvider {
    fn name(&self) -> &str {
        "nexa-subprocess"
    }

    async fn model_info(&self) -> ProviderResult<ModelInfo> {
        Ok(ModelInfo {
            name: self.model.clone(),
            version: None,
            context_length: 32768,
        })
    }

    async fn chat(
        &self,
        messages: Vec<ChatMessage>,
        system_prompt: Option<String>,
    ) -> ProviderResult<ProviderResponse> {
        // Build a simple text prompt from messages
        let mut parts: Vec<String> = Vec::new();

        if let Some(sys) = &system_prompt {
            parts.push(format!("System: {}", sys));
        }

        for msg in &messages {
            let role = if msg.role == "user" { "User" } else { "Assistant" };
            parts.push(format!("{}: {}", role, msg.content));
        }
        parts.push("Assistant:".to_string());

        let prompt = parts.join("\n\n");

        let output = tokio::process::Command::new(&self.binary)
            .args(["infer", &self.model, "--hide-think", "--max-tokens", "1024", "-p", &prompt])
            .output()
            .await
            .map_err(|e| ProviderError::ConnectionFailed(format!("Failed to spawn nexa: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ProviderError::ApiError(format!("nexa infer failed: {}", stderr)));
        }

        let raw = String::from_utf8_lossy(&output.stdout).to_string();
        let content = parse_nexa_infer_output(&raw);

        Ok(ProviderResponse {
            content,
            usage: None,
        })
    }

    async fn is_available(&self) -> bool {
        tokio::process::Command::new(&self.binary)
            .arg("--version")
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn default_model(&self) -> &str {
        &self.model
    }
}

/// Parse stdout from `nexa infer` and extract the response text.
/// Handles ANSI escape codes, spinner overwrites, and prompt-echo lines.
fn parse_nexa_infer_output(raw: &str) -> String {
    let clean = strip_ansi(raw);

    // Handle carriage-return overwrites: for each newline-segment, take the last \r part
    let mut display_lines: Vec<&str> = Vec::new();
    for nl_seg in clean.split('\n') {
        let last = nl_seg.split('\r').last().unwrap_or("");
        display_lines.push(last);
    }

    // Find the last prompt-echo line (starts with "> ")
    let last_prompt_idx = display_lines
        .iter()
        .enumerate()
        .rev()
        .find(|(_, line)| line.trim().starts_with("> "))
        .map(|(i, _)| i);

    let start_idx = last_prompt_idx.map(|i| i + 1).unwrap_or(0);

    display_lines[start_idx..]
        .iter()
        .filter(|line| {
            let t = line.trim();
            !t.is_empty()
                && !t.contains("tok/s")
                && !t.starts_with("loading")
                && !t.starts_with("encoding")
        })
        .copied()
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// Strip ANSI escape sequences from a string.
pub fn strip_ansi(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut result = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b {
            i += 1;
            if i < bytes.len() {
                match bytes[i] {
                    b'[' => {
                        i += 1;
                        while i < bytes.len() {
                            let b = bytes[i];
                            i += 1;
                            if b.is_ascii_alphabetic() {
                                break;
                            }
                        }
                    }
                    b']' => {
                        i += 1;
                        while i < bytes.len() {
                            if bytes[i] == 0x07 {
                                i += 1;
                                break;
                            }
                            if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'\\' {
                                i += 2;
                                break;
                            }
                            i += 1;
                        }
                    }
                    _ => {
                        i += 1;
                    }
                }
            }
        } else {
            result.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&result).into_owned()
}
