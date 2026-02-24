//! Email Tool - Send and read emails via Gmail
//!
//! Uses SMTP for sending and IMAP for reading emails.

use crate::tools::tool::{Tool, ToolResult};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Email configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailConfig {
    pub smtp_host: String,
    pub smtp_port: u16,
    pub imap_host: String,
    pub imap_port: u16,
    pub username: String,
    pub password: String,
    pub from_name: String,
}

impl Default for EmailConfig {
    fn default() -> Self {
        Self {
            smtp_host: "smtp.gmail.com".to_string(),
            smtp_port: 587,
            imap_host: "imap.gmail.com".to_string(),
            imap_port: 993,
            username: String::new(),
            password: String::new(),
            from_name: "Crusty".to_string(),
        }
    }
}

pub struct EmailTool {
    config: EmailConfig,
}

impl EmailTool {
    pub fn new(config: EmailConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl Tool for EmailTool {
    fn name(&self) -> &str { "email" }

    fn description(&self) -> &str {
        "Send and read emails. Actions: 'send' (compose and send an email), \
         'read' (check inbox for recent messages with full body text), \
         'search' (search emails by query with full body text). \
         Use this when the user asks you to send an email or check their mail."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["send", "read", "search"],
                    "description": "Action: 'send' to send email, 'read' to check inbox, 'search' to search emails"
                },
                "to": {
                    "type": "string",
                    "description": "Recipient email address (required for send)"
                },
                "subject": {
                    "type": "string",
                    "description": "Email subject (required for send)"
                },
                "body": {
                    "type": "string",
                    "description": "Email body text (required for send)"
                },
                "query": {
                    "type": "string",
                    "description": "Search query (for search action)"
                },
                "count": {
                    "type": "integer",
                    "description": "Number of recent emails to fetch (default 5, max 20)"
                }
            },
            "required": ["action"]
        })
    }

    async fn call(&self, args: serde_json::Value) -> ToolResult {
        if self.config.username.is_empty() || self.config.password.is_empty() {
            return ToolResult::err("Email not configured — missing username or password");
        }

        let action = match args.get("action").and_then(|v| v.as_str()) {
            Some(a) => a,
            None => return ToolResult::err("Missing 'action' parameter"),
        };

        match action {
            "send" => self.send_email(&args).await,
            "read" => self.read_inbox(&args).await,
            "search" => self.search_emails(&args).await,
            _ => ToolResult::err(format!("Unknown action: {}", action)),
        }
    }
}

impl EmailTool {
    async fn send_email(&self, args: &serde_json::Value) -> ToolResult {
        let to = match args.get("to").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => return ToolResult::err("Missing 'to' (recipient email address)"),
        };
        let subject = match args.get("subject").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => return ToolResult::err("Missing 'subject'"),
        };
        let body = match args.get("body").and_then(|v| v.as_str()) {
            Some(b) => b,
            None => return ToolResult::err("Missing 'body'"),
        };

        let from = format!("{} <{}>", self.config.from_name, self.config.username);

        let from_addr: lettre::message::Mailbox = match from.parse() {
            Ok(a) => a,
            Err(e) => return ToolResult::err(format!("Invalid from address: {}", e)),
        };
        let to_addr: lettre::message::Mailbox = match to.parse() {
            Ok(a) => a,
            Err(e) => return ToolResult::err(format!("Invalid to address: {}", e)),
        };

        let email = match lettre::Message::builder()
            .from(from_addr)
            .to(to_addr)
            .subject(subject)
            .body(body.to_string())
        {
            Ok(e) => e,
            Err(e) => return ToolResult::err(format!("Failed to build email: {}", e)),
        };

        let creds = lettre::transport::smtp::authentication::Credentials::new(
            self.config.username.clone(),
            self.config.password.clone(),
        );

        let mailer = match lettre::AsyncSmtpTransport::<lettre::Tokio1Executor>::starttls_relay(&self.config.smtp_host) {
            Ok(builder) => builder
                .credentials(creds)
                .port(self.config.smtp_port)
                .build(),
            Err(e) => return ToolResult::err(format!("SMTP connection failed: {}", e)),
        };

        use lettre::AsyncTransport;
        match mailer.send(email).await {
            Ok(_) => ToolResult::ok(format!("Email sent to {} — Subject: {}", to, subject)),
            Err(e) => ToolResult::err(format!("Failed to send email: {}", e)),
        }
    }

    async fn read_inbox(&self, args: &serde_json::Value) -> ToolResult {
        let count = args.get("count").and_then(|v| v.as_u64()).unwrap_or(5).min(20) as usize;

        let host = self.config.imap_host.clone();
        let port = self.config.imap_port;
        let username = self.config.username.clone();
        let password = self.config.password.clone();

        let result = tokio::task::spawn_blocking(move || {
            read_imap_inbox(&host, port, &username, &password, count)
        }).await;

        match result {
            Ok(Ok(messages)) => {
                if messages.is_empty() {
                    ToolResult::ok("Inbox is empty — no recent messages.")
                } else {
                    ToolResult::ok(format!("Recent emails ({}):\n\n{}", messages.len(), messages.join("\n\n===\n\n")))
                }
            }
            Ok(Err(e)) => ToolResult::err(format!("IMAP error: {}", e)),
            Err(e) => ToolResult::err(format!("Task error: {}", e)),
        }
    }

    async fn search_emails(&self, args: &serde_json::Value) -> ToolResult {
        let query = match args.get("query").and_then(|v| v.as_str()) {
            Some(q) => q.to_string(),
            None => return ToolResult::err("Missing 'query' for search"),
        };
        let count = args.get("count").and_then(|v| v.as_u64()).unwrap_or(5).min(20) as usize;

        let host = self.config.imap_host.clone();
        let port = self.config.imap_port;
        let username = self.config.username.clone();
        let password = self.config.password.clone();

        let result = tokio::task::spawn_blocking(move || {
            search_imap_emails(&host, port, &username, &password, &query, count)
        }).await;

        match result {
            Ok(Ok(messages)) => {
                if messages.is_empty() {
                    ToolResult::ok(format!("No emails found matching: {}", args.get("query").and_then(|v| v.as_str()).unwrap_or("")))
                } else {
                    ToolResult::ok(format!("Search results ({}):\n\n{}", messages.len(), messages.join("\n\n===\n\n")))
                }
            }
            Ok(Err(e)) => ToolResult::err(format!("IMAP search error: {}", e)),
            Err(e) => ToolResult::err(format!("Task error: {}", e)),
        }
    }
}

