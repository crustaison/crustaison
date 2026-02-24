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

/// Real embedding generator using Nexa API
pub struct Embedder {
    api_endpoint: String,
    model: String,
}

impl Embedder {
    pub fn new(api_endpoint: String, model: String) -> Self {
        Self { api_endpoint, model }
    }
    
    /// Generate embedding for text using Nexa API
    pub async fn embed(&self, text: &str) -> Vec<f32> {
        let client = reqwest::Client::new();
        
        let payload = serde_json::json!({
            "model": self.model,
            "input": [text]
        });
        
        match client.post(&format!("{}/v1/embeddings", self.api_endpoint))
            .json(&payload)
            .send()
            .await {
                Ok(response) => {
                    if let Ok(json) = response.json::<serde_json::Value>().await {
                        if let Some(data) = json.get("data").and_then(|d| d.as_array()) {
                            if let Some(first) = data.first() {
                                if let Some(embedding) = first.get("embedding").and_then(|e| e.as_array()) {
                                    return embedding.iter()
                                        .filter_map(|v| v.as_f64().map(|f| f as f32))
                                        .collect();
                                }
                            }
                        }
                    }
                    // Fallback: return zero vector on parse failure
                    vec![0.0; 1024]
                }
                Err(_) => {
                    // Fallback: return zero vector on network failure
                    vec![0.0; 1024]
                }
            }
    }
    
    /// Generate embeddings for multiple texts
    pub async fn embed_batch(&self, texts: &[&str]) -> Vec<Vec<f32>> {
        let client = reqwest::Client::new();
        
        let payload = serde_json::json!({
            "model": self.model,
            "input": texts
        });
        
        match client.post(&format!("{}/v1/embeddings", self.api_endpoint))
            .json(&payload)
            .send()
            .await {
                Ok(response) => {
                    if let Ok(json) = response.json::<serde_json::Value>().await {
                        if let Some(data) = json.get("data").and_then(|d| d.as_array()) {
                            return data.iter()
                                .filter_map(|item| {
                                    item.get("embedding").and_then(|e| e.as_array()).map(|emb| {
                                        emb.iter()
                                            .filter_map(|v| v.as_f64().map(|f| f as f32))
                                            .collect()
                                    })
                                })
                                .collect();
                        }
                    }
                    // Fallback
                    vec![]
                }
                Err(_) => vec![]
            }
    }
}
