//! RAG - Retrieval-Augmented Generation
//!
//! Combines vector search with LLM generation for grounded responses.

use crate::vector::{VectorStore, Embedder, EmbeddingEntry};
use serde::{Deserialize, Serialize};

/// RAG Configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RAGConfig {
    pub enabled: bool,
    pub max_context_docs: usize,
    pub min_similarity: f32,
    pub chunk_size: usize,
    pub chunk_overlap: usize,
}

/// A document chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentChunk {
    pub id: String,
    pub content: String,
    pub source: String,
    pub embedding: Vec<f32>,
    pub metadata: Option<serde_json::Value>,
    pub start_char: usize,
    pub end_char: usize,
}

/// RAG Engine
pub struct RAGEngine {
    vector_store: VectorStore,
    embedder: Embedder,
    config: RAGConfig,
}

impl RAGEngine {
    /// Create new RAG engine
    pub fn new(vector_store: VectorStore, embedder: Embedder, config: RAGConfig) -> Self {
        Self {
            vector_store,
            embedder,
            config,
        }
    }
    
    /// Index a document
    pub fn index_document(&mut self, content: &str, source: &str, metadata: Option<serde_json::Value>) -> Vec<String> {
        // Split into chunks
        let chunks = self.chunk_text(content);
        
        let mut ids = Vec::new();
        for chunk in &chunks {
            let embedding = self.embedder.embed(chunk);
            let id = self.vector_store.add(chunk, embedding, metadata.clone());
            ids.push(id);
        }
        
        ids
    }
    
    /// Search and retrieve relevant context
    pub fn retrieve(&self, query: &str, max_docs: Option<usize>) -> Vec<&EmbeddingEntry> {
        let query_embedding = self.embedder.embed(query);
        let limit = max_docs.unwrap_or(self.config.max_context_docs);
        
        let results = self.vector_store.search(&query_embedding, limit);
        
        results.into_iter()
            .filter(|(_, sim)| *sim >= self.config.min_similarity)
            .map(|(id, _)| self.vector_store.get(&id))
            .filter(Option::is_some)
            .map(Option::unwrap)
            .collect()
    }
    
    /// Build context from retrieved documents
    pub fn build_context(&self, query: &str) -> String {
        let docs = self.retrieve(query, None);
        
        if docs.is_empty() {
            return String::new();
        }
        
        let context: Vec<String> = docs.iter()
            .map(|d| format!("[Source: {}]\n{}", 
                d.metadata.as_ref()
                    .and_then(|m| m.get("source").cloned())
                    .unwrap_or_else(|| serde_json::json!("unknown")),
                d.text
            ))
            .collect();
        
        format!("## Retrieved Context\n\n{}", context.join("\n\n---\n\n"))
    }
    
    /// Chunk text into overlapping segments
    fn chunk_text(&self, text: &str) -> Vec<String> {
        let mut chunks = Vec::new();
        let chars: Vec<char> = text.chars().collect();
        
        if chars.len() <= self.config.chunk_size {
            return vec![text.to_string()];
        }
        
        let mut start = 0;
        while start < chars.len() {
            let end = (start + self.config.chunk_size).min(chars.len());
            
            // Try to break at sentence boundary
            let mut break_point = end;
            for i in (start..end).rev() {
                if chars[i] == '.' || chars[i] == '!' || chars[i] == '?' || chars[i] == '\n' {
                    if i > start + self.config.chunk_size / 4 {
                        break_point = i + 1;
                        break;
                    }
                }
            }
            
            let chunk: String = chars[start..break_point].iter().collect();
            if !chunk.trim().is_empty() {
                chunks.push(chunk.trim().to_string());
            }
            
            // Move forward with overlap
            start = if break_point >= end {
                end.min(chars.len().saturating_sub(self.config.chunk_overlap))
            } else {
                break_point
            };
        }
        
        chunks
    }
    
    /// Get stats
    pub fn stats(&self) -> RAGStats {
        RAGStats {
            total_documents: self.vector_store.len(),
            config: self.config.clone(),
        }
    }
}

/// RAG statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RAGStats {
    pub total_documents: usize,
    pub config: RAGConfig,
}