/// Extract plain text body from a raw email message
fn extract_body_text(raw: &[u8]) -> String {
    let raw_str = String::from_utf8_lossy(raw);

    // Split headers from body at the first blank line
    let parts: Vec<&str> = raw_str.splitn(2, "\r\n\r\n").collect();
    if parts.len() < 2 {
        // Try with just \n\n
        let parts2: Vec<&str> = raw_str.splitn(2, "\n\n").collect();
        if parts2.len() < 2 {
            return String::new();
        }
        return clean_body_text(parts2[1]);
    }

    let headers = parts[0].to_lowercase();
    let body = parts[1];

    // Check if it's a multipart message
    if headers.contains("multipart") {
        // Find boundary
        if let Some(boundary) = extract_boundary(&headers) {
            return extract_text_from_multipart(body, &boundary);
        }
    }

    // Check content-transfer-encoding
    if headers.contains("base64") {
        // Try to decode base64
        let cleaned: String = body.chars().filter(|c| !c.is_whitespace()).collect();
        if let Ok(decoded) = base64_decode(&cleaned) {
            return clean_body_text(&decoded);
        }
    }

    if headers.contains("quoted-printable") {
        return clean_body_text(&decode_quoted_printable(body));
    }

    clean_body_text(body)
}

/// Extract boundary string from Content-Type header
fn extract_boundary(headers: &str) -> Option<String> {
    for line in headers.lines() {
        if let Some(pos) = line.find("boundary=") {
            let boundary = line[pos + 9..].trim();
            let boundary = boundary.trim_matches('"').trim_matches('\'');
            return Some(boundary.to_string());
        }
    }
    None
}

