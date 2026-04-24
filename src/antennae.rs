//! Antennae — lifecycle event bus (crustacean-ified "hooks").
//!
//! An `AntennaSignal` fires at well-known points in the agent loop (tool use,
//! session lifecycle, etc.). Registered `AntennaListener`s receive the signal
//! in registration order and may return an `AntennaOutcome` that influences
//! control flow (block, warn, modify args).
//!
//! Event names are kept verbatim from the ClawdCode 11-event hook enum so a
//! listener written against that spec drops into Crustaison unchanged. Only
//! the container (module name, trait name) is Crusty-native vocabulary.

use std::sync::Arc;
use tokio::sync::RwLock;

/// Lifecycle events Crusty's antennae can sense.
#[derive(Debug, Clone)]
pub enum AntennaSignal {
    // Tool execution
    PreToolUse {
        tool: String,
        args: serde_json::Value,
    },
    PostToolUse {
        tool: String,
        args: serde_json::Value,
        success: bool,
        output: String,
    },
    PostToolUseFailure {
        tool: String,
        args: serde_json::Value,
        error: String,
    },
    PermissionRequest {
        tool: String,
        args: serde_json::Value,
    },

    // Session lifecycle
    UserPromptSubmit {
        text: String,
    },
    SessionStart {
        session_id: String,
    },
    SessionEnd {
        session_id: String,
    },

    // Conversation lifecycle (Crustaison-added; not in ClawdCode's 11-event set)
    /// Fires after the agent returns a response to the user. Listeners run
    /// async (telemetry, LLM-judge, compaction triggers) without blocking
    /// the reply path.
    ResponseComplete {
        user_text: String,
        response: String,
    },

    // Control flow
    Stop,
    SubagentStop,

    // Other
    Notification {
        message: String,
    },
    Compaction {
        before_tokens: usize,
        after_tokens: usize,
    },
}

/// Outcome a listener can return. Semantics match ClawdCode exit codes:
///   Continue → 0 (pass through)
///   Warn    → 1 (non-blocking error; log and proceed)
///   Block   → 2 (abort the action)
///   Modified → PreToolUse only; listener rewrote the arguments.
#[derive(Debug, Clone)]
pub enum AntennaOutcome {
    Continue,
    Modified(serde_json::Value),
    Warn(String),
    Block(String),
}

/// Trait for listeners that respond to antenna signals.
#[async_trait::async_trait]
pub trait AntennaListener: Send + Sync {
    fn name(&self) -> &str;
    async fn receive(&self, signal: &AntennaSignal) -> AntennaOutcome;
}

/// In-process dispatcher. Listeners are called in registration order; the
/// first non-Continue outcome short-circuits and is returned to the caller.
pub struct AntennaBus {
    listeners: RwLock<Vec<Arc<dyn AntennaListener>>>,
}

impl AntennaBus {
    pub fn new() -> Self {
        Self {
            listeners: RwLock::new(Vec::new()),
        }
    }

    pub async fn register(&self, listener: Arc<dyn AntennaListener>) {
        let mut ls = self.listeners.write().await;
        tracing::info!("antenna listener registered: {}", listener.name());
        ls.push(listener);
    }

    /// Fire a signal. Returns the first non-Continue outcome, or Continue if
    /// all listeners passed through.
    pub async fn fire(&self, signal: &AntennaSignal) -> AntennaOutcome {
        let ls = self.listeners.read().await;
        for l in ls.iter() {
            let outcome = l.receive(signal).await;
            match outcome {
                AntennaOutcome::Continue => continue,
                other => {
                    tracing::debug!(
                        "antenna '{}' short-circuited on {:?}: {:?}",
                        l.name(),
                        std::mem::discriminant(signal),
                        other
                    );
                    return other;
                }
            }
        }
        AntennaOutcome::Continue
    }
}

impl Default for AntennaBus {
    fn default() -> Self {
        Self::new()
    }
}
