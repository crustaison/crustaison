//! Heartbeat System - Nexa-Powered Monitoring + Task Scheduling
//!
//! Uses local Nexa provider for free AI analysis of system health checks.
//! Alerts Telegram only when Nexa determines attention is needed.
//! Also processes scheduled tasks from the task queue.

use crate::providers::nexa::NexaProvider;
use crate::providers::provider::{Provider, ChatMessage};
use crate::runtime::checks::{Check, CheckResult, CheckStatus, default_checks};
use crate::runtime::scheduler::{TaskQueue, ScheduledTask, TaskAction};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};
use tokio_util::sync::CancellationToken;

/// Simple heartbeat stub for backward compatibility
#[derive(Debug, Clone)]
pub struct Heartbeat {
    pub config_path: PathBuf,
}

impl Heartbeat {
    pub fn new(path: PathBuf) -> Self {
        Self { config_path: path }
    }
}

/// Heartbeat configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatConfig {
    pub enabled: bool,
    pub interval_secs: u64,
    pub alert_cooldown_secs: u64,
    pub nexa_host: String,
    pub nexa_port: u16,
    pub nexa_model: String,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_secs: 300,
            alert_cooldown_secs: 3600,
            nexa_host: "localhost".to_string(),
            nexa_port: 18181,
            nexa_model: "unsloth/Qwen3-1.7B-GGUF:Q4_0".to_string(),
        }
    }
}

/// Heartbeat runner with Nexa analysis and task scheduling
pub struct HeartbeatRunner {
    config: HeartbeatConfig,
    nexa: Option<NexaProvider>,
    checks: Vec<Box<dyn Check>>,
    alert_tx: mpsc::Sender<String>,
    last_alerts: HashMap<String, Instant>,
    cancellation: CancellationToken,
    // Task scheduling fields
    task_queue: Option<Arc<TaskQueue>>,
    bot_token: Option<String>,
    // Email checking fields
    email_config: Option<EmailCheckConfig>,
    last_seen_email_count: Option<u32>,
    default_chat_id: i64,
}

/// Email check configuration (IMAP credentials)
#[derive(Debug, Clone)]
pub struct EmailCheckConfig {
    pub imap_host: String,
    pub imap_port: u16,
    pub username: String,
    pub password: String,
}

impl HeartbeatRunner {
    /// Create new heartbeat runner
    pub fn new(
        config: HeartbeatConfig,
        nexa: Option<NexaProvider>,
        alert_tx: mpsc::Sender<String>,
    ) -> Self {
        Self {
            config,
            nexa,
            checks: default_checks(),
            alert_tx,
            last_alerts: HashMap::new(),
            cancellation: CancellationToken::new(),
            task_queue: None,
            bot_token: None,
            email_config: None,
            last_seen_email_count: None,
            default_chat_id: 0,
        }
    }
    
    /// Set the task queue for scheduled tasks
    pub fn set_task_queue(&mut self, queue: Arc<TaskQueue>) {
        self.task_queue = Some(queue);
    }
    
    /// Set the Telegram bot token for sending task results
    pub fn set_bot_token(&mut self, token: String) {
        self.bot_token = Some(token);
    }

    /// Set email config for inbox checking
    pub fn set_email_config(&mut self, config: EmailCheckConfig) {
        self.email_config = Some(config);
    }

    /// Set default chat ID for notifications
    pub fn set_default_chat_id(&mut self, chat_id: i64) {
        self.default_chat_id = chat_id;
    }
    
    /// Add a custom check
    pub fn add_check(&mut self, check: Box<dyn Check>) {
        self.checks.push(check);
    }
    
