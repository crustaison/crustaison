//! Coordinator - 5-Model Multi-Agent Router
//!
//! Routes incoming requests to the appropriate model:
//!   - LOCAL  → existing Agent (1.7B with tool loop, MiniMax for complex)
//!   - REASON → 35B via nexa infer subprocess (deep analysis, no tools)
//!
//! Also handles RAG: embeds each input, retrieves relevant memories, injects
//! context, and stores the exchange in the vector store after each response.

use crate::agent::Agent;
use crate::providers::nexa::NexaProvider;
use crate::providers::provider::{Provider, ChatMessage};
use crate::providers::subprocess::SubprocessProvider;
use crate::vector::{VectorStore, Embedder};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Central coordinator that routes requests across models
pub struct Coordinator {
    /// Existing agent — handles LOCAL routing (MiniMax + tools + RAG)
    agent: Arc<Mutex<Agent>>,
    /// 1.7B /no_think — used only for routing classification
    router: NexaProvider,
    /// 35B subprocess — for deep reasoning tasks (no tools)
    reason: SubprocessProvider,
    /// Embedding model for RAG
    embedder: Embedder,
    /// Persistent vector store for cross-session memory
    vector_store: Arc<Mutex<VectorStore>>,
}

impl Coordinator {
    pub fn new(
        agent: Arc<Mutex<Agent>>,
        router: NexaProvider,
        reason: SubprocessProvider,
        embedder: Embedder,
        vector_store: Arc<Mutex<VectorStore>>,
    ) -> Self {
        Self {
            agent,
            router,
            reason,
            embedder,
            vector_store,
        }
    }

    /// Process a text message: retrieve memories, route, dispatch, store result.
    pub async fn process(&self, input: &str, _user_id: i64) -> String {
        // 1. Retrieve relevant memories in parallel with routing
        let rag_context = self.get_rag_context(input).await;

        // 2. Determine route
        let route = self.route(input).await;
        tracing::info!("Coordinator route: {} for input: {:.60}...", route, input);

        // 3. Dispatch to appropriate model
        let response = match route.as_str() {
            "REASON" => self.dispatch_reason(input, &rag_context).await,
            _ => self.dispatch_agent(input, &rag_context).await,
        };

        // 4. Store this exchange in vector memory for future sessions
        self.store_memory(input, &response).await;

        response
    }

    /// Route: ask 1.7B /no_think to classify as LOCAL or REASON
    async fn route(&self, input: &str) -> String {
        let routing_prompt = format!(
            "Classify this request into exactly one word — LOCAL or REASON.\n\
             LOCAL = conversation, questions, tool use, commands, tasks, reminders, anything quick\n\
             REASON = complex multi-step analysis, detailed research reports, long writing tasks\n\
             \nRequest: {}\n\nOne word only:",
            input
        );

        // Prepend /no_think to skip thinking tokens for fast classification
        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: format!("/no_think {}", routing_prompt),
            images: Vec::new(),
        }];

        match self.router.chat(messages, None).await {
            Ok(response) => {
                let text = response.content.trim().to_uppercase();
                if text.contains("REASON") {
                    "REASON".to_string()
                } else {
                    "LOCAL".to_string()
                }
            }
            Err(e) => {
                tracing::warn!("Router failed, defaulting to LOCAL: {}", e);
                "LOCAL".to_string()
            }
        }
    }

    /// Dispatch to existing agent (handles tools + MiniMax conversation)
    async fn dispatch_agent(&self, input: &str, rag_context: &str) -> String {
        let full_input = if rag_context.is_empty() {
            input.to_string()
        } else {
            format!("{}\n\n{}", rag_context, input)
        };

        let mut agent = self.agent.lock().await;
        agent
            .chat(&full_input)
            .await
            .unwrap_or_else(|e| format!("Error: {}", e))
    }

    /// Dispatch to 35B subprocess for deep reasoning
    async fn dispatch_reason(&self, input: &str, rag_context: &str) -> String {
        let system_prompt = if rag_context.is_empty() {
            "You are Crusty, a thoughtful AI assistant. Provide thorough, well-reasoned analysis.".to_string()
        } else {
            format!(
                "You are Crusty, a thoughtful AI assistant. Provide thorough, well-reasoned analysis.\n\n{}",
                rag_context
            )
        };

        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: input.to_string(),
            images: Vec::new(),
        }];

        match self.reason.chat(messages, Some(system_prompt)).await {
            Ok(response) => {
                if response.content.is_empty() {
                    // Fallback to agent if 35B returns empty
                    tracing::warn!("35B returned empty, falling back to agent");
                    let mut agent = self.agent.lock().await;
                    agent.chat(input).await.unwrap_or_else(|e| format!("Error: {}", e))
                } else {
                    response.content
                }
            }
            Err(e) => {
                tracing::warn!("35B reasoning failed ({}), falling back to agent", e);
                let mut agent = self.agent.lock().await;
                agent.chat(input).await.unwrap_or_else(|e| format!("Error: {}", e))
            }
        }
    }

    /// Retrieve relevant memories from the vector store
    async fn get_rag_context(&self, input: &str) -> String {
        let embedding = self.embedder.embed(input).await;

        // If embedding is all zeros, the embedder failed — skip RAG
        if embedding.iter().all(|&v| v == 0.0) {
            return String::new();
        }

        let store = self.vector_store.lock().await;
        let results = store.search(&embedding, 3);

        let context: Vec<String> = results
            .iter()
            .filter(|(_, sim)| *sim > 0.4)
            .filter_map(|(id, _)| store.get(id))
            .map(|entry| entry.text.clone())
            .collect();

        if context.is_empty() {
            String::new()
        } else {
            format!("## Relevant Memory\n\n{}", context.join("\n---\n"))
        }
    }

    /// Store this exchange in the vector store for future retrieval
    async fn store_memory(&self, input: &str, response: &str) {
        // Only store meaningful exchanges (skip very short responses)
        if response.len() < 20 {
            return;
        }

        let memory_text = format!("Q: {}\nA: {}", input, &response[..response.len().min(500)]);
        let embedding = self.embedder.embed(&memory_text).await;

        if !embedding.iter().all(|&v| v == 0.0) {
            let mut store = self.vector_store.lock().await;
            store.add(&memory_text, embedding, None);
        }
    }
}
