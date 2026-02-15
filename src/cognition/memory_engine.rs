//! Memory Engine - Structured State
//!
//! IMPORTANT: The database is the AUTHORITATIVE state.
//! Markdown files are COGNITIVE INPUT for the planner only.
//!
//! This module manages the SQLite-backed structured memory.

use serde::{Deserialize, Serialize};
use sqlx::{SqlitePool, Row};
use anyhow::{Context, Result};
use std::path::PathBuf;

/// Structured memory record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRecord {
    pub id: i64,
    pub key: String,
    pub value: serde_json::Value,
    pub record_type: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Memory engine backed by SQLite
pub struct MemoryEngine {
    pool: SqlitePool,
}

impl MemoryEngine {
    /// Create or connect to memory database
    pub async fn new(db_path: &str) -> Result<Self> {
        let pool = SqlitePool::connect(db_path)
            .await
            .context("Failed to connect to memory database")?;
            
        // Create tables if they don't exist
        sqlx::query("
            CREATE TABLE IF NOT EXISTS memory (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                key TEXT UNIQUE NOT NULL,
                value TEXT NOT NULL,
                record_type TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )
        ").execute(&pool).await?;
        
        Ok(Self { pool })
    }
    
    /// Store a memory record
    pub async fn store(&self, key: &str, value: &serde_json::Value, record_type: &str) -> Result<()> {
        let now = chrono::Utc::now().timestamp_millis();
        
        sqlx::query(
            "INSERT OR REPLACE INTO memory (key, value, record_type, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?)"
        )
        .bind(key)
        .bind(value.to_string())
        .bind(record_type)
        .bind(now)
        .bind(now)
        .execute(&self.pool).await?;
        
        Ok(())
    }
    
    /// Retrieve a memory record
    pub async fn recall(&self, key: &str) -> Result<Option<MemoryRecord>> {
        let row = sqlx::query(
            "SELECT id, key, value, record_type, created_at, updated_at
             FROM memory WHERE key = ?"
        )
        .bind(key)
        .fetch_optional(&self.pool).await?;
        
        match row {
            Some(r) => Ok(Some(MemoryRecord {
                id: r.get(0),
                key: r.get(1),
                value: serde_json::from_str(&r.get::<String, _>(2))
                    .unwrap_or(serde_json::json!({})),
                record_type: r.get(3),
                created_at: r.get(4),
                updated_at: r.get(5),
            })),
            None => Ok(None),
        }
    }
    
    /// Search memories by type
    pub async fn search_by_type(&self, record_type: &str) -> Result<Vec<MemoryRecord>> {
        let rows = sqlx::query(
            "SELECT id, key, value, record_type, created_at, updated_at
             FROM memory WHERE record_type = ? ORDER BY updated_at DESC"
        )
        .bind(record_type)
        .fetch_all(&self.pool).await?;
        
        Ok(rows.into_iter().map(|r| MemoryRecord {
            id: r.get(0),
            key: r.get(1),
            value: serde_json::from_str(&r.get::<String, _>(2))
                .unwrap_or(serde_json::json!({})),
            record_type: r.get(3),
            created_at: r.get(4),
            updated_at: r.get(5),
        }).collect())
    }
    
    /// List all memory keys
    pub async fn list_keys(&self) -> Result<Vec<String>> {
        let rows = sqlx::query(
            "SELECT key FROM memory ORDER BY updated_at DESC"
        )
        .fetch_all(&self.pool).await?;
        
        Ok(rows.into_iter().map(|r| r.get(0)).collect())
    }
    
    /// Delete a memory record
    pub async fn delete(&self, key: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM memory WHERE key = ?")
            .bind(key)
            .execute(&self.pool).await?;
            
        Ok(result.rows_affected() > 0)
    }
    
    /// Clear all memory
    pub async fn clear(&self) -> Result<()> {
        sqlx::query("DELETE FROM memory")
            .execute(&self.pool).await?;
        Ok(())
    }
}
