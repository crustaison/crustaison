//! Destructive-operation consensus guard.
//!
//! Subscribes to `PreToolUse`. When the tool call matches a destructive
//! pattern (exec with `rm -rf`, force-push, schema drops, service kills,
//! etc.), asks TWO separate models — the router (fast, local) and MiniMax
//! (slow, capable) — whether to proceed. Only a concurring "YES" from both
//! models lets the action through; any NO or timeout blocks it.
//!
//! This is the `dual-LLM consensus on destructive tool calls` item from the
//! Crustaison plan. It's also the first listener that actually uses
//! `AntennaOutcome::Block` — the variants are no longer dead code.
//!
//! Rationale: the carapace (permission system) is a blunt allow/deny. The
//! guard adds a semantic layer that reasons about *intent* on a per-command
//! basis. It's an extra ~2s latency on maybe 5% of commands, zero latency
//! on the other 95%.

use std::time::Duration;

use crate::antennae::{AntennaListener, AntennaOutcome, AntennaSignal};

const ROUTER_URL: &str = "http://localhost:18181/v1/chat/completions";
const ROUTER_MODEL: &str = "unsloth/Qwen3-1.7B-GGUF:Q4_0";
const MINIMAX_URL: &str = "https://api.minimax.io/anthropic/chat/completions";
const MINIMAX_MODEL: &str = "MiniMax-M2.1";
const TIMEOUT_SECS: u64 = 20;

/// Substrings that mark a shell command as destructive enough to need
/// two-model sign-off. Curated conservatively — false positives (extra
/// guard calls) are cheap; false negatives (allowing a rogue `rm -rf /`)
/// are not.
const DESTRUCTIVE_PATTERNS: &[&str] = &[
    "rm -rf",
    "rm -fr",
    ":(){ :|:",               // fork bomb
    "dd if=",                  // raw device writes
    "mkfs",                    // filesystem wipes
    "> /dev/sda",
    "chmod -R 000",
    "chown -R",
    "git push --force",
    "git push -f",
    "git reset --hard origin", // destroys local commits
    "git clean -fd",
    "git branch -D",
    "DROP TABLE",
    "DROP DATABASE",
    "TRUNCATE TABLE",
    "DELETE FROM ",            // unbounded deletes
    "systemctl stop",
    "systemctl disable",
    "pkill -9",
    "killall -9",
    "shutdown",
    "reboot",
    "halt",
    "poweroff",
    "userdel",
    "groupdel",
    "iptables -F",
    "ufw reset",
];

pub struct DestructiveGuardListener {
    client: reqwest::Client,
    minimax_api_key: Option<String>,
}

