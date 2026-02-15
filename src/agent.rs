//! Agent Runtime - The Brain
//!
//! Connects the LLM provider to the gateway, doctrine, memory,
//! tools, and working memory to produce intelligent responses.

use anyhow::{Context, Result};
use crate::providers::provider::{ChatMessage, Provider};
use crate::cognition::{DoctrineLoader, Doctrine};
use crate::tools::{ToolRegistry, ToolResult};
use crate::authority::Executor;
use crate::ledger::GitLedger;
use crate::sessions::SessionManager;

/// Parsed tool call from LLM response
#[derive(Debug, Clone)]
pub struct ToolCall {
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Agent trait for polymorphic use
#[async_trait::async_trait]
pub trait AgentTrait: Send + Sync {
    async fn chat(&mut self, message: &str) -> Result<String, anyhow::Error>;
    fn clear_history(&mut self);
}

pub struct Agent {
    provider: Box<dyn Provider>,
    doctrine_loader: DoctrineLoader,
    system_prompt: String,
    history: Vec<ChatMessage>,
    max_history: usize,
    tool_registry: Option<std::sync::Arc<ToolRegistry>>,
    max_tool_iterations: usize,
    executor: Option<std::sync::Arc<Executor>>,
    git_ledger: Option<std::sync::Arc<GitLedger>>,
    session_manager: Option<std::sync::Arc<SessionManager>>,
    current_session_id: Option<String>,
}

impl Agent {
    pub async fn new<P: Provider + 'static>(
        provider: P,
        doctrine_loader: DoctrineLoader,
    ) -> Result<Self, anyhow::Error> {
        Self::with_executor(provider, doctrine_loader, None, None, None).await
    }
    
