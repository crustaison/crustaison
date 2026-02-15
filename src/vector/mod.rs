//! Vector Store - Embedding-based semantic search
//!
//! Stores text embeddings for semantic similarity search.

use serde::{Deserialize, Serialize};
use std::path::{PathBuf, Path};
use std::fs;
use serde_json;
use std::collections::HashMap;

/// A stored embedding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingEntry {
    pub id: String,
    pub text: String,
    pub embedding: Vec<f32>,
    pub metadata: Option<serde_json::Value>,
    pub created_at: i64,
}

/// Vector store configuration
#[derive(Debug, Clone)]
pub struct VectorStoreConfig {
    pub path: PathBuf,
    pub dimension: usize,
}

/// Simple vector store using raw embeddings
pub struct VectorStore {
    path: PathBuf,
    embeddings: HashMap<String, EmbeddingEntry>,
}

impl VectorStore {
    /// Create new vector store
    pub fn new(path: PathBuf) -> Self {
        if !path.exists() {
            let _ = fs::create_dir_all(&path);
        }
        
        let mut store = Self {
            path: path.join("embeddings.json"),
            embeddings: HashMap::new(),
        };
        
        // Load existing embeddings
        if store.path.exists() {
            if let Ok(content) = fs::read_to_string(&store.path) {
                if let Ok(embeddings) = serde_json::from_str(&content) {
                    store.embeddings = embeddings;
                }
            }
        }
        
        store
    }
    
    /// Add an embedding
    pub fn add(&mut self, text: &str, embedding: Vec<f32>, metadata: Option<serde_json::Value>) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let entry = EmbeddingEntry {
            id: id.clone(),
            text: text.to_string(),
            embedding,
            metadata,
            created_at: chrono::Utc::now().timestamp_millis(),
        };
        self.embeddings.insert(id.clone(), entry);
        self.save();
        id
    }
    
    /// Search by cosine similarity
    pub fn search(&self, query: &[f32], limit: usize) -> Vec<(String, f32)> {
        let mut results: Vec<(String, f32)> = self.embeddings.values()
            .map(|entry| {
                let similarity = cosine_similarity(query, &entry.embedding);
                (entry.id.clone(), similarity)
            })
            .filter(|(_, sim)| *sim > 0.0) // Basic filter
            .collect();
        
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        results.into_iter().take(limit).collect()
    }
    
    /// Get entry by ID
    pub fn get(&self, id: &str) -> Option<&EmbeddingEntry> {
        self.embeddings.get(id)
    }
    
    /// Delete entry
    pub fn delete(&mut self, id: &str) -> bool {
        let removed = self.embeddings.remove(id);
        if removed.is_some() {
            self.save();
        }
        removed.is_some()
    }
    
    /// Get count
    pub fn len(&self) -> usize {
        self.embeddings.len()
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.embeddings.is_empty()
    }
    
    /// Save to disk
    fn save(&self) {
        if let Ok(content) = serde_json::to_string(&self.embeddings) {
            let _ = fs::write(&self.path, content);
        }
    }
}

/// Cosine similarity between two vectors
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    
    let mut dot_product = 0.0;
    let mut norm_a = 0.0;
    let mut norm_b = 0.0;
    
    for i in 0..a.len() {
        dot_product += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }
    
    let norm_product = norm_a.sqrt() * norm_b.sqrt();
    if norm_product == 0.0 {
        0.0
    } else {
        dot_product / norm_product
    }
}

/// Simple embedding generator (placeholder)
/// In production, use a proper embedding model API
pub struct Embedder {
    api_endpoint: Option<String>,
    dimension: usize,
}

impl Embedder {
    pub fn new(api_endpoint: Option<String>, dimension: usize) -> Self {
        Self { api_endpoint, dimension }
    }
    
    /// Generate embedding for text
    /// Uses simple TF-IDF like approach as placeholder
    pub fn embed(&self, text: &str) -> Vec<f32> {
        // Simple bag-of-words style embedding
        // In production, replace with actual embedding model
        let words: Vec<&str> = text.split_whitespace().collect();
        let mut embedding = vec![0.0; self.dimension.min(384)];
        
        for (i, word) in words.iter().enumerate().take(embedding.len()) {
            // Simple hash-based value
            let hash = self.simple_hash(word);
            embedding[i] = ((hash % 100) as f32) / 100.0;
        }
        
        // Normalize
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in &mut embedding {
                *v /= norm;
            }
        }
        
        embedding
    }
    
    fn simple_hash(&self, s: &str) -> u64 {
        let mut h = 0u64;
        for c in s.chars() {
            h = h.wrapping_mul(31).wrapping_add(c as u64);
        }
        h
    }
}
