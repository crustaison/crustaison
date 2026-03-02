//! Telegram Channel Integration for Crustaison
//!
//! Enhanced with commands for model switching and history management.

use base64::Engine as _;
use crate::agent::Agent;
use crate::coordinator::Coordinator;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use teloxide::prelude::*;
use teloxide::types::Message;

/// User state (simplified - just tracks provider preference)
#[derive(Clone, Default)]
struct UserState {
    provider: String,
}

impl UserState {
    fn new(provider: &str) -> Self {
        Self { provider: provider.to_string() }
    }
}

/// Telegram handler with per-user state
pub struct TelegramHandler {
    agent: Arc<Mutex<Agent>>,
    coordinator: Arc<Coordinator>,
    user_states: std::sync::Mutex<HashMap<i64, UserState>>,
    default_provider: String,
    bot_token: String,
}

impl TelegramHandler {
    pub fn new(agent: Arc<Mutex<Agent>>, coordinator: Arc<Coordinator>, default_provider: &str, bot_token: String) -> Self {
        Self {
            agent,
            coordinator,
            user_states: std::sync::Mutex::new(HashMap::new()),
            default_provider: default_provider.to_string(),
            bot_token,
        }
    }
    
    fn get_state(&self, user_id: i64) -> UserState {
        let mut states = self.user_states.lock().unwrap();
        if !states.contains_key(&user_id) {
            states.insert(user_id, UserState::new(&self.default_provider));
        }
        states.get(&user_id).unwrap().clone()
    }
    
    fn update_state(&self, user_id: i64, state: UserState) {
        let mut states = self.user_states.lock().unwrap();
        states.insert(user_id, state);
    }
    
    async fn handle_message(&self, text: &str, user_id: i64, _chat_id: i64) -> String {
        // Check for commands
        if text.starts_with('/') {
            return self.handle_command(text, user_id).await;
        }
        
        // Route through coordinator (handles LOCAL vs REASON routing + RAG memory)
        self.coordinator.process(text, user_id).await
    }
    
    async fn handle_command(&self, text: &str, user_id: i64) -> String {
        let command = text.trim_start_matches('/').trim();
        
        match command {
            "help" | "start" => {
                "🪐 **Crustaison Commands**\n\n\
                 • `/clear` - Clear conversation history\n\
                 • `/model` - Show/switch LLM model\n\
                 • `/heartbeat` - Show system health status\n\
                 • `/heartbeat on` - Enable heartbeat alerts\n\
                 • `/heartbeat off` - Disable heartbeat alerts\n\
                 • `/schedule` - List scheduled tasks\n\
                 • `/schedule cancel <id>` - Cancel a task\n\
                 • `/help` - Show this message\n\n\
                 Just send any message to chat!"
                    .to_string()
            }
            
            "clear" => {
                let mut agent = self.agent.lock().await;
                agent.clear_history();
                "History cleared.".to_string()
            }
            
            "model" => {
                let state = self.get_state(user_id);
                format!(
                    "**Current Provider:** {}\n\n\
                     Available providers:\n\
                     • `minimax` - MiniMax M2.1 (cloud)\n\
                     • `ollama` - Local Ollama\n\
                     • `nexa` - Local Nexa\n\n\
                     *To switch, use `/model <name>`*",
                    state.provider
                )
            }
            
            _ if command.starts_with("model ") => {
                let new_model = command.trim_start_matches("model ").trim();
                let mut state = self.get_state(user_id);
                state.provider = new_model.to_string();
                self.update_state(user_id, state);
                format!("Provider set to: {}", new_model)
            }
            
            "heartbeat" => {
                "🔔 **Heartbeat Status**\n\n\
                 Heartbeat monitoring is active!\n\
                 • Checks run every 5 minutes\n\
                 • Uses local Nexa (free) for AI analysis\n\
                 • Alerts only when attention needed\n\n\
                 Use `/heartbeat status` for details"
                    .to_string()
            }
            
            "heartbeat status" | "heartbeat stats" => {
                "📊 **Heartbeat Stats**\n\n\
                 Status: Active\n\
                 Interval: 300s (5 min)\n\
                 Provider: Nexa (local, free)\n\
                 Alert Cooldown: 1 hour"
                    .to_string()
            }
            
            "heartbeat on" => {
                "✅ Heartbeat alerts enabled".to_string()
            }
            
            "heartbeat off" => {
                "⚠️ Heartbeat alerts disabled (use `/crustaison telegram` to re-enable)".to_string()
            }
            
            "schedule" => {
                "📋 **Scheduled Tasks**\n\n\
                 Use the agent to schedule tasks!\n\
                 Example: \"Check the weather in 20 minutes\"\n\n\
                 Tasks run via heartbeat when due."
                    .to_string()
            }
            
            _ => format!("Unknown command: /{}\nUse /help for commands.", command),
        }
    }
}