    pub async fn with_tools<P: Provider + 'static>(
        provider: P,
        doctrine_loader: DoctrineLoader,
        tool_registry: Option<std::sync::Arc<ToolRegistry>>,
    ) -> Result<Self, anyhow::Error> {
        Self::with_executor(provider, doctrine_loader, tool_registry, None, None).await
    }
    
    pub async fn with_executor<P: Provider + 'static>(
        provider: P,
        doctrine_loader: DoctrineLoader,
        tool_registry: Option<std::sync::Arc<ToolRegistry>>,
        executor: Option<std::sync::Arc<Executor>>,
        git_ledger: Option<std::sync::Arc<GitLedger>>,
    ) -> Result<Self, anyhow::Error> {
        Self::with_session_manager(provider, doctrine_loader, tool_registry, executor, git_ledger, None, None).await
    }
    
    pub async fn with_session_manager<P: Provider + 'static>(
        provider: P,
        doctrine_loader: DoctrineLoader,
        tool_registry: Option<std::sync::Arc<ToolRegistry>>,
        executor: Option<std::sync::Arc<Executor>>,
        git_ledger: Option<std::sync::Arc<GitLedger>>,
        session_manager: Option<std::sync::Arc<SessionManager>>,
        session_id: Option<String>,
    ) -> Result<Self, anyhow::Error> {
        let mut agent = Self {
            provider: Box::new(provider),
            doctrine_loader,
            system_prompt: String::new(),
            history: Vec::new(),
            max_history: 20,
            tool_registry,
            max_tool_iterations: 5,
            executor,
            git_ledger,
            session_manager,
            current_session_id: session_id,
        };
        
        // Load session history if session_id provided
        if let (Some(sm), Some(sid)) = (&agent.session_manager, &agent.current_session_id) {
            if let Ok(messages) = sm.get_messages(sid).await {
                for msg in messages {
                    agent.history.push(ChatMessage {
                        role: msg.role,
                        content: msg.content,
                    });
                }
                tracing::info!("Loaded {} messages from session {}", agent.history.len(), sid);
            }
        }
        
        agent.build_system_prompt().await;
        Ok(agent)
    }
    
    async fn build_system_prompt(&mut self) {
        let doctrine = self.doctrine_loader.load().await
            .unwrap_or_else(|_| Doctrine {
                soul: None,
                agents: None,
                principles: None,
            });

        let now = chrono::Local::now();
        let date_str = now.format("%A, %B %e, %Y %I:%M %p").to_string();

        let mut prompt = format!(
            "You are Crusty (Crustaison), a self-improving AI agent running on Sean's machine (ryz.local).\n\
             You are powered by MiniMax M2.1.\n\
             The current date and time is: {}.\n\
             You are located in Eldon, Missouri (Lake of the Ozarks area).\n\
             Your nickname is Crusty or Crust.\n\n",
            date_str
        );

        if let Some(ref registry) = self.tool_registry {
            let tools = registry.descriptions().await;
            if !tools.is_empty() {
                prompt.push_str("## Available Tools\n\n");
                prompt.push_str("When you need to use a tool, output a JSON object in this format:\n");
                prompt.push_str("```json\n{\"tool\": \"tool_name\", \"arguments\": {}}\n```\n\n");
                prompt.push_str("### Tools:\n\n");
                for (name, desc, _) in &tools {
                    prompt.push_str(&format!("- **{}**: {}\n", name, desc));
                }
                prompt.push_str("\nAfter receiving tool results, continue the conversation naturally.\n\n");
                prompt.push_str("## Capabilities\n\n");
                prompt.push_str("- IMPORTANT: When a user asks you to do something in the future (weather check, reminder, timed task), you MUST use the 'schedule' tool. NEVER use bash sleep or nohup for delayed tasks — those do not push results to Telegram.\n");
                prompt.push_str("- The 'schedule' tool queues tasks for the heartbeat system which AUTOMATICALLY sends results to the user via Telegram when they are due.\n");
                prompt.push_str("- For scheduled weather: use schedule with action='weather' and location='City, State'. For reminders: action='reminder'. For commands: action='command'.\n");
                prompt.push_str("- You can check weather, run commands, read/write files, and search the web\n");
                prompt.push_str("- You have a GitHub account (crustaison) and can create repos, push code, and manage issues\n");
                prompt.push_str("- You can send and read emails via crustaison@gmail.com\n\n");
                prompt.push_str("## Self-Improvement (Plugin System)\n\n");
                prompt.push_str("You have the ability to extend yourself by writing plugins. When you encounter a limitation:\n");
                prompt.push_str("1. Identify what capability is missing\n");
                prompt.push_str("2. Write a Python script that implements it\n");
                prompt.push_str("3. Save it as a plugin in ~/.config/crustaison/plugins/<name>/\n");
                prompt.push_str("4. Create a manifest.json with: name, description, script filename, interpreter, and parameters\n");
                prompt.push_str("5. The plugin will be available after restart (or tell the user you wrote a plugin and it needs a restart)\n\n");
                prompt.push_str("Plugin script contract: receives JSON args on stdin, prints JSON to stdout:\n");
                prompt.push_str("  Success: {\"success\": true, \"output\": \"result text\"}\n");
                prompt.push_str("  Failure: {\"success\": false, \"error\": \"error message\"}\n\n");
                prompt.push_str("Example manifest.json:\n");
                prompt.push_str("{\"name\": \"my_tool\", \"description\": \"What it does\", \"script\": \"plugin.py\", \"interpreter\": \"python3\", \"parameters\": {}}\n\n");
            }
        }

        if let Some(soul) = &doctrine.soul {
            prompt.push_str("## Your Soul\n");
            prompt.push_str(soul);
            prompt.push_str("\n\n");
        }
        if let Some(agents) = &doctrine.agents {
            prompt.push_str("## Operating Rules\n");
            prompt.push_str(agents);
            prompt.push_str("\n\n");
        }
        if let Some(principles) = &doctrine.principles {
            prompt.push_str("## Principles\n");
            prompt.push_str(principles);
            prompt.push_str("\n\n");
        }

        self.system_prompt = prompt;
    }

    pub async fn chat(&mut self, user_message: &str) -> Result<String, anyhow::Error> {
        self.history.push(ChatMessage {
            role: "user".to_string(),
            content: user_message.to_string(),
        });

        // Save user message to session
        if let (Some(sm), Some(sid)) = (&self.session_manager, &self.current_session_id) {
            let _ = sm.add_message(sid, "user", user_message).await;
        }

        self.trim_history();

        let response = self.process_with_tools().await?;
        
        // Save assistant response to session
        if let (Some(sm), Some(sid)) = (&self.session_manager, &self.current_session_id) {
            let _ = sm.add_message(sid, "assistant", &response).await;
        }
        
        Ok(response)
    }

    async fn process_with_tools(&mut self) -> Result<String, anyhow::Error> {
        let mut iterations = 0;
        
        loop {
            let response = self.provider.chat(
                self.history.clone(),
                Some(self.system_prompt.clone()),
            ).await.context("Failed to get LLM response")?;
            
            let content = response.content.clone();
            
            tracing::debug!("LLM response (first 500 chars): {}", &content[..content.len().min(500)]);
            if let Some(tool_calls) = parse_tool_calls(&content) {
                iterations += 1;
                
                if iterations > self.max_tool_iterations {
                    self.history.push(ChatMessage {
                        role: "assistant".to_string(),
                        content: content.clone(),
                    });
                    self.history.push(ChatMessage {
                        role: "user".to_string(),
                        content: "Too many tool calls. Please provide a final answer.".to_string(),
                    });
                    
                    let final_response = self.provider.chat(
                        self.history.clone(),
                        Some(self.system_prompt.clone()),
                    ).await?;
                    
                    self.history.push(ChatMessage {
                        role: "assistant".to_string(),
                        content: final_response.content.clone(),
                    });
                    
                    return Ok(strip_tool_calls(&final_response.content));
                }
                
                self.history.push(ChatMessage {
                    role: "assistant".to_string(),
                    content: content.clone(),
                });
                
                tracing::info!("Parsed {} tool call(s)", tool_calls.len());
                for tool_call in tool_calls {
                    tracing::info!("Executing tool '{}' with args: {}", tool_call.name, tool_call.arguments);
                    let result = self.execute_tool_call(&tool_call).await;
                    tracing::info!("Tool '{}' success={}, output_len={}", tool_call.name, result.success, result.output.len());
                    
                    let result_text = if result.success {
                        format!("Tool '{}' result:\n{}", tool_call.name, result.output)
                    } else {
                        format!("Tool '{}' error: {}\n\nNote: Check the parameters you provided. See tool documentation for expected inputs.", 
                            tool_call.name, result.error.as_deref().unwrap_or("Unknown error"))
                    };
                    
                    self.history.push(ChatMessage {
                        role: "user".to_string(),
                        content: result_text,
                    });
                }
                
                continue;
            }
            
            self.history.push(ChatMessage {
                role: "assistant".to_string(),
                content: content.clone(),
            });
            
            if let Some(usage) = &response.usage {
                tracing::info!(
                    prompt_tokens = usage.prompt_tokens,
                    completion_tokens = usage.completion_tokens,
                    total_tokens = usage.total_tokens,
                    "LLM usage"
                );
            }
            
            return Ok(strip_tool_calls(&content));
        }
    }

    async fn execute_tool_call(&self, tool_call: &ToolCall) -> ToolResult {
        let tool_name = &tool_call.name;
        let args = &tool_call.arguments;
        
        // Policy check via executor (if available)
        if let Some(ref exec) = self.executor {
            tracing::info!("Policy check for '{}'", tool_name);
            
            let cmd = crate::authority::Command {
                name: tool_name.clone(),
                parameters: args.clone(),
                context: serde_json::json!({}),
            };
            
            match exec.execute(cmd).await {
                Ok(result) if !result.success => {
                    // Policy denied - log and return error
                    tracing::warn!("Tool '{}' denied by policy: {:?}", tool_name, result.error);
                    if let Some(ref ledger) = self.git_ledger {
                        let log_content = serde_json::json!({
                            "tool": tool_name,
                            "arguments": args,
                            "policy": "denied",
                            "reason": result.error
                        });
                        let _ = ledger.add("tool_denied", &log_content).await;
                    }
                    return ToolResult {
                        success: false,
                        output: String::new(),
                        error: result.error.or(Some("Denied by policy".to_string())),
                        metadata: None,
                    };
                }
                Ok(_) => {
                    // Policy allowed - fall through to tool registry
                    tracing::info!("Tool '{}' approved by policy", tool_name);
                }
                Err(e) => {
                    tracing::warn!("Executor error for '{}': {}", tool_name, e);
                    // On executor error, still allow tool execution (fail open)
                }
            }
        }
        
        // Fallback: execute directly via tool registry (no policy check)
        if let Some(ref registry) = self.tool_registry {
            let result = registry.execute(tool_name, args.clone()).await;
            
            // Log to ledger
            if let Some(ref ledger) = self.git_ledger {
                let log_content = serde_json::json!({
                    "tool": tool_name,
                    "arguments": args,
                    "success": result.success,
                    "output": result.output
                });
                let _ = ledger.add("tool_executed", &log_content).await;
            }
            
            result
        } else {
            ToolResult::err("No tool registry or executor available")
        }
    }

    pub fn clear_history(&mut self) {
        self.history.clear();
    }
    
    /// Switch to a different session
    pub async fn switch_session(&mut self, session_id: &str) -> Result<(), anyhow::Error> {
        if let Some(sm) = &self.session_manager {
            // Save current session
            self.current_session_id = None;
            
            // Load new session
            self.current_session_id = Some(session_id.to_string());
            let messages = sm.get_messages(session_id).await?;
            
            self.history.clear();
            for msg in messages {
                self.history.push(ChatMessage {
                    role: msg.role,
                    content: msg.content,
                });
            }
            tracing::info!("Switched to session {} ({} messages)", session_id, self.history.len());
        }
        Ok(())
    }
    
    /// Get current session ID
    pub fn current_session(&self) -> Option<&str> {
        self.current_session_id.as_deref()
    }

    fn trim_history(&mut self) {
        if self.history.len() > self.max_history {
            let drain_count = self.history.len() - self.max_history;
            self.history.drain(0..drain_count);
        }
    }
}

