//! Model Switch Tool - Hot-swap the LLM provider at runtime

use crate::tools::tool::{Tool, ToolResult};
use crate::agent::Agent;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct ModelSwitchTool {
    agent: Arc<Mutex<Agent>>,
    minimax_api_key: String,
    minimax_model: String,
    minimax_base_url: Option<String>,
    nexa_host: String,
    nexa_port: u16,
}

impl ModelSwitchTool {
    pub fn new(
        agent: Arc<Mutex<Agent>>,
        minimax_api_key: String,
        minimax_model: String,
        minimax_base_url: Option<String>,
        nexa_host: String,
        nexa_port: u16,
    ) -> Self {
        Self { agent, minimax_api_key, minimax_model, minimax_base_url, nexa_host, nexa_port }
    }
}

#[async_trait]
impl Tool for ModelSwitchTool {
    fn name(&self) -> &str { "switch_model" }

    fn description(&self) -> &str {
        "Switch the active LLM model at runtime without restarting. \
         Use provider='nexa' to switch to a local Nexa model (fast, private, free). \
         Use provider='minimax' to switch back to MiniMax M2.1 (cloud, most capable). \
         For Nexa, specify a model name. Available Nexa models: \
         unsloth/Qwen3-1.7B-GGUF:Q4_0 (fast/small), \
         unsloth/Qwen3-4B-Thinking-2507-GGUF:Q4_K_XL (reasoning), \
         Manojb/Qwen3-4B-toolcalling-gguf-codex (tool use). \
         The switch takes effect immediately on the next message."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "provider": {
                    "type": "string",
                    "enum": ["nexa", "minimax"],
                    "description": "Which provider to switch to"
                },
                "model": {
                    "type": "string",
                    "description": "Model name — required for nexa, ignored for minimax. \
                                    E.g. 'unsloth/Qwen3-1.7B-GGUF:Q4_0'"
                }
            },
            "required": ["provider"]
        })
    }

    async fn call(&self, args: serde_json::Value) -> ToolResult {
        let provider = match args.get("provider").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ToolResult::err("Missing 'provider' parameter"),
        };

        match provider {
            "nexa" => {
                let model = match args.get("model").and_then(|v| v.as_str()) {
                    Some(m) => m.to_string(),
                    None => "unsloth/Qwen3-1.7B-GGUF:Q4_0".to_string(),
                };
                let nexa = crate::providers::nexa::NexaProvider::new(
                    self.nexa_host.clone(),
                    self.nexa_port,
                    model.clone(),
                );
                self.agent.lock().await.set_provider(Box::new(nexa));
                ToolResult::ok(format!("Switched to Nexa model: {}", model))
            }
            "minimax" => {
                let mm = crate::providers::MiniMaxProvider::new(
                    self.minimax_api_key.clone(),
                    self.minimax_model.clone(),
                    self.minimax_base_url.clone(),
                );
                self.agent.lock().await.set_provider(Box::new(mm));
                ToolResult::ok(format!("Switched back to MiniMax ({})", self.minimax_model))
            }
            other => ToolResult::err(format!("Unknown provider '{}'. Use 'nexa' or 'minimax'.", other)),
        }
    }
}