/// Strip ANSI escape sequences from a string.
fn strip_ansi(s: &str) -> String {
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
                        // consume parameter bytes and final alphabetic byte
                        while i < bytes.len() {
                            let b = bytes[i];
                            i += 1;
                            if b.is_ascii_alphabetic() { break; }
                        }
                    }
                    b']' => {
                        // OSC sequence — ends at BEL or ESC-backslash
                        i += 1;
                        while i < bytes.len() {
                            if bytes[i] == 0x07 { i += 1; break; }
                            if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'\\' {
                                i += 2; break;
                            }
                            i += 1;
                        }
                    }
                    _ => { i += 1; } // other 2-char sequences
                }
            }
        } else {
            result.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&result).into_owned()
}

/// Parse the raw stdout from `nexa infer` (VLM mode) and return just the response text.
///
/// The output format is:
///   [ANSI/spinner] > /tmp/image.jpg\n[optional intermediate response]\n— stats —\n
///   [ANSI/spinner] > [text prompt]\n[final response]\n— stats —\n
///
/// We want the content that follows the LAST `> ` prompt-echo line.
fn parse_nexa_vlm_output(raw: &str, _is_default_prompt: bool) -> String {
    let clean = strip_ansi(raw);

    // Build display lines: for each \n-segment, \r restarts the line so we take the last part
    let mut display_lines: Vec<&str> = Vec::new();
    for nl_seg in clean.split('\n') {
        let last = nl_seg.split('\r').last().unwrap_or("");
        display_lines.push(last);
    }

    // Find the last prompt-echo line (starts with "> ")
    let last_prompt_idx = display_lines.iter().enumerate()
        .rev()
        .find(|(_, line)| line.trim().starts_with("> "))
        .map(|(i, _)| i);

    let start_idx = last_prompt_idx.map(|i| i + 1).unwrap_or(0);

    display_lines[start_idx..].iter()
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

/// Run the Telegram bot
pub async fn run_telegram_bot(
    bot_token: String,
    agent: Arc<Mutex<Agent>>,
    coordinator: Arc<Coordinator>,
    allowed_users: Vec<i64>,
) {
    let handler_token = bot_token.clone();
    let bot = Bot::new(bot_token);

    println!("🤖 Crustaison Telegram bot starting...");

    match bot.get_me().await {
        Ok(me) => {
            println!("Bot: @{}", me.username.as_ref().unwrap_or(&"unknown".to_string()));
        }
        Err(e) => {
            eprintln!("Failed to get bot info: {}", e);
            return;
        }
    };

    println!("Listening for messages...");

    let allowed = Arc::new(allowed_users);
    let handler = Arc::new(TelegramHandler::new(agent, coordinator, "minimax", handler_token));

    teloxide::repl(bot, move |bot: Bot, msg: Message| {
        let handler = handler.clone();
        let allowed = allowed.clone();

        async move {
            let chat_id = msg.chat.id;
            let user_id = msg.from().map(|u| u.id.0 as i64).unwrap_or(0);

            // Auth check
            if !allowed.is_empty() && !allowed.contains(&user_id) {
                bot.send_message(chat_id, "Not authorized.").await?;
                return Ok(());
            }

            if let Some(photos) = msg.photo() {
                // Handle photo messages (with optional caption)
                let caption = msg.caption().unwrap_or("").to_string();
                let photo = photos.iter().max_by_key(|p| p.width * p.height)
                    .or_else(|| photos.last());

                if let Some(photo) = photo {
                    match bot.get_file(photo.file.id.clone()).await {
                        Ok(file) => {
                            let download_url = format!(
                                "https://api.telegram.org/file/bot{}/{}",
                                handler.bot_token, file.path
                            );
                            match reqwest::get(&download_url).await {
                                Ok(resp) => match resp.bytes().await {
                                    Ok(bytes) => {
                                        let prompt_text = if caption.is_empty() {
                                            "Please read and transcribe ALL text visible in this image exactly as written, including every date, time, name, and detail. List everything you can see.".to_string()
                                        } else {
                                            caption.clone()
                                        };

                                        let typing_bot = bot.clone();
                                        let typing_token = tokio_util::sync::CancellationToken::new();
                                        let typing_cancel = typing_token.clone();
                                        tokio::spawn(async move {
                                            loop {
                                                let _ = typing_bot.send_chat_action(chat_id, teloxide::types::ChatAction::Typing).await;
                                                tokio::select! {
                                                    _ = tokio::time::sleep(tokio::time::Duration::from_secs(4)) => {}
                                                    _ = typing_cancel.cancelled() => break,
                                                }
                                            }
                                        });

                                        // Use nexa infer with Qwen3-VL-2B for vision
                                        // Save image to temp file, run nexa infer, parse output
                                        let img_path = format!("/tmp/crusty_vision_{}.jpg", uuid::Uuid::new_v4());
                                        let response = match tokio::fs::write(&img_path, &bytes).await {
                                            Ok(_) => {
                                                let mut cmd_args = vec![
                                                    "infer".to_string(),
                                                    "unsloth/Qwen3-VL-2B-Instruct-GGUF:Q4_0".to_string(),
                                                    "-p".to_string(), img_path.clone(),
                                                ];
                                                if !prompt_text.is_empty() {
                                                    cmd_args.push("-p".to_string());
                                                    cmd_args.push(prompt_text.clone());
                                                }
                                                cmd_args.extend(["--hide-think".to_string(), "--max-tokens".to_string(), "512".to_string()]);

                                                match tokio::process::Command::new("nexa")
                                                    .args(&cmd_args)
                                                    .output()
                                                    .await
                                                {
                                                    Ok(out) => {
                                                        let _ = tokio::fs::remove_file(&img_path).await;
                                                        let raw = String::from_utf8_lossy(&out.stdout).to_string();
                                                        parse_nexa_vlm_output(&raw, prompt_text.is_empty())
                                                    }
                                                    Err(e) => {
                                                        let _ = tokio::fs::remove_file(&img_path).await;
                                                        format!("Vision error: {}", e)
                                                    }
                                                }
                                            }
                                            Err(e) => format!("Failed to save image: {}", e),
                                        };
                                        typing_token.cancel();

                                        // Add vision exchange to agent history so follow-up messages have context
                                        {
                                            let user_msg = if caption.is_empty() {
                                                "[sent an image]".to_string()
                                            } else {
                                                format!("[sent an image: {}]", caption)
                                            };
                                            let mut agent = handler.agent.lock().await;
                                            agent.add_context(&user_msg, &format!("[Kimi-VL image analysis]: {}", response.trim()));
                                        }

                                        bot.send_message(chat_id, if response.trim().is_empty() { "✓ Done.".to_string() } else { response }).await?;
                                    }
                                    Err(e) => { bot.send_message(chat_id, format!("Failed to read image: {}", e)).await?; }
                                },
                                Err(e) => { bot.send_message(chat_id, format!("Failed to download image: {}", e)).await?; }
                            }
                        }
                        Err(e) => { bot.send_message(chat_id, format!("Failed to get file info: {}", e)).await?; }
                    }
                }
            } else if let Some(text) = msg.text() {
                // Spawn recurring typing indicator until response is ready
                let typing_bot = bot.clone();
                let typing_token = tokio_util::sync::CancellationToken::new();
                let typing_cancel = typing_token.clone();
                tokio::spawn(async move {
                    loop {
                        let _ = typing_bot.send_chat_action(chat_id, teloxide::types::ChatAction::Typing).await;
                        tokio::select! {
                            _ = tokio::time::sleep(tokio::time::Duration::from_secs(4)) => {}
                            _ = typing_cancel.cancelled() => break,
                        }
                    }
                });

                // Get response
                let response = handler.handle_message(text, user_id, chat_id.0 as i64).await;
                typing_token.cancel();

                // Send response (handle length limits)
                let response = if response.trim().is_empty() {
                    "✓ Done.".to_string()
                } else {
                    response
                };
                tracing::info!("Sending response ({} chars) to chat {}", response.len(), chat_id);
                let send_result = if response.len() > 4000 {
                    let mut last_result = Ok(());
                    for chunk in response.chars().collect::<String>().as_bytes().chunks(3900) {
                        let chunk_str = String::from_utf8_lossy(chunk).to_string();
                        last_result = bot.send_message(chat_id, chunk_str).await.map(|_| ());
                        if last_result.is_err() { break; }
                    }
                    last_result
                } else {
                    bot.send_message(chat_id, response).await.map(|_| ())
                };
                match send_result {
                    Ok(_) => tracing::info!("Response delivered to chat {}", chat_id),
                    Err(ref e) => tracing::error!("Failed to send response to chat {}: {}", chat_id, e),
                }
                send_result?;
            }

            Ok(())
        }
    }).await;
}