impl DestructiveGuardListener {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(TIMEOUT_SECS))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        // Reuse the same env var the rest of crustaison uses; main.rs can
        // also pass the key through explicitly via with_api_key().
        let minimax_api_key = std::env::var("CRUSTAISON_API_KEY").ok();
        Self { client, minimax_api_key }
    }

    /// Override the MiniMax API key after construction (e.g. from config.toml
    /// rather than env var).
    pub fn with_api_key(mut self, key: String) -> Self {
        self.minimax_api_key = Some(key);
        self
    }

    fn is_destructive(tool: &str, args: &serde_json::Value) -> bool {
        // Currently only exec commands are pattern-matched. Extending to
        // other tools (e.g. files delete_all) is additive.
        if tool != "exec" {
            return false;
        }
        let Some(cmd) = args.get("command").and_then(|v| v.as_str()) else {
            return false;
        };
        let lc = cmd.to_lowercase();
        DESTRUCTIVE_PATTERNS.iter().any(|p| lc.contains(&p.to_lowercase()))
    }

    async fn ask_router(client: &reqwest::Client, cmd: &str) -> Option<bool> {
        let prompt = format!(
            "/no_think A Linux command is about to run on Sean's production machine.\n\
             Answer YES if it looks like a routine sysadmin action Sean would approve.\n\
             Answer NO if it looks destructive, irreversible, or out-of-character.\n\
             Respond with ONE word: YES or NO.\n\n\
             Command: {}\n\nAnswer:",
            cmd
        );
        let body = serde_json::json!({
            "model": ROUTER_MODEL,
            "messages": [{"role": "user", "content": prompt}],
            "max_tokens": 8,
            "temperature": 0.0,
        });
        let resp = client.post(ROUTER_URL).json(&body).send().await.ok()?;
        let v: serde_json::Value = resp.json().await.ok()?;
        let content = v["choices"][0]["message"]["content"].as_str()?;
        Some(parse_yes(content))
    }

    async fn ask_minimax(&self, cmd: &str) -> Option<bool> {
        let key = self.minimax_api_key.as_ref()?;
        let prompt = format!(
            "A Linux command is about to run on Sean's production machine.\n\
             Answer YES if it looks like a routine sysadmin action Sean would approve.\n\
             Answer NO if it looks destructive, irreversible, or out-of-character.\n\
             Respond with ONE word: YES or NO.\n\n\
             Command: {}\n\nAnswer:",
            cmd
        );
        // MiniMax exposes an OpenAI-compatible /chat/completions endpoint
        // under its /anthropic prefix; auth is Bearer token.
        let body = serde_json::json!({
            "model": MINIMAX_MODEL,
            "max_tokens": 8,
            "messages": [{"role": "user", "content": prompt}],
            "stream": false,
        });
        let resp = self.client
            .post(MINIMAX_URL)
            .header("Authorization", format!("Bearer {}", key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .ok()?;
        let v: serde_json::Value = resp.json().await.ok()?;
        let content = v["choices"][0]["message"]["content"].as_str()?;
        Some(parse_yes(content))
    }
}

impl Default for DestructiveGuardListener {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl AntennaListener for DestructiveGuardListener {
    fn name(&self) -> &str {
        "destructive_guard"
    }

    async fn receive(&self, signal: &AntennaSignal) -> AntennaOutcome {
        let AntennaSignal::PreToolUse { tool, args } = signal else {
            return AntennaOutcome::Continue;
        };

        if !Self::is_destructive(tool, args) {
            return AntennaOutcome::Continue;
        }

        let cmd = args
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        tracing::warn!(
            "destructive_guard: flagged '{}' — requesting dual-LLM consensus",
            &cmd[..cmd.len().min(120)]
        );

        // Query both models concurrently.
        let router_fut = Self::ask_router(&self.client, &cmd);
        let minimax_fut = self.ask_minimax(&cmd);
        let (router_yes, minimax_yes) = tokio::join!(router_fut, minimax_fut);

        match (router_yes, minimax_yes) {
            (Some(true), Some(true)) => {
                tracing::info!("destructive_guard: both models approved");
                AntennaOutcome::Continue
            }
            (Some(false), _) | (_, Some(false)) => {
                let who = if router_yes == Some(false) { "router" } else { "minimax" };
                tracing::warn!("destructive_guard: {} refused; blocking", who);
                AntennaOutcome::Block(format!(
                    "destructive command rejected by {} consensus check",
                    who
                ))
            }
            (None, None) => {
                // Neither responded — fail closed. Better to ask Sean than to
                // proceed without consensus.
                tracing::warn!("destructive_guard: both consensus calls failed; blocking");
                AntennaOutcome::Block(
                    "destructive command: consensus unavailable (both models timed out)".into(),
                )
            }
            _ => {
                // Exactly one responded and said yes — require both.
                tracing::warn!("destructive_guard: only one model responded; blocking");
                AntennaOutcome::Block("destructive command: only partial consensus".into())
            }
        }
    }
}

fn parse_yes(s: &str) -> bool {
    let t = s.trim().to_uppercase();
    t.starts_with("YES")
}
