//! Telegram Channel Integration for Crustaison
//!
//! Enhanced with commands for model switching and history management.

use crate::agent::Agent;
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
    user_states: std::sync::Mutex<HashMap<i64, UserState>>,
    default_provider: String,
}

impl TelegramHandler {
    pub fn new(agent: Arc<Mutex<Agent>>, default_provider: &str) -> Self {
        Self {
            agent,
            user_states: std::sync::Mutex::new(HashMap::new()),
            default_provider: default_provider.to_string(),
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
        
        // Get LLM response
        let mut agent = self.agent.lock().await;
        match agent.chat(text).await {
            Ok(response) => response,
            Err(e) => format!("Error: {}", e),
        }
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

/// Run the Telegram bot
pub async fn run_telegram_bot(
    bot_token: String,
    agent: Arc<Mutex<Agent>>,
    allowed_users: Vec<i64>,
) {
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
    let handler = Arc::new(TelegramHandler::new(agent, "minimax"));

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

            if let Some(text) = msg.text() {
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
                if response.len() > 4000 {
                    for chunk in response.chars().collect::<String>().as_bytes().chunks(4000) {
                        let chunk_str = String::from_utf8_lossy(chunk);
                        bot.send_message(chat_id, chunk_str.to_string()).await?;
                    }
                } else {
                    bot.send_message(chat_id, response).await?;
                }
            }

            Ok(())
        }
    }).await;
}
