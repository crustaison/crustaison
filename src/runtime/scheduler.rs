//! Task Scheduler - Scheduled task queue for the heartbeat
//!
//! Persists tasks to a JSON file. The heartbeat checks for due tasks
//! each cycle and executes them, sending results to Telegram.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledTask {
    pub id: String,
    pub action: TaskAction,
    pub due_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub chat_id: i64,
    pub status: TaskStatus,
    pub result: Option<String>,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interval_secs: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskAction {
    Weather { location: String },
    Command { command: String },
    Reminder { message: String },
    WebFetch { url: String },
    Custom { prompt: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

pub struct TaskQueue {
    path: PathBuf,
}

impl TaskQueue {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub async fn load(&self) -> Vec<ScheduledTask> {
        match fs::read_to_string(&self.path).await {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    }

    pub async fn save(&self, tasks: &[ScheduledTask]) -> std::io::Result<()> {
        let content = serde_json::to_string_pretty(tasks)?;
        fs::write(&self.path, content).await
    }

    pub async fn add(&self, task: ScheduledTask) -> std::io::Result<()> {
        let mut tasks = self.load().await;
        tasks.push(task);
        self.save(&tasks).await
    }

    pub async fn get_due_tasks(&self) -> Vec<ScheduledTask> {
        let now = Utc::now();
        self.load().await.into_iter()
            .filter(|t| t.status == TaskStatus::Pending && t.due_at <= now)
            .collect()
    }

    pub async fn start_task(&self, task_id: &str) -> std::io::Result<()> {
        let mut tasks = self.load().await;
        if let Some(task) = tasks.iter_mut().find(|t| t.id == task_id) {
            task.status = TaskStatus::Running;
        }
        self.save(&tasks).await
    }

    pub async fn complete_task(&self, task_id: &str, result: String) -> std::io::Result<()> {
        let mut tasks = self.load().await;
        if let Some(task) = tasks.iter_mut().find(|t| t.id == task_id) {
            task.status = TaskStatus::Completed;
            task.result = Some(result);
        }
        self.save(&tasks).await
    }

    pub async fn fail_task(&self, task_id: &str, error: String) -> std::io::Result<()> {
        let mut tasks = self.load().await;
        if let Some(task) = tasks.iter_mut().find(|t| t.id == task_id) {
            task.status = TaskStatus::Failed;
            task.result = Some(error);
        }
        self.save(&tasks).await
    }

    pub async fn list_pending(&self) -> Vec<ScheduledTask> {
        self.load().await.into_iter()
            .filter(|t| t.status == TaskStatus::Pending)
            .collect()
    }

    pub async fn cleanup(&self) -> std::io::Result<()> {
        let tasks = self.load().await;
        let pending: Vec<_> = tasks.iter().filter(|t| t.status == TaskStatus::Pending || t.status == TaskStatus::Running).cloned().collect();
        let mut done: Vec<_> = tasks.iter().filter(|t| t.status == TaskStatus::Completed || t.status == TaskStatus::Failed).cloned().collect();
        done.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        done.truncate(50);
        let mut all = pending;
        all.extend(done);
        self.save(&all).await
    }
}
