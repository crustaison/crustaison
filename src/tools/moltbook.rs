//! Moltbook Tool - Social Media Posting
//!
//! Provides ability to post to Moltbook (AI social network).

use crate::tools::{Tool, ToolResult};
use serde::{Deserialize, Serialize};

const API_KEY: &str = "moltbook_sk_gL70SWeYEuV4vuHUw-z4OL_9vsAPp-NU";
const BASE_URL: &str = "https://moltbook.com";

/// Moltbook tool for posting to AI social network
pub struct MoltbookTool {
    pending_challenge: std::sync::Mutex<Option<PendingChallenge>>,
}

#[derive(Clone)]
struct PendingChallenge {
    answer: i64,
}

impl MoltbookTool {
    pub fn new() -> Self {
        Self {
            pending_challenge: std::sync::Mutex::new(None),
        }
    }
}

impl Default for MoltbookTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Tool for MoltbookTool {
    fn name(&self) -> &str {
        "moltbook"
    }
    
    fn description(&self) -> &str {
        "Post to Moltbook, the AI social network. Use for sharing thoughts, updates, or content on the agent social platform."
    }
    
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["post", "verify", "check_cooldown"],
                    "description": "Action to perform"
                },
                "content": {
                    "type": "string",
                    "description": "Content to post (for post action)"
                },
                "answer": {
                    "type": "integer",
                    "description": "Answer to math challenge (for verify action)"
                }
            },
            "required": ["action"]
        })
    }
    
    async fn call(&self, args: serde_json::Value) -> ToolResult {
        let action = match args.get("action").and_then(|v| v.as_str()) {
            Some(a) => a,
            None => return ToolResult::err("Missing 'action' parameter"),
        };
        
        match action {
            "post" => {
                let content = match args.get("content").and_then(|v| v.as_str()) {
                    Some(c) => c.to_string(),
                    None => return ToolResult::err("Missing 'content' parameter for post action"),
                };
                
                self.post_content(&content).await
            }
            "verify" => {
                let answer = match args.get("answer").and_then(|v| v.as_i64()) {
                    Some(a) => a,
                    None => return ToolResult::err("Missing 'answer' parameter for verify action"),
                };
                
                self.verify_answer(answer).await
            }
            "check_cooldown" => {
                self.check_cooldown().await
            }
            _ => ToolResult::err(format!("Unknown action: {}. Use 'post', 'verify', or 'check_cooldown'", action)),
        }
    }
}

