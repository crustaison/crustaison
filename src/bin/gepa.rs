//! GEPA-style prompt mutator — Phase 3 MVP.
//!
//! Full GEPA (Genetic-Pareto prompt evolution) needs a scoring function to
//! drive selection. Without labeled eval fixtures we can't score routing
//! decisions — a 5-point rubric from an LLM judge would be circular. So
//! this MVP does the *mutation* half honestly and leaves selection to a
//! human:
//!
//!   1. Read the current prompt file.
//!   2. Ask MiniMax to produce 3 variants with different trade-offs
//!      (stricter / looser / reworded).
//!   3. Write variants to `gepa_out/variant_{n}.txt`.
//!   4. Print a short comparison and tell the user to pick.
//!
//! Usage:
//!   cargo run --release --bin gepa -- --prompt path/to/current.txt
//!
//! Requires `CRUSTAISON_API_KEY` in the env.

use std::path::{Path, PathBuf};
use std::time::Duration;

const MINIMAX_URL: &str = "https://api.minimax.io/anthropic/chat/completions";
const MINIMAX_MODEL: &str = "MiniMax-M2.1";

#[derive(Debug)]
struct Args {
    prompt_path: PathBuf,
    out_dir: PathBuf,
    n_variants: usize,
}

fn parse_args() -> Result<Args, String> {
    let mut prompt_path: Option<PathBuf> = None;
    let mut out_dir = PathBuf::from("gepa_out");
    let mut n_variants = 3usize;
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--prompt" => prompt_path = it.next().map(PathBuf::from),
            "--out" => out_dir = it.next().map(PathBuf::from).unwrap_or(out_dir),
            "--n" => {
                n_variants = it.next().and_then(|s| s.parse().ok()).unwrap_or(3);
            }
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            other => return Err(format!("unknown arg: {}", other)),
        }
    }
    let prompt_path = prompt_path
        .ok_or_else(|| "missing --prompt <path>".to_string())?;
    Ok(Args {
        prompt_path,
        out_dir,
        n_variants,
    })
}

fn print_help() {
    println!("GEPA-style prompt mutator — generates variants for human selection.");
    println!();
    println!("Usage: gepa --prompt <path> [--out <dir>] [--n <count>]");
    println!();
    println!("  --prompt   path to the current prompt text file (required)");
    println!("  --out      output directory (default: gepa_out)");
    println!("  --n        number of variants to generate (default: 3)");
    println!();
    println!("Env: CRUSTAISON_API_KEY must be set (MiniMax auth).");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {}", e);
            print_help();
            std::process::exit(2);
        }
    };

    let api_key = std::env::var("CRUSTAISON_API_KEY")
        .map_err(|_| "CRUSTAISON_API_KEY not set in env")?;

    let current = tokio::fs::read_to_string(&args.prompt_path).await?;
    println!("Current prompt ({} bytes):", current.len());
    println!("---");
    println!("{}", preview(&current, 600));
    println!("---\n");

    tokio::fs::create_dir_all(&args.out_dir).await?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()?;

    let trade_offs: Vec<&str> = vec![
        "Stricter: narrower triggers, fewer false positives, more explicit rules",
        "Looser: broader applicability, more tolerant of ambiguity, less rigid",
        "Concise: same meaning in fewer words, drop redundancy, keep all invariants",
        "Explicit examples: add 2-3 short inline examples of correct behavior",
        "Role-forward: lead with persona/role, then rules",
    ];

    for i in 0..args.n_variants {
        let trade_off = trade_offs[i % trade_offs.len()];
        println!("Generating variant {} ({})...", i + 1, trade_off);
        match mutate(&client, &api_key, &current, trade_off).await {
            Ok(variant) => {
                let path = args.out_dir.join(format!("variant_{}.txt", i + 1));
                tokio::fs::write(&path, format!("# Trade-off: {}\n\n{}", trade_off, variant)).await?;
                println!("  → {}", path.display());
            }
            Err(e) => eprintln!("  ! variant {} failed: {}", i + 1, e),
        }
    }

    println!();
    println!("Variants written to {}", args.out_dir.display());
    println!("Review them by hand, then copy the winner over the original prompt.");
    println!("(GEPA's genetic-Pareto selection is not automated — selection needs a");
    println!("labeled eval corpus we don't have yet.)");
    Ok(())
}

fn preview(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max])
    }
}

async fn mutate(
    client: &reqwest::Client,
    api_key: &str,
    current: &str,
    trade_off: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let user_prompt = format!(
        "You are a prompt engineer. Rewrite the following system prompt with this specific \
         trade-off in mind:\n\n\
         TRADE-OFF: {}\n\n\
         Rules:\n\
         - Preserve all behavioral invariants (what the prompt must make the model do).\n\
         - Do not introduce new behaviors not implied by the original.\n\
         - Output ONLY the rewritten prompt text, no preamble, no explanation, no quotes.\n\n\
         ---ORIGINAL PROMPT---\n{}",
        trade_off, current
    );

    let body = serde_json::json!({
        "model": MINIMAX_MODEL,
        "max_tokens": 4096,
        "messages": [{"role": "user", "content": user_prompt}],
        "stream": false,
    });

    let resp = client
        .post(MINIMAX_URL)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("minimax {}: {}", status, preview(&body, 300)).into());
    }

    let v: serde_json::Value = resp.json().await?;
    let content = v["choices"][0]["message"]["content"]
        .as_str()
        .ok_or("no content in response")?;
    Ok(content.trim().to_string())
}

// Silence dead-code warning when the helper is unused in a particular code path.
#[allow(dead_code)]
fn _ensure_path_type(_p: &Path) {}