    /// Start the heartbeat loop
    pub async fn start(&mut self) {
        if !self.config.enabled {
            return;
        }
        
        let mut interval = interval(Duration::from_secs(self.config.interval_secs));
        
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    self.run_cycle().await;
                }
                _ = self.cancellation.cancelled() => {
                    break;
                }
            }
        }
    }
    
    /// Stop the heartbeat
    pub fn stop(&self) {
        self.cancellation.cancel();
    }
    
    /// Run a single check cycle
    pub async fn run_cycle(&mut self) {
        // 1. Run health checks
        let results = self.run_checks().await;
        let should_alert = self.analyze_with_nexa(&results).await;
        
        if should_alert {
            let message = self.format_alert(&results);
            self.send_alert(&message).await;
        }
        
        // 2. Process scheduled tasks
        self.process_scheduled_tasks().await;

        // 3. Check email inbox
        self.check_email_inbox().await;
    }
    
    /// Check email inbox for new messages
    async fn check_email_inbox(&mut self) {
        let email_config = match &self.email_config {
            Some(c) => c.clone(),
            None => return,
        };

        let host = email_config.imap_host.clone();
        let port = email_config.imap_port;
        let username = email_config.username.clone();
        let password = email_config.password.clone();

        let result = tokio::task::spawn_blocking(move || {
            check_inbox_for_new(&host, port, &username, &password)
        }).await;

        match result {
            Ok(Ok((count, new_emails))) => {
                let prev_count = self.last_seen_email_count.unwrap_or(count);
                self.last_seen_email_count = Some(count);

                if count > prev_count && !new_emails.is_empty() {
                    let new_count = count - prev_count;
                    let msg = format!(
                        "📧 {} new email{}\n\n{}",
                        new_count,
                        if new_count == 1 { "" } else { "s" },
                        new_emails.join("\n---\n")
                    );
                    let chat_id = self.default_chat_id;
                    if chat_id != 0 {
                        self.send_to_chat(chat_id, &msg).await;
                    }
                    tracing::info!("Email: {} new message(s) detected", new_count);
                } else {
                    tracing::debug!("Email: no new messages (total: {})", count);
                }
            }
            Ok(Err(e)) => {
                tracing::warn!("Email check failed: {}", e);
            }
            Err(e) => {
                tracing::warn!("Email check task failed: {}", e);
            }
        }
    }

    /// Process due scheduled tasks
    async fn process_scheduled_tasks(&mut self) {
        if let Some(ref queue) = self.task_queue {
            let due_tasks = queue.get_due_tasks().await;
            
            for task in due_tasks {
                tracing::info!("Executing scheduled task: {}", task.description);
                
                // Mark as running
                let _ = queue.start_task(&task.id).await;
                
                // Execute the task
                let result = self.execute_scheduled_task(&task).await;
                
                match result {
                    Ok(output) => {
                        let _ = queue.complete_task(&task.id, output.clone()).await;
                        // Send result to Telegram
                        let msg = format!("⏰ Scheduled Task Complete\n\n📋 {}\n\n{}", task.description, output);
                        self.send_to_chat(task.chat_id, &msg).await;
                    }
                    Err(e) => {
                        let _ = queue.fail_task(&task.id, e.to_string()).await;
                        let msg = format!("⏰ Scheduled Task Failed\n\n📋 {}\n\n❌ {}", task.description, e);
                        self.send_to_chat(task.chat_id, &msg).await;
                    }
                }
            }
            
            // Cleanup old tasks periodically
            let _ = queue.cleanup().await;
        }
    }
    
    /// Execute a scheduled task based on its action type
    async fn execute_scheduled_task(&self, task: &ScheduledTask) -> Result<String, String> {
        match &task.action {
            TaskAction::Weather { location } => {
                let url = format!("https://wttr.in/{}?format=%l:+%C+%t+%w+%h+humidity", urlencoding::encode(location));
                let client = reqwest::Client::new();
                match client.get(&url)
                    .header("User-Agent", "curl/7.0")
                    .send()
                    .await
                {
                    Ok(resp) => {
                        let text = resp.text().await.map_err(|e| e.to_string())?;
                        if text.contains("Unknown location") || text.contains("Sorry") || text.is_empty() {
                            Err(format!("Could not find weather for: {}", location))
                        } else {
                            Ok(format!("Weather for {}:
{}", location, text.trim()))
                        }
                    }
                    Err(e) => Err(format!("Weather fetch failed: {}", e)),
                }
            }
            TaskAction::Command { command } => {
                let output = tokio::process::Command::new("bash")
                    .arg("-c")
                    .arg(command)
                    .output()
                    .await
                    .map_err(|e| e.to_string())?;
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                if output.status.success() {
                    Ok(stdout.to_string())
                } else {
                    Err(format!("{}\n{}", stdout, stderr))
                }
            }
            TaskAction::Reminder { message } => {
                Ok(format!("🔔 Reminder: {}", message))
            }
            TaskAction::WebFetch { url } => {
                let client = reqwest::Client::new();
                match client.get(url).send().await {
                    Ok(resp) => {
                        let text = resp.text().await.map_err(|e| e.to_string())?;
                        Ok(text.chars().take(2000).collect())
                    }
                    Err(e) => Err(format!("Fetch failed: {}", e)),
                }
            }
            TaskAction::Custom { prompt } => {
                if let Some(ref nexa) = self.nexa {
                    let messages = vec![ChatMessage {
                        role: "user".to_string(),
                        content: prompt.clone(),
                        images: Vec::new(),
                    }];
                    match nexa.chat(messages, None).await {
                        Ok(response) => Ok(response.content),
                        Err(e) => Err(format!("Nexa error: {}", e)),
                    }
                } else {
                    Ok(format!("Custom task: {}", prompt))
                }
            }
        }
    }
    
    /// Send a message to a specific Telegram chat
    async fn send_to_chat(&self, chat_id: i64, message: &str) {
        if chat_id == 0 {
            // No chat configured, send to alert channel instead
            let _ = self.alert_tx.send(message.to_string()).await;
            return;
        }
        
        if let Some(ref token) = self.bot_token {
            let url = format!("https://api.telegram.org/bot{}/sendMessage", token);
            let client = reqwest::Client::new();
            let _ = client.post(&url)
                .json(&serde_json::json!({
                    "chat_id": chat_id,
                    "text": message,
                }))
                .send()
                .await;
        } else {
            // Fallback: use alert channel
            let _ = self.alert_tx.send(message.to_string()).await;
        }
    }
    
    /// Run all check functions
    async fn run_checks(&self) -> Vec<CheckResult> {
        let mut results = Vec::new();
        
        for check in &self.checks {
            results.push(check.run().await);
        }
        
        results
    }
    
    /// Analyze check results with Nexa (free, local)
    async fn analyze_with_nexa(&self, results: &[CheckResult]) -> bool {
        // Build summary for Nexa
        let summary: Vec<String> = results.iter()
            .map(|r| format!("- {}: {} ({})", r.name, r.message, serde_json::to_string(&r.status).unwrap_or_default()))
            .collect();
        
        let prompt = format!(
            r#"You are a system monitoring assistant. Analyze these health check results and determine if anything needs human attention.

{}

Respond with EXACTLY one of:
1. "ALL_CLEAR" if everything looks normal
2. "ALERT: <brief description>" if something needs attention"#,
            summary.join("\n")
        );
        
        if let Some(ref nexa) = self.nexa {
            // Use local Nexa for free analysis
            let messages = vec![ChatMessage {
                role: "user".to_string(),
                content: format!("/no_think {}", prompt),
                images: Vec::new(),
            }];
            
            match nexa.chat(messages, None).await {
                Ok(response) => {
                    // Strip <think>...</think> blocks before parsing
                    let raw = response.content;
                    let text = strip_think_tags(&raw);
                    // Return true if alert needed
                    text.contains("ALERT:")
                }
                Err(_) => false,
            }
        } else {
            // Fallback: alert on any Warning or Critical
            results.iter().any(|r| r.status != CheckStatus::Ok)
        }
    }
    
    /// Format alert message
    fn format_alert(&self, results: &[CheckResult]) -> String {
        let issues: Vec<String> = results.iter()
            .filter(|r| r.status != CheckStatus::Ok)
            .map(|r| format!("• {}: {}", r.name, r.message))
            .collect();
        
        if issues.is_empty() {
            "System check completed".to_string()
        } else {
            format!("Heartbeat Alert\n\n{}", issues.join("\n"))
        }
    }
    
    /// Send alert to Telegram
    async fn send_alert(&mut self, message: &str) {
        // Rate limit: don't repeat same alert within cooldown
        let key = message.to_string();
        let now = Instant::now();
        
        if let Some(last_time) = self.last_alerts.get(&key) {
            if now.duration_since(*last_time) < Duration::from_secs(self.config.alert_cooldown_secs) {
                return; // Still in cooldown
            }
        }
        
        self.last_alerts.insert(key, now);
        
        // Send to Telegram channel
        let _ = self.alert_tx.send(message.to_string()).await;
    }
    
    /// Get status for /heartbeat command
    pub async fn get_status(&self) -> HeartbeatStatus {
        let results = self.run_checks().await;
        
        HeartbeatStatus {
            checks: results,
            last_run: chrono::Utc::now(),
            next_run: chrono::Utc::now() + chrono::Duration::seconds(self.config.interval_secs as i64),
            config: self.config.clone(),
        }
    }
}

/// Status response for /heartbeat command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatStatus {
    pub checks: Vec<CheckResult>,
    pub last_run: chrono::DateTime<chrono::Utc>,
    pub next_run: chrono::DateTime<chrono::Utc>,
    pub config: HeartbeatConfig,
}

