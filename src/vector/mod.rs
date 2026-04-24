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

/// Real embedding generator using llama-cpp-python subprocess
pub struct Embedder {
    script_path: String,
}

impl Embedder {
    pub fn new(_api_endpoint: String, _model: String) -> Self {
        Self {
            script_path: "/home/sean/crustaison/scripts/embed.py".to_string(),
        }
    }

    /// Generate embedding for text via Python subprocess
    pub async fn embed(&self, text: &str) -> Vec<f32> {
        use tokio::process::Command;
        use tokio::io::AsyncWriteExt;

        let mut child = match Command::new("python3")
            .arg(&self.script_path)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            Ok(c) => c,
            Err(_) => return vec![0.0; 1024],
        };

        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(text.as_bytes()).await;
        }

        let output = match child.wait_with_output().await {
            Ok(o) => o,
            Err(_) => return vec![0.0; 1024],
        };

        if !output.status.success() {
            return vec![0.0; 1024];
        }

        match serde_json::from_slice::<serde_json::Value>(&output.stdout) {
            Ok(json) => {
                if let Some(arr) = json.get("embedding").and_then(|e| e.as_array()) {
                    return arr.iter()
                        .filter_map(|v| v.as_f64().map(|f| f as f32))
                        .collect();
                }
                vec![0.0; 1024]
            }
            Err(_) => vec![0.0; 1024],
        }
    }
    
    /// Generate embeddings for multiple texts (calls embed() sequentially)
    pub async fn embed_batch(&self, texts: &[&str]) -> Vec<Vec<f32>> {
        let mut results = Vec::new();
        for text in texts {
            results.push(self.embed(text).await);
        }
        results
    }
}
