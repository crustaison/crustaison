// Runtime Module - Working State
//!
//! This module manages the runtime working state:
//! - memory.json: Working memory for current session
//! - heartbeat.json: Periodic task configuration
//! - run_logs/: Execution logs
//! - scheduled_tasks.json: Scheduled task queue

pub mod memory_json;
pub mod heartbeat;
pub mod run_logs;
pub mod checks;
pub mod scheduler;

pub use memory_json::{MemoryJson, WorkingMemory};
pub use heartbeat::{Heartbeat, HeartbeatRunner, HeartbeatStatus, HeartbeatConfig, EmailCheckConfig};
pub use run_logs::{RunLogs, LogEntry};
pub use checks::{Check, CheckResult, CheckStatus, default_checks};
pub use scheduler::{TaskQueue, ScheduledTask, TaskAction, TaskStatus};