/// Format status for display
impl std::fmt::Display for HeartbeatStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "=== Heartbeat Status ===")?;
        writeln!(f, "Last check: {}", self.last_run)?;
        writeln!(f, "Next check: {}", self.next_run)?;
        writeln!(f, "Interval: {}s", self.config.interval_secs)?;
        writeln!(f, "Alert cooldown: {}s", self.config.alert_cooldown_secs)?;
        writeln!(f)?;
        
        for check in &self.checks {
            let icon = match check.status {
                CheckStatus::Ok => "✅",
                CheckStatus::Warning => "⚠️",
                CheckStatus::Critical => "🚨",
            };
            writeln!(f, "{} {} - {}", icon, check.name, check.message)?;
        }
        
        Ok(())
    }
}

/// Check inbox via IMAP and return (total_count, new_email_summaries)
fn check_inbox_for_new(host: &str, port: u16, username: &str, password: &str) -> Result<(u32, Vec<String>), String> {
    let tls = native_tls::TlsConnector::builder()
        .build()
        .map_err(|e| format!("TLS error: {}", e))?;

    let client = imap::connect((host, port), host, &tls)
        .map_err(|e| format!("IMAP connect failed: {}", e))?;

    let mut session = client.login(username, password)
        .map_err(|e| format!("IMAP login failed: {}", e.0))?;

    let mailbox = session.select("INBOX")
        .map_err(|e| format!("Failed to select INBOX: {}", e))?;

    let total = mailbox.exists;

    // Fetch the 5 most recent message headers
    let mut summaries = Vec::new();
    if total > 0 {
        let start = if total > 5 { total - 4 } else { 1 };
        let range = format!("{}:{}", start, total);

        if let Ok(messages) = session.fetch(&range, "BODY.PEEK[HEADER.FIELDS (FROM SUBJECT DATE)]") {
            for msg in messages.iter() {
                if let Some(header) = msg.header() {
                    let text = String::from_utf8_lossy(header).to_string();
                    // Parse into a clean summary
                    let mut from = String::new();
                    let mut subject = String::new();
                    for line in text.lines() {
                        let lower = line.to_lowercase();
                        if lower.starts_with("from:") {
                            from = line[5..].trim().to_string();
                        } else if lower.starts_with("subject:") {
                            subject = line[8..].trim().to_string();
                        }
                    }
                    if !from.is_empty() || !subject.is_empty() {
                        summaries.push(format!("From: {}\nSubject: {}", from, subject));
                    }
                }
            }
        }
    }

    let _ = session.logout();
    Ok((total, summaries))
}

/// Strip <think>...</think> blocks from Qwen3 extended thinking output.
fn strip_think_tags(text: &str) -> String {
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