/// Extract text/plain part from multipart message
fn extract_text_from_multipart(body: &str, boundary: &str) -> String {
    let delimiter = format!("--{}", boundary);
    let parts: Vec<&str> = body.split(&delimiter).collect();

    for part in &parts {
        let lower = part.to_lowercase();
        // Prefer text/plain
        if lower.contains("content-type: text/plain") || lower.contains("content-type:text/plain") {
            // Split this part's headers from body
            let sub_parts: Vec<&str> = part.splitn(2, "\r\n\r\n").collect();
            let sub_body = if sub_parts.len() >= 2 {
                sub_parts[1]
            } else {
                let sub_parts2: Vec<&str> = part.splitn(2, "\n\n").collect();
                if sub_parts2.len() >= 2 { sub_parts2[1] } else { continue; }
            };

            let sub_headers = sub_parts.get(0).unwrap_or(&"").to_lowercase();

            if sub_headers.contains("base64") {
                let cleaned: String = sub_body.chars().filter(|c| !c.is_whitespace()).collect();
                if let Ok(decoded) = base64_decode(&cleaned) {
                    return clean_body_text(&decoded);
                }
            }
            if sub_headers.contains("quoted-printable") {
                return clean_body_text(&decode_quoted_printable(sub_body));
            }
            return clean_body_text(sub_body);
        }
    }

    // Fallback: try text/html and strip tags
    for part in &parts {
        let lower = part.to_lowercase();
        if lower.contains("content-type: text/html") || lower.contains("content-type:text/html") {
            let sub_parts: Vec<&str> = part.splitn(2, "\r\n\r\n").collect();
            let sub_body = if sub_parts.len() >= 2 {
                sub_parts[1]
            } else {
                let sub_parts2: Vec<&str> = part.splitn(2, "\n\n").collect();
                if sub_parts2.len() >= 2 { sub_parts2[1] } else { continue; }
            };

            let sub_headers = sub_parts.get(0).unwrap_or(&"").to_lowercase();
            let html = if sub_headers.contains("base64") {
                let cleaned: String = sub_body.chars().filter(|c| !c.is_whitespace()).collect();
                base64_decode(&cleaned).unwrap_or_else(|_| sub_body.to_string())
            } else if sub_headers.contains("quoted-printable") {
                decode_quoted_printable(sub_body)
            } else {
                sub_body.to_string()
            };
            return strip_html_tags(&html);
        }
    }

    // Last resort: return first non-empty part
    for part in &parts {
        let trimmed = part.trim();
        if !trimmed.is_empty() && !trimmed.starts_with("--") && trimmed.len() > 10 {
            return clean_body_text(trimmed);
        }
    }

    String::new()
}

/// Simple base64 decode
fn base64_decode(input: &str) -> Result<String, String> {
    // Standard base64 alphabet
    let alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut bytes = Vec::new();
    let mut buffer: u32 = 0;
    let mut bits = 0;

    for ch in input.chars() {
        if ch == '=' { break; }
        if let Some(val) = alphabet.find(ch) {
            buffer = (buffer << 6) | val as u32;
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                bytes.push((buffer >> bits) as u8);
                buffer &= (1 << bits) - 1;
            }
        }
    }

    String::from_utf8(bytes).map_err(|e| format!("UTF-8 decode error: {}", e))
}

/// Decode quoted-printable encoding
fn decode_quoted_printable(input: &str) -> String {
    let mut result = String::new();
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '=' {
            // Check for soft line break
            match chars.peek() {
                Some('\r') => { chars.next(); if chars.peek() == Some(&'\n') { chars.next(); } }
                Some('\n') => { chars.next(); }
                Some(_) => {
                    let h1 = chars.next().unwrap_or('0');
                    let h2 = chars.next().unwrap_or('0');
                    let hex = format!("{}{}", h1, h2);
                    if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                        result.push(byte as char);
                    } else {
                        result.push('=');
                        result.push(h1);
                        result.push(h2);
                    }
                }
                None => { result.push('='); }
            }
        } else {
            result.push(ch);
        }
    }

    result
}

/// Strip HTML tags for fallback text extraction
fn strip_html_tags(html: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;
    let mut last_was_space = false;

    for ch in html.chars() {
        if ch == '<' {
            in_tag = true;
        } else if ch == '>' {
            in_tag = false;
        } else if !in_tag {
            if ch.is_whitespace() {
                if !last_was_space {
                    result.push(' ');
                    last_was_space = true;
                }
            } else {
                result.push(ch);
                last_was_space = false;
            }
        }
    }

    // Decode common HTML entities
    result
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
        .trim()
        .to_string()
}

/// Clean up body text
fn clean_body_text(text: &str) -> String {
    let trimmed = text.trim();
    // Truncate very long emails
    if trimmed.len() > 3000 {
        format!("{}...\n[truncated]", &trimmed[..3000])
    } else {
        trimmed.to_string()
    }
}

/// Parse headers into a clean format
fn parse_headers(raw: &[u8]) -> (String, String, String) {
    let text = String::from_utf8_lossy(raw);
    let mut from = String::new();
    let mut subject = String::new();
    let mut date = String::new();

    for line in text.lines() {
        let lower = line.to_lowercase();
        if lower.starts_with("from:") {
            from = line[5..].trim().to_string();
        } else if lower.starts_with("subject:") {
            subject = line[8..].trim().to_string();
        } else if lower.starts_with("date:") {
            date = line[5..].trim().to_string();
        }
    }

    (from, subject, date)
}

