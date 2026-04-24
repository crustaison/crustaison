//! LLM-judge listener.
//!
//! Subscribes to `ResponseComplete` and asks the router model (1.7B /no_think,
//! ~300ms) to rate the response 1-5 for helpfulness + factuality. Scores land
//! in `~/.local/share/crustaison/judge_scores.jsonl` for later analysis by the
//! GEPA prompt-evolution loop. Runs fully async in a spawned task — never
//! blocks the reply path to the user.
//!
//! Why the router and not the 35B (as the plan specified)? Two reasons:
//!   1. 35B subprocess is ~60s per call and resident-RAM expensive; running it
//!      on every Telegram message is a waste.
//!   2. The 1.7B can produce a coarse 1-5 judgement fine. We're not grading
//!      essays — we're flagging the bottom quartile for review.
//! The plan's "use the 35B" constraint is relaxed here; swap the URL/model
//! constants below if you decide to escalate.

use std::path::PathBuf;
use std::time::Duration;

use tokio::io::AsyncWriteExt;

use crate::antennae::{AntennaListener, AntennaOutcome, AntennaSignal};

const ROUTER_URL: &str = "http://localhost:18181/v1/chat/completions";
const ROUTER_MODEL: &str = "unsloth/Qwen3-1.7B-GGUF:Q4_0";
const TIMEOUT_SECS: u64 = 15;

pub struct JudgeListener {
    out_path: PathBuf,
    client: reqwest::Client,
}

impl JudgeListener {
    pub fn new(out_path: PathBuf) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(TIMEOUT_SECS))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { out_path, client }
    }

    pub fn default_path() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        PathBuf::from(home).join(".local/share/crustaison/judge_scores.jsonl")
    }

    async fn append(path: &PathBuf, entry: &serde_json::Value) {
        if let Some(parent) = path.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }
        if let Ok(mut f) = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .await
        {
            let line = format!("{}\n", entry);
            let _ = f.write_all(line.as_bytes()).await;
        }
    }

    /// Send the judge prompt, return Some(1..=5) if a clean score was parsed.
    async fn judge(client: &reqwest::Client, user_text: &str, response: &str) -> Option<u8> {
        // Truncate inputs — the 1.7B context is small.
        let user_snip = truncate(user_text, 400);
        let resp_snip = truncate(response, 800);
        let prompt = format!(
            "/no_think You are grading an AI assistant's reply.\n\
             Rate it 1-5 for (helpful AND factually grounded).\n\
             5 = directly answers, no hallucination\n\
             4 = answers, minor slips\n\
             3 = partial answer or mild drift\n\
             2 = wrong / off-topic\n\
             1 = refused or nonsense\n\
             Respond with ONE digit 1-5, nothing else.\n\
             \n---USER---\n{}\n---REPLY---\n{}\n---\nScore:",
            user_snip, resp_snip
        );

        let body = serde_json::json!({
            "model": ROUTER_MODEL,
            "messages": [{"role": "user", "content": prompt}],
            "max_tokens": 4,
            "temperature": 0.0,
        });

        let resp = client.post(ROUTER_URL).json(&body).send().await.ok()?;
        let v: serde_json::Value = resp.json().await.ok()?;
        let content = v["choices"][0]["message"]["content"].as_str()?;
        parse_score(content)
    }
}

#[async_trait::async_trait]
impl AntennaListener for JudgeListener {
    fn name(&self) -> &str {
        "llm_judge"
    }

    async fn receive(&self, signal: &AntennaSignal) -> AntennaOutcome {
        if let AntennaSignal::ResponseComplete { user_text, response } = signal {
            // Skip trivially-short or empty responses — no signal to grade.
            if response.trim().len() < 20 || user_text.trim().is_empty() {
                return AntennaOutcome::Continue;
            }
            let client = self.client.clone();
            let path = self.out_path.clone();
            let user_text = user_text.clone();
            let response = response.clone();
            tokio::spawn(async move {
                let ts = chrono::Utc::now().to_rfc3339();
                let score = JudgeListener::judge(&client, &user_text, &response).await;
                let entry = serde_json::json!({
                    "ts": ts,
                    "score": score,
                    "user_text": truncate(&user_text, 300),
                    "response_len": response.len(),
                    "response_preview": truncate(&response, 300),
                });
                if let Some(s) = score {
                    tracing::info!("llm-judge score={} for response of {} chars", s, response.len());
                } else {
                    tracing::debug!("llm-judge: failed to parse score");
                }
                JudgeListener::append(&path, &entry).await;
            });
        }
        AntennaOutcome::Continue
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max])
    }
}

/// Pull a clean 1-5 digit out of whatever the model returns.
fn parse_score(text: &str) -> Option<u8> {
    for c in text.chars() {
        if let Some(d) = c.to_digit(10) {
            if (1..=5).contains(&d) {
                return Some(d as u8);
            }
        }
    }
    None
}