#[async_trait::async_trait]
impl AgentTrait for Agent {
    async fn chat(&mut self, message: &str) -> Result<String, anyhow::Error> {
        Agent::chat(self, message).await
    }
    
    fn clear_history(&mut self) {
        self.history.clear();
    }
}

fn parse_tool_calls(text: &str) -> Option<Vec<ToolCall>> {
    // Strategy 1: Look for JSON in markdown code fences (```json ... ```)
    if let Some(fence_start) = text.find("```json") {
        let json_start = fence_start + 7; // skip past "```json" (7 chars)
        if let Some(fence_end) = text[json_start..].find("```") {
            let json_str = text[json_start..json_start + fence_end].trim();
            tracing::debug!("Found fenced JSON: {}", json_str);

            // Try parsing as a single tool call object
            if let Ok(call) = serde_json::from_str::<serde_json::Value>(json_str) {
                if let Some(tc) = try_parse_single_tool_call(&call) {
                    return Some(vec![tc]);
                }
                // Try as array of tool calls
                if let Some(arr) = call.as_array() {
                    let calls: Vec<ToolCall> = arr.iter()
                        .filter_map(|v| try_parse_single_tool_call(v))
                        .collect();
                    if !calls.is_empty() {
                        return Some(calls);
                    }
                }
            }
        }
    }

    // Strategy 2: Look for bare JSON objects with "tool" key using brace matching
    let mut results = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '{' {
            // Find matching closing brace
            let mut depth = 0;
            let start = i;
            let mut found_end = None;
            for j in i..chars.len() {
                match chars[j] {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            found_end = Some(j);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            if let Some(end) = found_end {
                let json_str: String = chars[start..=end].iter().collect();
                if json_str.contains("\"tool\"") {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&json_str) {
                        if let Some(tc) = try_parse_single_tool_call(&val) {
                            tracing::debug!("Found bare JSON tool call: {}", tc.name);
                            results.push(tc);
                        }
                    }
                }
                i = end + 1;
                continue;
            }
        }
        i += 1;
    }
    if !results.is_empty() {
        return Some(results);
    }

    // Strategy 3: Check for MiniMax-style XML tool calls
    if text.contains("<minimax:tool_call>") || text.contains("<tool_call>") {
        // Extract content between tags and try to parse
        let tag_pairs = [
            ("<minimax:tool_call>", "</minimax:tool_call>"),
            ("<tool_call>", "</tool_call>"),
        ];
        for (open, close) in &tag_pairs {
            if let Some(start) = text.find(open) {
                let content_start = start + open.len();
                if let Some(end) = text[content_start..].find(close) {
                    let inner = text[content_start..content_start + end].trim();
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(inner) {
                        if let Some(tc) = try_parse_single_tool_call(&val) {
                            return Some(vec![tc]);
                        }
                    }
                }
            }
        }
    }

    None
}