/// Read recent emails via IMAP (blocking) — fetches headers AND body
fn read_imap_inbox(host: &str, port: u16, username: &str, password: &str, count: usize) -> Result<Vec<String>, String> {
    let tls = native_tls::TlsConnector::builder()
        .build()
        .map_err(|e| format!("TLS error: {}", e))?;

    let client = imap::connect((host, port), host, &tls)
        .map_err(|e| format!("IMAP connect failed: {}", e))?;

    let mut session = client.login(username, password)
        .map_err(|e| format!("IMAP login failed: {}", e.0))?;

    session.select("INBOX")
        .map_err(|e| format!("Failed to select INBOX: {}", e))?;

    let mailbox = session.search("ALL")
        .map_err(|e| format!("Search failed: {}", e))?;

    let mut seq_nums: Vec<u32> = mailbox.into_iter().collect();
    seq_nums.sort();
    seq_nums.reverse();
    seq_nums.truncate(count);

    if seq_nums.is_empty() {
        let _ = session.logout();
        return Ok(Vec::new());
    }

    let seq_set = seq_nums.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(",");

    // Fetch headers
    let headers = session.fetch(&seq_set, "BODY.PEEK[HEADER.FIELDS (FROM SUBJECT DATE)]")
        .map_err(|e| format!("Header fetch failed: {}", e))?;

    // Fetch full body
    let bodies = session.fetch(&seq_set, "BODY.PEEK[]")
        .map_err(|e| format!("Body fetch failed: {}", e))?;

    let mut results = Vec::new();

    // Build a map of seq -> header info
    let mut header_map = std::collections::HashMap::new();
    for msg in headers.iter() {
        if let Some(h) = msg.header() {
            let (from, subject, date) = parse_headers(h);
            header_map.insert(msg.message, (from, subject, date));
        }
    }

    for msg in bodies.iter() {
        let (from, subject, date) = header_map.get(&msg.message)
            .cloned()
            .unwrap_or_else(|| ("Unknown".to_string(), "No Subject".to_string(), "".to_string()));

        let body_text = if let Some(body) = msg.body() {
            extract_body_text(body)
        } else {
            "[No body content]".to_string()
        };

        let formatted = format!(
            "From: {}\nSubject: {}\nDate: {}\n\n[UNTRUSTED_EXTERNAL_CONTENT]\n{}\n[/UNTRUSTED_EXTERNAL_CONTENT]",
            from, subject, date, body_text
        );
        results.push(formatted);
    }

    let _ = session.logout();
    Ok(results)
}

/// Search emails via IMAP (blocking) — fetches headers AND body
fn search_imap_emails(host: &str, port: u16, username: &str, password: &str, query: &str, count: usize) -> Result<Vec<String>, String> {
    let tls = native_tls::TlsConnector::builder()
        .build()
        .map_err(|e| format!("TLS error: {}", e))?;

    let client = imap::connect((host, port), host, &tls)
        .map_err(|e| format!("IMAP connect failed: {}", e))?;

    let mut session = client.login(username, password)
        .map_err(|e| format!("IMAP login failed: {}", e.0))?;

    session.select("INBOX")
        .map_err(|e| format!("Failed to select INBOX: {}", e))?;

    let search_query = format!("OR SUBJECT \"{}\" FROM \"{}\"", query, query);
    let results = session.search(&search_query)
        .map_err(|e| format!("Search failed: {}", e))?;

    let mut seq_nums: Vec<u32> = results.into_iter().collect();
    seq_nums.sort();
    seq_nums.reverse();
    seq_nums.truncate(count);

    if seq_nums.is_empty() {
        let _ = session.logout();
        return Ok(Vec::new());
    }

    let seq_set = seq_nums.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(",");

    let headers = session.fetch(&seq_set, "BODY.PEEK[HEADER.FIELDS (FROM SUBJECT DATE)]")
        .map_err(|e| format!("Header fetch failed: {}", e))?;

    let bodies = session.fetch(&seq_set, "BODY.PEEK[]")
        .map_err(|e| format!("Body fetch failed: {}", e))?;

    let mut email_results = Vec::new();

    let mut header_map = std::collections::HashMap::new();
    for msg in headers.iter() {
        if let Some(h) = msg.header() {
            let (from, subject, date) = parse_headers(h);
            header_map.insert(msg.message, (from, subject, date));
        }
    }

    for msg in bodies.iter() {
        let (from, subject, date) = header_map.get(&msg.message)
            .cloned()
            .unwrap_or_else(|| ("Unknown".to_string(), "No Subject".to_string(), "".to_string()));

        let body_text = if let Some(body) = msg.body() {
            extract_body_text(body)
        } else {
            "[No body content]".to_string()
        };

        let formatted = format!(
            "From: {}\nSubject: {}\nDate: {}\n\n[UNTRUSTED_EXTERNAL_CONTENT]\n{}\n[/UNTRUSTED_EXTERNAL_CONTENT]",
            from, subject, date, body_text
        );
        email_results.push(formatted);
    }

    let _ = session.logout();
    Ok(email_results)
}
