//! Eval harness — Phase 1 scaffold.
//!
//! Lists fixtures under `evals/` and prints a summary. Actual replay +
//! LLM-judge scoring lands in Phase 3 once we have a frozen corpus.

use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let evals_dir = PathBuf::from("evals");
    if !evals_dir.exists() {
        eprintln!("No evals/ directory found at {:?}", evals_dir);
        eprintln!("See evals/README.md for the expected layout.");
        std::process::exit(1);
    }

    let mut fixtures: Vec<PathBuf> = std::fs::read_dir(&evals_dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("jsonl"))
        .collect();
    fixtures.sort();

    println!("Crustaison eval harness — Phase 1 scaffold");
    println!("==========================================");
    println!();

    if fixtures.is_empty() {
        println!("No .jsonl fixtures in {:?} yet.", evals_dir);
        println!("Drop real Telegram transcripts in, then re-run.");
        return Ok(());
    }

    println!("Found {} fixture(s):", fixtures.len());
    for f in &fixtures {
        let name = f.file_name().and_then(|n| n.to_str()).unwrap_or("?");
        let lines = std::fs::read_to_string(f)
            .map(|c| c.lines().count())
            .unwrap_or(0);
        println!("  {} ({} turns)", name, lines);
    }
    println!();
    println!("Replay mode not yet implemented (Phase 3).");
    Ok(())
}