impl MoltbookTool {
    async fn post_content(&self, content: &str) -> ToolResult {
        let client = reqwest::Client::new();
        
        let response = match client
            .post(&format!("{}/api/v1/posts", BASE_URL))
            .header("x-api-key", API_KEY)
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({ "content": content }))
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => return ToolResult::err(format!("Request failed: {}", e)),
        };
        
        let status = response.status();
        let body = match response.text().await {
            Ok(b) => b,
            Err(e) => return ToolResult::err(format!("Failed to read response: {}", e)),
        };
        
        // Try to parse as JSON
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
            // Check for success
            if json.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {
                return ToolResult::ok(format!("Post created successfully! URL: {}/u/Clyde_RYZ", BASE_URL));
            }
            
            // Check for math challenge
            if let Some(challenge) = json.get("challenge").and_then(|v| v.as_str()) {
                // Try to extract the answer from the challenge
                if let Some(answer) = extract_answer_from_challenge(challenge) {
                    // Store the answer for verify step
                    let mut pending = self.pending_challenge.lock().unwrap();
                    *pending = Some(PendingChallenge { answer });
                    
                    return ToolResult {
                        success: true,
                        output: format!(
                            "Math challenge required! Solve this to complete your post:\n\n{}\n\nOnce you have the answer, call moltbook with action='verify' and answer={}",
                            challenge,
                            answer
                        ),
                        error: None,
                        metadata: Some(serde_json::json!({ "pending_answer": answer })),
                    };
                }
                
                return ToolResult::ok(format!(
                    "Math challenge required! Solve: {}\n\nThen call moltbook with action='verify' and the answer.",
                    challenge
                ));
            }
            
            // Check for rate limit
            if let Some(retry_after) = json.get("retry_after_minutes").and_then(|v| v.as_i64()) {
                return ToolResult::err(format!(
                    "Rate limited. Wait {} minutes before posting again.",
                    retry_after
                ));
            }
            
            // Other error
            let error = json.get("error").and_then(|v| v.as_str()).unwrap_or("Unknown error");
            return ToolResult::err(format!("Post failed: {}", error));
        }
        
        ToolResult::err(format!("Unexpected response (status {}): {}", status, &body[..body.len().min(500)]))
    }
    
    async fn verify_answer(&self, answer: i64) -> ToolResult {
        // Check stored answer
        let stored_answer = {
            let pending = self.pending_challenge.lock().unwrap();
            pending.clone()
        };
        
        // If we have a pending challenge, verify the answer matches
        if let Some(challenge) = stored_answer {
            if answer != challenge.answer {
                return ToolResult::err(format!("Incorrect answer. Expected: {}", challenge.answer));
            }
            
            // Clear the pending challenge
            let mut pending = self.pending_challenge.lock().unwrap();
            *pending = None;
        }
        
        let client = reqwest::Client::new();
        
        let response = match client
            .post(&format!("{}/api/v1/verify", BASE_URL))
            .header("x-api-key", API_KEY)
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({ "answer": answer }))
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => return ToolResult::err(format!("Request failed: {}", e)),
        };
        
        let body = match response.text().await {
            Ok(b) => b,
            Err(e) => return ToolResult::err(format!("Failed to read response: {}", e)),
        };
        
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
            if json.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {
                return ToolResult::ok(format!(
                    "Verification successful! Your post is now live.\n\nView it at: {}/u/Clyde_RYZ",
                    BASE_URL
                ));
            }
            
            let error = json.get("error").and_then(|v| v.as_str()).unwrap_or("Verification failed");
            return ToolResult::err(format!("Verification failed: {}", error));
        }
        
        ToolResult::err(format!("Unexpected verify response: {}", &body[..body.len().min(200)]))
    }
    
    async fn check_cooldown(&self) -> ToolResult {
        // Try to post an empty post to check cooldown
        let client = reqwest::Client::new();
        
        let response = match client
            .post(&format!("{}/api/v1/posts", BASE_URL))
            .header("x-api-key", API_KEY)
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({ "content": "cooldown check" }))
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => return ToolResult::err(format!("Request failed: {}", e)),
        };
        
        let body = match response.text().await {
            Ok(b) => b,
            Err(e) => return ToolResult::err(format!("Failed to read response: {}", e)),
        };
        
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
            if let Some(retry_after) = json.get("retry_after_minutes").and_then(|v| v.as_i64()) {
                return ToolResult::ok(format!(
                    "Still in cooldown. Wait {} more minutes.",
                    retry_after
                ));
            }
            
            if json.get("challenge").is_some() {
                return ToolResult::ok("Cooldown complete! You can post now.".to_string());
            }
            
            if json.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {
                return ToolResult::ok("Cooldown complete! You can post now.".to_string());
            }
        }
        
        ToolResult::ok("Unable to determine cooldown status. Try posting.".to_string())
    }
}

/// Extract numeric answer from a lobster-speak math challenge
fn extract_answer_from_challenge(challenge: &str) -> Option<i64> {
    // Look for patterns like "What is X + Y?" or "X + Y = ?"
    let challenge = challenge.to_lowercase();
    
    // Try to find addition
    if let Some(pos) = challenge.find("what is ") {
        let after_what_is = &challenge[pos + 8..];
        
        // Split by common operators
        if let Some(sum_pos) = after_what_is.find('+') {
            let left = &after_what_is[..sum_pos].trim();
            let right = &after_what_is[sum_pos + 1..].trim();
            
            // Handle "X + Y?" or "X + Y ="
            let right_clean: String = right.chars().take_while(|c| c.is_numeric() || c.is_whitespace()).collect();
            let right_clean = right_clean.trim();
            
            if let (Ok(a), Ok(b)) = (left.parse::<i64>(), right_clean.parse::<i64>()) {
                return Some(a + b);
            }
        }
        
        // Try subtraction
        if let Some(sum_pos) = after_what_is.find('-') {
            let left = &after_what_is[..sum_pos].trim();
            let right = &after_what_is[sum_pos + 1..].trim();
            
            let right_clean: String = right.chars().take_while(|c| c.is_numeric() || c.is_whitespace()).collect();
            let right_clean = right_clean.trim();
            
            if let (Ok(a), Ok(b)) = (left.parse::<i64>(), right_clean.parse::<i64>()) {
                return Some(a - b);
            }
        }
        
        // Try multiplication
        if let Some(sum_pos) = after_what_is.find('x') {
            let left = &after_what_is[..sum_pos].trim();
            let right = &after_what_is[sum_pos + 1..].trim();
            
            let right_clean: String = right.chars().take_while(|c| c.is_numeric() || c.is_whitespace()).collect();
            let right_clean = right_clean.trim();
            
            if let (Ok(a), Ok(b)) = (left.parse::<i64>(), right_clean.parse::<i64>()) {
                return Some(a * b);
            }
        }
    }
    
    // Try direct parsing of a number at the end
    let words: Vec<&str> = challenge.split_whitespace().collect();
    for word in words.iter().rev() {
        let cleaned = word.trim_end_matches('?');
        if let Ok(n) = cleaned.parse::<i64>() {
            return Some(n);
        }
    }
    
    None
}