/// Try to extract a ToolCall from a JSON value
fn try_parse_single_tool_call(val: &serde_json::Value) -> Option<ToolCall> {
    let name = val.get("tool")
        .or_else(|| val.get("name"))
        .or_else(|| val.get("function"))
        .and_then(|v| v.as_str())?;

    let args = val.get("arguments")
        .or_else(|| val.get("args"))
        .or_else(|| val.get("parameters"))
        .or_else(|| val.get("input"))
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));

    Some(ToolCall {
        name: name.to_string(),
        arguments: args,
    })
}

fn strip_tool_calls(text: &str) -> String {
    let mut result = text.to_string();

    while let Some(start) = result.find("```json") {
        if let Some(end) = result[start+6..].find("```") {
            result = format!("{}{}", &result[..start], &result[start+6+end+3..]);
        } else {
            result = result[..start].to_string();
        }
    }

    while let Some(start) = result.find("<tool_call>") {
        if let Some(end) = result[start..].find("</tool_call>") {
            result = format!("{}{}", &result[..start], &result[start+end+13..]);
        } else {
            result = result[..start].to_string();
        }
    }

    // Strip <think>...</think> reasoning tags (MiniMax/DeepSeek expose these)
    while let Some(start) = result.find("<think>") {
        if let Some(end) = result[start..].find("</think>") {
            result = format!("{}{}", &result[..start], &result[start+end+8..]);
        } else {
            // Unclosed think tag - strip from <think> to end
            result = result[..start].to_string();
        }
    }

    result.trim().to_string()
}
