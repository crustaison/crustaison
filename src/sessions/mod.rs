//! Session Manager - Persist conversation sessions
//!
//! SQLite-backed session storage for chat history.

use sqlx::{SqlitePool, FromRow};
use serde::{Deserialize, Serialize};
use chrono::Utc;

/// A chat session
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Session {
    pub id: String,
    pub name: String,
    pub created_at: i64,
    pub updated_at: i64,
    #[sqlx(default)]
    pub message_count: i64,
}

/// A message within a session
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SessionMessage {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub created_at: i64,
}

/// Session manager
pub struct SessionManager {
    pool: SqlitePool,
}

impl SessionManager {
    /// Create new session manager with database path
    pub async fn new(db_path: &str) -> Result<Self, sqlx::Error> {
        let url = format!("sqlite://{}", db_path);
        let pool = SqlitePool::connect(&url).await?;
        
        // Initialize tables
        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )
        "#).execute(&pool).await?;
        
        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS session_messages (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                FOREIGN KEY (session_id) REFERENCES sessions(id)
            )
        "#).execute(&pool).await?;
        
        Ok(Self { pool })
    }
    
    /// Create a new session
    pub async fn create_session(&self, name: &str) -> Result<Session, sqlx::Error> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now().timestamp_millis();
        
        sqlx::query(
            "INSERT INTO sessions (id, name, created_at, updated_at) VALUES (?, ?, ?, ?)"
        )
        .bind(&id)
        .bind(name)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;
        
        Ok(Session {
            id,
            name: name.to_string(),
            created_at: now,
            updated_at: now,
            message_count: 0,
        })
    }
    
    /// List all sessions
    pub async fn list_sessions(&self) -> Result<Vec<Session>, sqlx::Error> {
        let sessions = sqlx::query_as::<_, Session>(
            "SELECT s.id, s.name, s.created_at, s.updated_at, COUNT(sm.id) as message_count
             FROM sessions s LEFT JOIN session_messages sm ON s.id = sm.session_id
             GROUP BY s.id ORDER BY s.updated_at DESC"
        )
        .fetch_all(&self.pool)
        .await?;
        
        Ok(sessions)
    }
    
    /// Get a session by ID
    pub async fn get_session(&self, id: &str) -> Result<Option<Session>, sqlx::Error> {
        let session = sqlx::query_as::<_, Session>(
            "SELECT id, name, created_at, updated_at, 0 as message_count FROM sessions WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        
        Ok(session)
    }
    
    /// Get messages for a session
    pub async fn get_messages(&self, session_id: &str) -> Result<Vec<SessionMessage>, sqlx::Error> {
        let messages = sqlx::query_as::<_, SessionMessage>(
            "SELECT id, session_id, role, content, created_at FROM session_messages WHERE session_id = ? ORDER BY created_at ASC"
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;
        
        Ok(messages)
    }
    
    /// Add a message to a session
    pub async fn add_message(
        &self,
        session_id: &str,
        role: &str,
        content: &str,
    ) -> Result<SessionMessage, sqlx::Error> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now().timestamp_millis();
        
        sqlx::query(
            "INSERT INTO session_messages (id, session_id, role, content, created_at) VALUES (?, ?, ?, ?, ?)"
        )
        .bind(&id)
        .bind(session_id)
        .bind(role)
        .bind(content)
        .bind(now)
        .execute(&self.pool)
        .await?;
        
        // Update session timestamp
        sqlx::query(
            "UPDATE sessions SET updated_at = ? WHERE id = ?"
        )
        .bind(now)
        .bind(session_id)
        .execute(&self.pool)
        .await?;
        
        Ok(SessionMessage {
            id,
            session_id: session_id.to_string(),
            role: role.to_string(),
            content: content.to_string(),
            created_at: now,
        })
    }
    
    /// Delete a session and its messages
    pub async fn delete_session(&self, id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM session_messages WHERE session_id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        
        sqlx::query("DELETE FROM sessions WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        
        Ok(())
    }
    
    /// Clear all messages from a session
    pub async fn clear_session(&self, session_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM session_messages WHERE session_id = ?")
            .bind(session_id)
            .execute(&self.pool)
            .await?;
        
        Ok(())
    }
    
    /// Search messages in all sessions
    pub async fn search(&self, query: &str) -> Result<Vec<SessionMessage>, sqlx::Error> {
        let search = format!("%{}%", query);
        let messages = sqlx::query_as::<_, SessionMessage>(
            "SELECT id, session_id, role, content, created_at FROM session_messages WHERE content LIKE ? ORDER BY created_at DESC LIMIT 50"
        )
        .bind(&search)
        .fetch_all(&self.pool)
        .await?;
        
        Ok(messages)
    }
}
