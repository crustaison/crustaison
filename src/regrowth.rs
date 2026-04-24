//! Regrowth — named failure recovery (crustacean-ified "recovery_recipes").
//!
//! When a tool call fails, classify the failure into a `LimbLoss` variant and
//! try to regrow the limb before escalating to the user. Crustaceans can
//! regenerate severed limbs between molts; Crusty does the same with errors.
//!
//! Kept small intentionally: three recipes cover ~80% of transient failures
//! observed in the Helm/Jimmy logs. Add more as new patterns surface.

use std::sync::Arc;
use std::time::Duration;

use crate::tools::{ToolRegistry, ToolResult};

/// Named failure patterns Crusty knows how to recover from.
#[derive(Debug, Clone)]
pub enum LimbLoss {
    /// Tool name didn't resolve — try stripping common prefixes/suffixes.
    ToolNotFound { name: String },
    /// Provider rate-limited us — back off and retry once.
    RateLimited { retry_after_ms: u64 },
    /// Transient network error — short backoff and retry once.
    NetworkTransient,
}

/// Classifier. Returns Some(LimbLoss) if the error pattern matches a known recipe.
pub fn classify(error: &str, tool: &str) -> Option<LimbLoss> {
    let e = error.to_lowercase();
    if e.contains("tool not found") || e.contains("unknown tool") {
        return Some(LimbLoss::ToolNotFound { name: tool.to_string() });
    }
    if e.contains("rate limit") || e.contains("429") || e.contains("too many requests") {
        return Some(LimbLoss::RateLimited { retry_after_ms: 1500 });
    }
    if e.contains("connection refused")
        || e.contains("connection reset")
        || e.contains("timed out")
        || e.contains("temporary failure")
    {
        return Some(LimbLoss::NetworkTransient);
    }
    None
}

/// Regrowth coordinator — holds the tool registry so recipes can retry.
pub struct Regrowth {
    tool_registry: Arc<ToolRegistry>,
}

impl Regrowth {
    pub fn new(tool_registry: Arc<ToolRegistry>) -> Self {
        Self { tool_registry }
    }

    /// Attempt to regrow. Returns Some(ToolResult) if recovery succeeded; None
    /// if the recipe gave up. Only one recovery attempt per failure — we don't
    /// loop forever.
    pub async fn attempt(
        &self,
        limb: LimbLoss,
        tool: &str,
        args: &serde_json::Value,
    ) -> Option<ToolResult> {
        match limb {
            LimbLoss::ToolNotFound { name } => {
                tracing::info!("regrowth: ToolNotFound for '{}', trying variants", name);
                let candidates = [
                    name.trim_start_matches("mcp_").to_string(),
                    name.trim_start_matches("fn_").to_string(),
                    name.trim_end_matches("_v2").to_string(),
                    name.trim_end_matches("_tool").to_string(),
                ];
                for variant in candidates {
                    if variant == name {
                        continue;
                    }
                    if let Some(real) = self.tool_registry.resolve(&variant).await {
                        tracing::info!("regrowth: ToolNotFound → {} → {}", name, real);
                        let result = self.tool_registry.execute(&real, args.clone()).await;
                        if result.success {
                            return Some(result);
                        }
                    }
                }
                None
            }
            LimbLoss::RateLimited { retry_after_ms } => {
                tracing::info!(
                    "regrowth: RateLimited on '{}', sleeping {}ms and retrying",
                    tool, retry_after_ms
                );
                tokio::time::sleep(Duration::from_millis(retry_after_ms)).await;
                let result = self.tool_registry.execute(tool, args.clone()).await;
                if result.success { Some(result) } else { None }
            }
            LimbLoss::NetworkTransient => {
                tracing::info!("regrowth: NetworkTransient on '{}', short backoff", tool);
                tokio::time::sleep(Duration::from_millis(500)).await;
                let result = self.tool_registry.execute(tool, args.clone()).await;
                if result.success { Some(result) } else { None }
            }
        }
    }
}
