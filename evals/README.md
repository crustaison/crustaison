# Crustaison Eval Harness

Frozen conversations used to detect regressions after prompt, router, or
tool-registry changes. Prerequisite for GEPA prompt evolution and
Karpathy-style autoresearch branches.

## Format

Each `.jsonl` file is one eval fixture. Lines are `{"role", "content"}` pairs
exactly as they appeared in the real Telegram transcript. The LAST line
is always role=`"expected"` and contains a short natural-language description
of what a passing response looks like (not an exact string match — the harness
uses an LLM judge).

Example:

    {"role": "user", "content": "what time is it"}
    {"role": "expected", "content": "Responds with a datetime tool call and formats the result for humans."}

## Running

    cargo run --release --bin eval -- [--fixture path/to.jsonl]

With no args, lists all fixtures in `evals/` and exits. Full replay mode is
TODO — the harness is scaffolded now so the directory layout and Cargo bin
are in place; actual replay + diff + judging lands in Phase 3.

## Seed fixtures

Drop 20 real transcripts here before running GEPA. Pull them from the
Telegram bot logs:

    journalctl -u crustaison-telegram --since "7 days ago" \
      | grep -E "USER:|BOT:" > /tmp/transcript.log

Then hand-split by session break.
