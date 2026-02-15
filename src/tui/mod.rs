//! Simple TUI for Crustaison
//!
//! Basic terminal UI without complex dependencies.

use crate::agent::AgentTrait;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::io::{self, Write};

/// ANSI color codes
struct Colors {
    cyan: String,
    green: String,
    yellow: String,
    red: String,
    magenta: String,
    reset: String,
    bold: String,
}

impl Colors {
    fn new() -> Self {
        Self {
            cyan: "\x1b[36m".to_string(),
            green: "\x1b[32m".to_string(),
            yellow: "\x1b[33m".to_string(),
            red: "\x1b[31m".to_string(),
            magenta: "\x1b[35m".to_string(),
            reset: "\x1b[0m".to_string(),
            bold: "\x1b[1m".to_string(),
        }
    }
}

impl Default for Colors {
    fn default() -> Self {
        Self::new()
    }
}

/// A message to display in the TUI
struct TuiMessage {
    role: String,
    content: String,
}

impl TuiMessage {
    fn user(content: String) -> Self {
        Self { role: "user".to_string(), content }
    }
    
    fn assistant(content: String) -> Self {
        Self { role: "assistant".to_string(), content }
    }
}

fn print_message(colors: &Colors, msg: &TuiMessage) {
    let prefix = match msg.role.as_str() {
        "user" => format!("{}user{}>", colors.green, colors.reset),
        "assistant" => format!("{}crust{}>", colors.magenta, colors.reset),
        _ => format!("{}unknown{}>", colors.yellow, colors.reset),
    };
    println!("{} {}", prefix, msg.content);
}

/// Simple TUI that just uses println
pub async fn run_tui(
    agent: Arc<Mutex<dyn AgentTrait>>,
) -> Result<(), anyhow::Error> {
    let colors = Colors::new();
    
    println!("{}🪐 {}Crustaison v0.1.0{} - Terminal Interface{}", 
        colors.cyan, colors.bold, colors.reset, colors.cyan);
    println!("{}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━{}", colors.cyan, colors.reset);
    println!("{}Type {}/quit{} to exit{}\n", colors.yellow, colors.bold, colors.reset, colors.yellow);
    
    let stdin = io::stdin();
    let mut input = String::new();
    
    let mut messages: Vec<TuiMessage> = Vec::new();
    
    messages.push(TuiMessage::assistant(
        "I'm ready to help! Type a message and press Enter.\nCommands:\n- /clear - Clear conversation\n- /quit - Exit\n- /sessions - List sessions".to_string()
    ));
    
    for msg in &messages {
        print_message(&colors, &msg);
    }
    
    loop {
        print!("{}you{}> {}",
            colors.green, 
            colors.reset,
            colors.bold
        );
        io::stdout().flush()?;
        
        input.clear();
        stdin.read_line(&mut input)?;
        let input = input.trim();
        
        if input.is_empty() {
            continue;
        }
        
        if input == "/quit" || input == "/exit" {
            println!("{}Goodbye! 👋{}", colors.magenta, colors.reset);
            break;
        }
        
        if input == "/clear" {
            messages.clear();
            println!("{}Conversation cleared.{}", colors.yellow, colors.reset);
            continue;
        }
        
        if input == "/sessions" {
            println!("{}📋 Session commands coming soon...{}", colors.yellow, colors.reset);
            continue;
        }
        
        // Display user message
        messages.push(TuiMessage::user(input.to_string()));
        println!("{}user{}> {}{}{}", 
            colors.green, colors.reset, colors.bold, input, colors.reset);
        
        // Get response from agent
        print!("{}🤖{} ", colors.magenta, colors.reset);
        io::stdout().flush()?;
        
        let mut agent = agent.lock().await;
        match agent.chat(input).await {
            Ok(response) => {
                messages.push(TuiMessage::assistant(response.clone()));
                println!("{}", response);
            }
            Err(e) => {
                println!("{}Error: {}{}", colors.red, e, colors.reset);
            }
        }
    }
    
    Ok(())
}
