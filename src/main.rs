//! Crustaison - Self-improving AI Agent
//!
//! Architecture:
//! - /authority: Immutable safety layer (auth, policy, execution)
//! - /cognition: Self-improving layer (planning, memory, reflection)
//! - /doctrine: Identity and rules (soul.md, agents.md, principles.md)
//! - /runtime: Working state (memory.json, heartbeat.json, run_logs)
//! - /ledger: Immutable audit trail (git-backed)

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::cmp::min;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use warp::Filter;

mod config;
mod authority;
mod cognition;
mod ledger;
mod runtime;
mod telegram;
mod providers;
mod agent;
mod tools;
mod tui;
mod sessions;
mod memory;
mod cli;
mod vector;
mod rag;
mod plugins;
mod webhooks;

use authority::gateway::{Gateway, GatewayMessage, NormalizedMessage};
use authority::executor::{Executor, Command, ExecutionResult};
use cognition::{MemoryEngine, DoctrineLoader, Planner, Plan, Reflection, ReflectionEngine};
use ledger::GitLedger;
use runtime::{MemoryJson, WorkingMemory, RunLogs, HeartbeatRunner, HeartbeatStatus, HeartbeatConfig, TaskQueue};
use crate::runtime::Heartbeat;
use providers::MiniMaxProvider;
use providers::nexa::NexaProvider;
use agent::Agent;
use tools::{ToolRegistry, create_tool_registry, ScheduleTool, EmailTool, GitHubTool};
use tui::run_tui;
use sessions::SessionManager;
use cli::{config_commands, security_commands, edit_commands, plugin_commands};
use memory::MemoryManager;
use vector::{VectorStore, Embedder};
use rag::RAGEngine;
use plugins::{PluginManager, PluginState};
use webhooks::{WebhookServer, WebhookClient, OutboundWebhook};

#[derive(Parser, Debug)]
#[command(name = "crustaison")]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run the agent in TUI mode
    Tui,
    /// Run as REST API server
    Daemon { port: Option<u16> },
    /// Run as Telegram bot
    Telegram,
    /// Check configuration
    Check,
    /// Show version info
    Version,
    /// Memory commands
    Memory {
        #[command(subcommand)]
        action: Option<MemoryCommands>,
    },
    /// Session commands
    Sessions {
        #[command(subcommand)]
        action: Option<SessionCommands>,
    },
    /// Config management
    Config {
        #[command(subcommand)]
        action: Option<ConfigCommands>,
    },
    /// Security settings
    Security {
        #[command(subcommand)]
        action: Option<SecurityCommands>,
    },
    /// File operations
    Edit {
        #[command(subcommand)]
        action: Option<EditCommands>,
    },
    /// Plugin management
    Plugins {
        #[command(subcommand)]
        action: Option<PluginCommands>,
    },
    /// Webhook management
    Webhook {
        #[command(subcommand)]
        action: Option<WebhookCommands>,
    },
    /// RAG (Retrieval-Augmented Generation)
    RAG {
        #[command(subcommand)]
        action: Option<RAGCommands>,
    },
}

#[derive(Subcommand, Debug)]
enum MemoryCommands {
    /// Write to journal
    Journal { content: String },
    /// Read today's journal
    Today,
    /// List journal entries
    List,
    /// Save a named context
    Save { name: String, content: String },
    /// Load a context
    Load { name: String },
    /// List contexts
    Contexts,
    /// Delete a context
    Delete { name: String },
}

#[derive(Subcommand, Debug)]
enum SessionCommands {
    /// List sessions
    List,
    /// Show a session
    Show { id: String },
    /// Delete a session
    Delete { id: String },
    /// Clear messages in a session
    Clear { id: String },
}

#[derive(Subcommand, Debug)]
enum ConfigCommands {
    /// Show current configuration
    Show,
    /// Edit configuration
    Edit,
    /// Validate configuration
    Validate,
    /// Reset to defaults
    Reset,
}

#[derive(Subcommand, Debug)]
enum SecurityCommands {
    /// Show security policy
    Policy,
    /// Show security status
    Status,
    /// Update security policy
    Update { policy: String },
    /// Add blocked command
    Block { command: String },
}

#[derive(Subcommand, Debug)]
enum EditCommands {
    /// Edit a file
    File { path: String, line: Option<usize> },
    /// Read a file
    Read { path: String, lines: Option<usize> },
    /// Write a file
    Write { path: String, content: String },
    /// Append to a file
    Append { path: String, content: String },
    /// List directory
    Ls { path: String },
}

#[derive(Subcommand, Debug)]
enum PluginCommands {
    /// List installed plugins
    List,
    /// Install a plugin
    Install { name: String, source: Option<String> },
    /// Uninstall a plugin
    Uninstall { name: String },
    /// Enable a plugin
    Enable { name: String },
    /// Disable a plugin
    Disable { name: String },
}

#[derive(Subcommand, Debug)]
enum WebhookCommands {
    /// List configured webhooks
    List,
    /// Add an outbound webhook
    Add { name: String, url: String, events: Vec<String> },
    /// Remove a webhook
    Remove { name: String },
    /// Test a webhook URL
    Test { url: String },
    /// Start webhook server
    Server { port: Option<u16> },
}

#[derive(Subcommand, Debug)]
enum RAGCommands {
    /// Index a document
    Index { source: String, content: String },
    /// Search for relevant documents
    Search { query: String },
    /// Get RAG stats
    Stats,
    /// Build context for a query
    Context { query: String },
}

/// JSON response wrapper
#[derive(serde::Serialize)]
struct ApiResponse<T> {
    success: bool,
    data: Option<T>,
    error: Option<String>,
}

/// Chat request body
#[derive(serde::Deserialize)]
struct ChatRequest {
    message: String,
    source: String,
    metadata: Option<serde_json::Value>,
}

/// Execute request body
#[derive(serde::Deserialize)]
struct ExecuteRequest {
    command: String,
    parameters: Option<serde_json::Value>,
    context: Option<serde_json::Value>,
}

/// Memory store request
#[derive(serde::Deserialize)]
struct MemoryStoreRequest {
    key: String,
    value: serde_json::Value,
    record_type: Option<String>,
}

/// Memory store response
#[derive(serde::Serialize)]
struct MemoryStoreResponse {
    key: String,
}

/// Memory recall request
#[derive(serde::Deserialize)]
struct MemoryRecallRequest {
    key: String,
}

/// Health check endpoint
async fn health() -> Result<impl warp::Reply, warp::Rejection> {
    Ok(warp::reply::json(&ApiResponse::<()> {
        success: true,
        data: None,
        error: None,
    }))
}

/// Chat endpoint - processes messages through gateway
async fn chat(
    body: ChatRequest,
    gateway: std::sync::Arc<Gateway>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let message = GatewayMessage {
        raw: body.message,
        source: body.source,
        timestamp: chrono::Utc::now().timestamp_millis(),
        metadata: body.metadata.unwrap_or(serde_json::json!({})),
    };

    match gateway.process(message).await {
        Ok(normalized) => Ok(warp::reply::json(&ApiResponse::<NormalizedMessage> {
            success: true,
            data: Some(normalized),
            error: None,
        })),
        Err(e) => Ok(warp::reply::json(&ApiResponse::<()> {
            success: false,
            data: None,
            error: Some(e),
        })),
    }
}

/// Execute endpoint - executes commands through executor
async fn execute(
    body: ExecuteRequest,
    executor: std::sync::Arc<Executor>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let command = Command {
        name: body.command,
        parameters: body.parameters.unwrap_or(serde_json::json!({})),
        context: body.context.unwrap_or(serde_json::json!({})),
    };

    match executor.execute(command).await {
        Ok(result) => Ok(warp::reply::json(&ApiResponse::<ExecutionResult> {
            success: true,
            data: Some(result),
            error: None,
        })),
        Err(e) => Ok(warp::reply::json(&ApiResponse::<()> {
            success: false,
            data: None,
            error: Some(e.to_string()),
        })),
    }
}

/// Memory store endpoint
async fn memory_store(
    body: MemoryStoreRequest,
    engine: std::sync::Arc<MemoryEngine>,
) -> Result<impl warp::Reply, warp::Rejection> {
    match engine.store(&body.key, &body.value, &body.record_type.unwrap_or_else(|| "default".to_string())).await {
        Ok(_) => {
            let response_data = MemoryStoreResponse { key: body.key };
            Ok(warp::reply::json(&ApiResponse::<MemoryStoreResponse> {
                success: true,
                data: Some(response_data),
                error: None,
            }))
        }
        Err(e) => Ok(warp::reply::json(&ApiResponse::<()> {
            success: false,
            data: None,
            error: Some(e.to_string()),
        })),
    }
}

/// Memory recall endpoint
async fn memory_recall(
    body: MemoryRecallRequest,
    engine: std::sync::Arc<MemoryEngine>,
) -> Result<impl warp::Reply, warp::Rejection> {
    match engine.recall(&body.key).await {
        Ok(record) => Ok(warp::reply::json(&ApiResponse::<cognition::MemoryRecord> {
            success: true,
            data: Some(record.unwrap_or_else(|| cognition::MemoryRecord {
                id: 0,
                key: body.key,
                value: serde_json::json!({}),
                record_type: "unknown".to_string(),
                created_at: 0,
                updated_at: 0,
            })),
            error: None,
        })),
        Err(e) => Ok(warp::reply::json(&ApiResponse::<()> {
            success: false,
            data: None,
            error: Some(e.to_string()),
        })),
    }
}

/// Memory list keys endpoint
async fn memory_list(
    engine: std::sync::Arc<MemoryEngine>,
) -> Result<impl warp::Reply, warp::Rejection> {
    match engine.list_keys().await {
        Ok(keys) => Ok(warp::reply::json(&ApiResponse::<Vec<String>> {
            success: true,
            data: Some(keys),
            error: None,
        })),
        Err(e) => Ok(warp::reply::json(&ApiResponse::<()> {
            success: false,
            data: None,
            error: Some(e.to_string()),
        })),
    }
}

/// Doctrine load endpoint
async fn doctrine(
    loader: std::sync::Arc<DoctrineLoader>,
) -> Result<impl warp::Reply, warp::Rejection> {
    match loader.load().await {
        Ok(doctrine) => Ok(warp::reply::json(&ApiResponse::<cognition::Doctrine> {
            success: true,
            data: Some(doctrine),
            error: None,
        })),
        Err(e) => Ok(warp::reply::json(&ApiResponse::<()> {
            success: false,
            data: None,
            error: Some(e.to_string()),
        })),
    }
}

/// Rate limit status endpoint
async fn rate_limit_status(
    source: String,
    gateway: std::sync::Arc<Gateway>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let remaining = gateway.get_rate_limit_status(&source).await;
    Ok(warp::reply::json(&ApiResponse::<serde_json::Value> {
        success: true,
        data: Some(serde_json::json!({
            "identity": source,
            "remaining": remaining
        })),
        error: None,
    }))
}

/// Execution log endpoint
async fn execution_log(
    executor: std::sync::Arc<Executor>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let log = executor.get_log().await;
    Ok(warp::reply::json(&ApiResponse::<Vec<authority::executor::PolicyResult>> {
        success: true,
        data: Some(log),
        error: None,
    }))
}

/// Planning request
#[derive(serde::Deserialize)]
struct PlanRequest {
    goal: String,
    context: Option<serde_json::Value>,
}

/// Planning endpoint
async fn plan(
    body: PlanRequest,
    planner: std::sync::Arc<tokio::sync::Mutex<Planner>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let mut planner = planner.lock().await;
    let context = body.context.unwrap_or_else(|| serde_json::json!({}));
    let context_map = if context.is_object() {
        serde_json::from_value(context).unwrap_or_else(|_| HashMap::new())
    } else {
        HashMap::new()
    };
    let result = planner.plan(&body.goal, &context_map).await;
    Ok(warp::reply::json(&ApiResponse::<cognition::PlanningResult> {
        success: true,
        data: Some(result),
        error: None,
    }))
}

/// Reflect request
#[derive(serde::Deserialize)]
struct ReflectRequest {
    events: Vec<HashMap<String, serde_json::Value>>,
}

/// Reflection endpoint
async fn reflection(
    body: ReflectRequest,
    engine: std::sync::Arc<tokio::sync::Mutex<ReflectionEngine>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let mut engine = engine.lock().await;
    let results = engine.reflect(&body.events).await;
    Ok(warp::reply::json(&ApiResponse::<Vec<cognition::Reflection>> {
        success: true,
        data: Some(results),
        error: None,
    }))
}

/// List reflections endpoint
async fn reflections(
    engine: std::sync::Arc<tokio::sync::Mutex<ReflectionEngine>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let engine = engine.lock().await;
    let all_reflections = engine.get_all().to_vec();
    Ok(warp::reply::json(&ApiResponse::<Vec<cognition::Reflection>> {
        success: true,
        data: Some(all_reflections),
        error: None,
    }))
}

/// Ledger add request
#[derive(serde::Deserialize)]
struct LedgerAddRequest {
    entry_type: String,
    content: serde_json::Value,
}

/// Ledger add endpoint
async fn ledger_add(
    body: LedgerAddRequest,
    ledger: std::sync::Arc<GitLedger>,
) -> Result<impl warp::Reply, warp::Rejection> {
    match ledger.add(&body.entry_type, &body.content).await {
        Ok(entry) => Ok(warp::reply::json(&ApiResponse::<ledger::LedgerEntry> {
            success: true,
            data: Some(entry),
            error: None,
        })),
        Err(e) => Ok(warp::reply::json(&ApiResponse::<()> {
            success: false,
            data: None,
            error: Some(e.to_string()),
        })),
    }
}

/// Working memory request
#[derive(serde::Deserialize)]
struct WorkingMemoryRequest {
    role: String,
    content: String,
}

/// Working memory add message endpoint
async fn memory_add_message(
    body: WorkingMemoryRequest,
    memory: std::sync::Arc<tokio::sync::Mutex<MemoryJson>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let mut memory = memory.lock().await;
    memory.add_message(&body.role, &body.content);
    let _ = memory.save().await;
    Ok(warp::reply::json(&ApiResponse::<()> {
        success: true,
        data: None,
        error: None,
    }))
}

/// Working memory get endpoint
async fn memory_get(
    memory: std::sync::Arc<tokio::sync::Mutex<MemoryJson>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let memory = memory.lock().await;
    let mem = memory.get().clone();
    Ok(warp::reply::json(&ApiResponse::<runtime::WorkingMemory> {
        success: true,
        data: Some(mem),
        error: None,
    }))
}

/// List run logs endpoint
async fn list_run_logs(
    _logs: std::sync::Arc<RunLogs>,
) -> Result<impl warp::Reply, warp::Rejection> {
    // Simplified - would list files in run_logs directory
    Ok(warp::reply::json(&ApiResponse::<Vec<String>> {
        success: true,
        data: Some(vec!["run_logs/".to_string()]),
        error: None,
    }))
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    // Load configuration
    let config = config::Config::load(None)?;

    // Initialize gateway
    let gateway = std::sync::Arc::new(Gateway::new(
        config.gateway.rate_limit_requests,
        config.gateway.rate_limit_window_seconds,
    ));

    // Initialize executor
    let executor = std::sync::Arc::new(Executor::new());

    // Initialize memory engine
    let memory_db_path = config.cognition.memory_db_path.to_string_lossy().to_string();
    let memory_engine = std::sync::Arc::new(
        MemoryEngine::new(&memory_db_path).await?
    );

    // Initialize doctrine loader
    let doctrine_loader = std::sync::Arc::new(
        DoctrineLoader::new(config.cognition.doctrine_path.clone())
    );

    // Initialize planner
    let planner = std::sync::Arc::new(tokio::sync::Mutex::new(Planner::new()));
    
    // Initialize reflection engine
    let reflection_engine = std::sync::Arc::new(tokio::sync::Mutex::new(ReflectionEngine::new()));

    // Initialize git ledger
    let git_ledger = std::sync::Arc::new(GitLedger::new(config.ledger.git_repo_path.clone()));
    let _ = git_ledger.init().await;

    // Initialize runtime components
    let working_memory = std::sync::Arc::new(tokio::sync::Mutex::new(MemoryJson::new(config.runtime.memory_json_path.clone())));
    let heartbeat = std::sync::Arc::new(Heartbeat::new(config.runtime.heartbeat_path.clone()));
    let run_logs_manager = std::sync::Arc::new(RunLogs::new(config.runtime.run_logs_path.clone()));
    let _ = run_logs_manager.init().await;

    // Initialize LLM provider and agent
    let api_key = config.cognition.api_key.clone()
        .or_else(|| std::env::var("CRUSTAISON_API_KEY").ok())
        .expect("No API key configured. Set cognition.api_key in config.toml or CRUSTAISON_API_KEY env var");

    let provider = MiniMaxProvider::new(
        api_key,
        config.cognition.model.clone(),
        config.cognition.base_url.clone(),
    );

    // Initialize tool registry
    let tool_registry = std::sync::Arc::new(create_tool_registry().await);
    
    // Get default chat_id from telegram config
    let default_chat_id = if !config.telegram.allowed_users.is_empty() {
        config.telegram.allowed_users[0] as i64
    } else {
        0
    };
    
    // Initialize task queue for scheduled tasks
    let task_queue_path = config.runtime.memory_json_path.parent()
        .unwrap_or(&std::path::PathBuf::from("~/.config/crustaison"))
        .join("scheduled_tasks.json");
    let task_queue = std::sync::Arc::new(TaskQueue::new(task_queue_path));
    
    // Register schedule tool with the tool registry (with correct chat_id)
    tool_registry.register(ScheduleTool::new(task_queue.clone(), default_chat_id)).await;

    // Register email tool if configured
    if let Some(ref email_config) = config.email {
        let email_tool_config = tools::email::EmailConfig {
            smtp_host: email_config.smtp_host.clone(),
            smtp_port: email_config.smtp_port,
            imap_host: email_config.imap_host.clone(),
            imap_port: email_config.imap_port,
            username: email_config.username.clone(),
            password: email_config.password.clone(),
            from_name: email_config.from_name.clone(),
        };
        tool_registry.register(EmailTool::new(email_tool_config)).await;
        tracing::info!("Email tool registered for {}", email_config.username);
    }

    // Register GitHub tool if configured
    if let Some(ref gh_config) = config.github {
        let github_tool_config = tools::github::GitHubConfig {
            username: gh_config.username.clone(),
            token: gh_config.token.clone(),
        };
        tool_registry.register(GitHubTool::new(github_tool_config)).await;
        tracing::info!("GitHub tool registered for {}", gh_config.username);
    }

    // Load script plugins from plugins directory
    let plugins_dir = config.cognition.doctrine_path.parent()
        .unwrap_or(&std::path::PathBuf::from("~/.config/crustaison"))
        .join("plugins");
    let plugins = tools::load_plugins(&plugins_dir);
    for plugin in plugins {
        tool_registry.register(plugin).await;
    }

    // Initialize session manager for persistence
    let session_db = config.cognition.memory_db_path.to_string_lossy().to_string();
    let session_manager = match SessionManager::new(&session_db).await {
        Ok(sm) => Some(std::sync::Arc::new(sm)),
        Err(e) => {
            tracing::warn!("Failed to initialize session manager: {}", e);
            None
        }
    };
    
    // Create or get default session
    let mut default_session_id = None;
    if let Some(ref sm) = session_manager {
        let sessions = sm.list_sessions().await.unwrap_or_default();
        if let Some(first) = sessions.first() {
            default_session_id = Some(first.id.clone());
        } else {
            // Create default session
            match sm.create_session("default").await {
                Ok(sess) => default_session_id = Some(sess.id),
                Err(e) => tracing::warn!("Failed to create default session: {}", e),
            }
        }
    }
    
    // Initialize agent with LLM, tools, executor, ledger, and session manager
    let agent = std::sync::Arc::new(tokio::sync::Mutex::new(
        Agent::with_session_manager(
            provider, 
            doctrine_loader.as_ref().clone(), 
            Some(tool_registry.clone()),
            Some(executor.clone()),
            Some(git_ledger.clone()),
            session_manager,
            default_session_id,
        ).await
            .expect("Failed to initialize agent")
    ));

    let args = Args::parse();

    match args.command.unwrap_or(Commands::Tui) {
        Commands::Tui => {
            println!("🪐 Crustaison v0.1.0 - Terminal Interface");
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            println!("Gateway port: {}", config.gateway.port);
            println!("Cognition model: {}", config.cognition.model);
            println!("Doctrine: {}", config.cognition.doctrine_path.display());
            println!("Memory DB: {}", config.cognition.memory_db_path.display());
            println!("Memory: {}", config.runtime.memory_json_path.display());
            println!("Ledger: {}", config.ledger.git_repo_path.display());
            
            println!("\n[ Press Enter to start, Ctrl+C to exit ]");
            
            // Run TUI
            if let Err(e) = run_tui(agent.clone()).await {
                eprintln!("TUI error: {}", e);
            }
        }
        Commands::Daemon { port } => {
            let port = port.unwrap_or(config.gateway.port);
            let addr = SocketAddr::from(([0, 0, 0, 0], port));

            println!("🌐 Crustaison API server on http://{}", addr);

            let agent_chat = agent.clone();
            let gateway_rate = gateway.clone();
            let executor_exec = executor.clone();
            let executor_log = executor.clone();
            let engine_store = memory_engine.clone();
            let engine_recall = memory_engine.clone();
            let engine_list = memory_engine.clone();

            let doctrine_loader = doctrine_loader.clone();
            let heartbeat = heartbeat.clone();
            let _run_logs = run_logs_manager.clone(); // Unused, for future

            // Health check
            let health = warp::path!("health")
                .and(warp::get())
                .and_then(health);

            // Chat endpoint - uses LLM agent
            let agent_endpoint = agent.clone();
            let chat = warp::path!("chat")
                .and(warp::post())
                .and(warp::body::json())
                .and(warp::any().map(move || agent_endpoint.clone()))
                .and_then(|body: ChatRequest, agent: std::sync::Arc<tokio::sync::Mutex<Agent>>| async move {
                    let mut agent = agent.lock().await;
                    match agent.chat(&body.message).await {
                        Ok(response) => Ok::<_, warp::Rejection>(warp::reply::json(&ApiResponse::<serde_json::Value> {
                            success: true,
                            data: Some(serde_json::json!({
                                "response": response,
                                "source": body.source,
                            })),
                            error: None,
                        })),
                        Err(e) => Ok(warp::reply::json(&ApiResponse::<()> {
                            success: false,
                            data: None,
                            error: Some(e.to_string()),
                        })),
                    }
                });

            // Execute endpoint
            let execute = warp::path!("execute")
                .and(warp::post())
                .and(warp::body::json())
                .and(warp::any().map(move || executor_exec.clone()))
                .and_then(execute);

            // Memory endpoints
            let store_memory = warp::path!("memory" / "store")
                .and(warp::post())
                .and(warp::body::json())
                .and(warp::any().map(move || engine_store.clone()))
                .and_then(memory_store);

            let recall_memory = warp::path!("memory" / "recall")
                .and(warp::post())
                .and(warp::body::json())
                .and(warp::any().map(move || engine_recall.clone()))
                .and_then(memory_recall);

            let list_memory = warp::path!("memory" / "list")
                .and(warp::get())
                .and(warp::any().map(move || engine_list.clone()))
                .and_then(memory_list);

            // Doctrine endpoint
            let load_doctrine = warp::path!("doctrine")
                .and(warp::get())
                .and(warp::any().map(move || doctrine_loader.clone()))
                .and_then(doctrine);
            
            // Planning endpoint
            let planner_endpoint = planner.clone();
            let make_plan = warp::path!("plan")
                .and(warp::post())
                .and(warp::body::json())
                .and(warp::any().map(move || planner_endpoint.clone()))
                .and_then(plan);
            
            // Reflection endpoint
            let reflection_endpoint = reflection_engine.clone();
            let reflect = warp::path!("reflect")
                .and(warp::post())
                .and(warp::body::json())
                .and(warp::any().map(move || reflection_endpoint.clone()))
                .and_then(reflection);
            
            // Get reflections
            let reflections_list = warp::path!("reflections")
                .and(warp::get())
                .and(warp::any().map(move || reflection_engine.clone()))
                .and_then(reflections);
            
            // Ledger add endpoint
            let ledger_endpoint = git_ledger.clone();
            let ledger_add = warp::path!("ledger" / "add")
                .and(warp::post())
                .and(warp::body::json())
                .and(warp::any().map(move || ledger_endpoint.clone()))
                .and_then(ledger_add);
            
            // Rate limit check
            let rate_limit = warp::path!("rate-limit" / String)
                .and(warp::get())
                .and(warp::any().map(move || gateway_rate.clone()))
                .and_then(rate_limit_status);

            // Execution log
            let execution_log_route = warp::path!("log")
                .and(warp::get())
                .and(warp::any().map(move || executor_log.clone()))
                .and_then(execution_log);
            
            // Runtime endpoints
            let working_memory_clone = working_memory.clone();
            let mem_add = warp::path!("memory" / "add")
                .and(warp::post())
                .and(warp::body::json())
                .and(warp::any().map(move || working_memory_clone.clone()))
                .and_then(memory_add_message);
            
            let mem_get = warp::path!("memory" / "get")
                .and(warp::get())
                .and(warp::any().map(move || working_memory.clone()))
                .and_then(memory_get);
            
            let run_logs_endpoint = run_logs_manager.clone();
            let list_run_logs_route = warp::path!("run_logs")
                .and(warp::get())
                .and(warp::any().map(move || run_logs_endpoint.clone()))
                .and_then(list_run_logs);

            // Initialize session manager
            let session_db = config.cognition.memory_db_path.to_string_lossy().to_string();
            let session_manager = std::sync::Arc::new(tokio::sync::Mutex::new(
                SessionManager::new(&session_db).await?
            ));
            
            // Initialize memory manager
            let memory_path = config.runtime.memory_json_path.parent()
                .unwrap_or(&PathBuf::from("~/.config/crustaison"))
                .to_path_buf();
            let memory_manager = std::sync::Arc::new(MemoryManager::new(memory_path));

            // Routes
            let routes = health.or(chat).or(execute)
                .or(store_memory).or(recall_memory).or(list_memory)
                .or(load_doctrine).or(make_plan).or(reflect).or(reflections_list)
                .or(ledger_add)
                .or(mem_add).or(mem_get).or(list_run_logs_route)
                .or(rate_limit).or(execution_log_route);

            warp::serve(routes).run(addr).await;
        }
        Commands::Telegram => {
            println!("📱 Crustaison Telegram bot starting...");
            
            // Get bot token from env or config
            let bot_token = std::env::var("CRUSTAISON_TELEGRAM_TOKEN")
                .unwrap_or_else(|_| {
                    config.telegram.bot_token.clone()
                        .unwrap_or_else(|| "YOUR_BOT_TOKEN".to_string())
                });
            
            let allowed_users = if config.telegram.allowed_users.is_empty() {
                vec![7766171845] // Default: Sean's ID
            } else {
                config.telegram.allowed_users.clone()
            };
            
            println!("Bot token: {}...", &bot_token[..20.min(bot_token.len())]);
            
            // Start heartbeat with Nexa (local, free inference)
            let nexa_provider = NexaProvider::new(
                "localhost".to_string(),
                18181,
                "unsloth/Qwen3-1.7B-GGUF".to_string(),
            );
            
            let heartbeat_config = HeartbeatConfig::default();
            let (alert_tx, mut alert_rx) = tokio::sync::mpsc::channel::<String>(32);
            
            let mut heartbeat_runner = HeartbeatRunner::new(
                heartbeat_config,
                Some(nexa_provider),
                alert_tx,
            );
            
            // Set task queue and bot token for scheduled tasks
            heartbeat_runner.set_task_queue(task_queue.clone());
            heartbeat_runner.set_bot_token(bot_token.clone());
            heartbeat_runner.set_default_chat_id(allowed_users[0] as i64);

            // Set email config for inbox monitoring
            if let Some(ref email_config) = config.email {
                heartbeat_runner.set_email_config(runtime::heartbeat::EmailCheckConfig {
                    imap_host: email_config.imap_host.clone(),
                    imap_port: email_config.imap_port,
                    username: email_config.username.clone(),
                    password: email_config.password.clone(),
                });
                tracing::info!("Heartbeat email monitoring enabled for {}", email_config.username);
            }
            
            // Spawn heartbeat monitoring loop
            tokio::spawn(async move {
                println!("💓 Heartbeat started (Nexa watchdog, 5 min interval)");
                heartbeat_runner.start().await;
            });
            
            // Spawn alert forwarder to Telegram
            let alert_bot_token = bot_token.clone();
            let alert_chat_id = allowed_users[0] as i64;
            tokio::spawn(async move {
                let client = reqwest::Client::new();
                while let Some(alert_msg) = alert_rx.recv().await {
                    let url = format!(
                        "https://api.telegram.org/bot{}/sendMessage",
                        alert_bot_token
                    );
                    let _ = client.post(&url)
                        .json(&serde_json::json!({
                            "chat_id": alert_chat_id,
                            "text": format!("🔔 {}", alert_msg),
                        }))
                        .send()
                        .await;
                }
            });
            
            println!("✅ Telegram bot + heartbeat ready");
            
            // Wire RAG engine to agent for semantic memory
            {
                let data_dir_rag = dirs::data_dir()
                    .unwrap_or_else(|| PathBuf::from("~/.local/share"))
                    .join("crustaison");
                let vector_path = data_dir_rag.join("vector_store");
                let vs = vector::VectorStore::new(vector_path);
                let embedder = vector::Embedder::new(
                    "http://localhost:18181".to_string(),
                    "Qwen/Qwen3-Embedding-0.6B-GGUF".to_string(),
                );
                let rag_cfg = rag::RAGConfig {
                    enabled: true,
                    max_context_docs: 3,
                    min_similarity: 0.4,
                    chunk_size: 1000,
                    chunk_overlap: 200,
                };
                let rag_engine = std::sync::Arc::new(tokio::sync::Mutex::new(
                    rag::RAGEngine::new(vs, embedder, rag_cfg)
                ));
                agent.lock().await.set_rag_engine(rag_engine);
                println!("🧠 RAG engine wired (Nexa Qwen3-Embedding)");
            }

            // Pass the agent to telegram
            telegram::run_telegram_bot(bot_token, agent.clone(), allowed_users).await;
        }
        Commands::Check => {
            println!("✓ Crustaison configuration OK");
            println!("  Gateway port: {}", config.gateway.port);
            println!("  Model: {}", config.cognition.model);
            println!("  Memory DB: {}", config.cognition.memory_db_path.display());
            println!("  Rate limit: {}/{}s",
                config.gateway.rate_limit_requests,
                config.gateway.rate_limit_window_seconds);
        }
        Commands::Version => {
            println!("🪐 Crustaison v0.1.0");
        }
        Commands::Memory { action } => {
            let memory = MemoryManager::new(config.runtime.memory_json_path.parent().unwrap_or(&PathBuf::from("~/.config/crustaison")).to_path_buf());
            
            match action {
                Some(MemoryCommands::Journal { content }) => {
                    match memory.journal_write(&content).await {
                        Ok(entry) => println!("Wrote to journal for {}", entry.date),
                        Err(e) => eprintln!("Error: {}", e),
                    }
                }
                Some(MemoryCommands::Today) => {
                    match memory.journal_read_today().await {
                        Ok(Some(entry)) => println!("{}", entry.content),
                        Ok(None) => println!("No journal entry for today"),
                        Err(e) => eprintln!("Error: {}", e),
                    }
                }
                Some(MemoryCommands::List) => {
                    match memory.journal_list().await {
                        Ok(entries) => {
                            println!("Journal entries ({} total):", entries.len());
                            for entry in entries.iter().take(10) {
                                println!("  - {}", entry);
                            }
                            if entries.len() > 10 {
                                println!("  ... and {} more", entries.len() - 10);
                            }
                        }
                        Err(e) => eprintln!("Error: {}", e),
                    }
                }
                Some(MemoryCommands::Save { name, content }) => {
                    match memory.context_save(&name, &content).await {
                        Ok(ctx) => println!("Saved context: {} ({})", ctx.name, ctx.content.len()),
                        Err(e) => eprintln!("Error: {}", e),
                    }
                }
                Some(MemoryCommands::Load { name }) => {
                    match memory.context_load(&name).await {
                        Ok(Some(ctx)) => println!("{}", ctx.content),
                        Ok(None) => println!("Context not found: {}", name),
                        Err(e) => eprintln!("Error: {}", e),
                    }
                }
                Some(MemoryCommands::Contexts) => {
                    match memory.context_list().await {
                        Ok(contexts) => {
                            if contexts.is_empty() {
                                println!("No contexts saved");
                            } else {
                                println!("Contexts ({}):", contexts.len());
                                for ctx in &contexts {
                                    println!("  - {}", ctx);
                                }
                            }
                        }
                        Err(e) => eprintln!("Error: {}", e),
                    }
                }
                Some(MemoryCommands::Delete { name }) => {
                    match memory.context_delete(&name).await {
                        Ok(_) => println!("Deleted context: {}", name),
                        Err(e) => eprintln!("Error: {}", e),
                    }
                }
                None => {
                    println!("Memory commands:");
                    println!("  /memory journal <content> - Write to journal");
                    println!("  /memory today - Read today's journal");
                    println!("  /memory list - List journal entries");
                    println!("  /memory save <name> <content> - Save context");
                    println!("  /memory load <name> - Load context");
                    println!("  /memory contexts - List contexts");
                    println!("  /memory delete <name> - Delete context");
                }
            }
        }
        Commands::Sessions { action } => {
            let session_db = config.cognition.memory_db_path.to_string_lossy().to_string();
            let manager = match SessionManager::new(&session_db).await {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("Failed to open session database: {}", e);
                    return Ok(());
                }
            };
            
            match action {
                Some(SessionCommands::List) => {
                    match manager.list_sessions().await {
                        Ok(sessions) => {
                            println!("Sessions ({} total):", sessions.len());
                            for sess in sessions.iter().take(10) {
                                println!("  {} - {} ({} msgs)", 
                                    &sess.id[..8], 
                                    sess.name, 
                                    sess.message_count);
                            }
                            if sessions.len() > 10 {
                                println!("  ... and {} more", sessions.len() - 10);
                            }
                        }
                        Err(e) => eprintln!("Error: {}", e),
                    }
                }
                Some(SessionCommands::Show { id }) => {
                    match manager.get_session(&id).await {
                        Ok(Some(session)) => {
                            println!("Session: {} ({})", session.id, session.name);
                            println!("Messages:");
                            match manager.get_messages(&id).await {
                                Ok(msgs) => {
                                    for msg in msgs {
                                        println!("  [{}] {}", msg.role, &msg.content[..min(80, msg.content.len())]);
                                    }
                                }
                                Err(e) => eprintln!("Error loading messages: {}", e),
                            }
                        }
                        Ok(None) => println!("Session not found: {}", id),
                        Err(e) => eprintln!("Error: {}", e),
                    }
                }
                Some(SessionCommands::Delete { id }) => {
                    match manager.delete_session(&id).await {
                        Ok(_) => println!("Deleted session: {}", id),
                        Err(e) => eprintln!("Error: {}", e),
                    }
                }
                Some(SessionCommands::Clear { id }) => {
                    match manager.clear_session(&id).await {
                        Ok(_) => println!("Cleared messages in session: {}", id),
                        Err(e) => eprintln!("Error: {}", e),
                    }
                }
                None => {
                    println!("Session commands:");
                    println!("  /sessions list - List sessions");
                    println!("  /sessions show <id> - Show session details");
                    println!("  /sessions delete <id> - Delete a session");
                    println!("  /sessions clear <id> - Clear messages in session");
                }
            }
        }
        Commands::Config { action } => {
            match action {
                Some(ConfigCommands::Show) => {
                    match config_commands::show() {
                        Ok(output) => println!("{}", output),
                        Err(e) => eprintln!("Error: {}", e),
                    }
                }
                Some(ConfigCommands::Edit) => {
                    match config_commands::edit() {
                        Ok(output) => println!("{}", output),
                        Err(e) => eprintln!("Error: {}", e),
                    }
                }
                Some(ConfigCommands::Validate) => {
                    match config_commands::validate() {
                        Ok(output) => println!("{}", output),
                        Err(e) => eprintln!("Error: {}", e),
                    }
                }
                Some(ConfigCommands::Reset) => {
                    match config_commands::reset() {
                        Ok(output) => println!("{}", output),
                        Err(e) => eprintln!("Error: {}", e),
                    }
                }
                None => {
                    println!("Config commands:");
                    println!("  /config show - Show current configuration");
                    println!("  /config edit - Edit configuration");
                    println!("  /config validate - Validate configuration");
                    println!("  /config reset - Reset to defaults");
                }
            }
        }
        Commands::Security { action } => {
            match action {
                Some(SecurityCommands::Policy) => {
                    match security_commands::show_policy() {
                        Ok(output) => println!("{}", output),
                        Err(e) => eprintln!("Error: {}", e),
                    }
                }
                Some(SecurityCommands::Status) => {
                    match security_commands::status() {
                        Ok(output) => println!("{}", output),
                        Err(e) => eprintln!("Error: {}", e),
                    }
                }
                Some(SecurityCommands::Update { policy }) => {
                    match security_commands::update_policy(&policy) {
                        Ok(output) => println!("{}", output),
                        Err(e) => eprintln!("Error: {}", e),
                    }
                }
                Some(SecurityCommands::Block { command }) => {
                    match security_commands::add_blocked(&command) {
                        Ok(output) => println!("{}", output),
                        Err(e) => eprintln!("Error: {}", e),
                    }
                }
                None => {
                    println!("Security commands:");
                    println!("  /security policy - Show security policy");
                    println!("  /security status - Show security status");
                    println!("  /security update <json> - Update policy");
                    println!("  /security block <command> - Add blocked command");
                }
            }
        }
        Commands::Edit { action } => {
            match action {
                Some(EditCommands::File { path, line }) => {
                    match edit_commands::edit_file(&path, line) {
                        Ok(output) => println!("{}", output),
                        Err(e) => eprintln!("Error: {}", e),
                    }
                }
                Some(EditCommands::Read { path, lines }) => {
                    match edit_commands::read_file(&path, lines) {
                        Ok(output) => println!("{}", output),
                        Err(e) => eprintln!("Error: {}", e),
                    }
                }
                Some(EditCommands::Write { path, content }) => {
                    match edit_commands::write_file(&path, &content) {
                        Ok(output) => println!("{}", output),
                        Err(e) => eprintln!("Error: {}", e),
                    }
                }
                Some(EditCommands::Append { path, content }) => {
                    match edit_commands::append_file(&path, &content) {
                        Ok(output) => println!("{}", output),
                        Err(e) => eprintln!("Error: {}", e),
                    }
                }
                Some(EditCommands::Ls { path }) => {
                    match edit_commands::list_dir(&path) {
                        Ok(output) => println!("{}", output),
                        Err(e) => eprintln!("Error: {}", e),
                    }
                }
                None => {
                    println!("Edit commands:");
                    println!("  /edit file <path> [line] - Edit a file");
                    println!("  /edit read <path> [lines] - Read a file");
                    println!("  /edit write <path> <content> - Write a file");
                    println!("  /edit append <path> <content> - Append to a file");
                    println!("  /edit ls <path> - List directory");
                }
            }
        }
        Commands::Plugins { action } => {
            match action {
                Some(PluginCommands::List) => {
                    match plugin_commands::list() {
                        Ok(output) => println!("{}", output),
                        Err(e) => eprintln!("Error: {}", e),
                    }
                }
                Some(PluginCommands::Install { name, source }) => {
                    match plugin_commands::install(&name, source.as_deref()) {
                        Ok(output) => println!("{}", output),
                        Err(e) => eprintln!("Error: {}", e),
                    }
                }
                Some(PluginCommands::Uninstall { name }) => {
                    match plugin_commands::uninstall(&name) {
                        Ok(output) => println!("{}", output),
                        Err(e) => eprintln!("Error: {}", e),
                    }
                }
                Some(PluginCommands::Enable { name }) => {
                    match plugin_commands::enable(&name) {
                        Ok(output) => println!("{}", output),
                        Err(e) => eprintln!("Error: {}", e),
                    }
                }
                Some(PluginCommands::Disable { name }) => {
                    match plugin_commands::disable(&name) {
                        Ok(output) => println!("{}", output),
                        Err(e) => eprintln!("Error: {}", e),
                    }
                }
                None => {
                    println!("Plugin commands:");
                    println!("  /plugins list - List installed plugins");
                    println!("  /plugins install <name> [source] - Install a plugin");
                    println!("  /plugins uninstall <name> - Uninstall a plugin");
                    println!("  /plugins enable <name> - Enable a plugin");
                    println!("  /plugins disable <name> - Disable a plugin");
                }
            }
        }
        Commands::Webhook { action } => {
            let data_dir = dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from("~/.local/share"))
                .join("crustaison");
            let webhooks_path = data_dir.join("webhooks.json");
            
            // Load existing webhooks
            let mut webhooks: Vec<OutboundWebhook> = if webhooks_path.exists() {
                let content = std::fs::read_to_string(&webhooks_path).unwrap_or_default();
                serde_json::from_str(&content).unwrap_or_default()
            } else {
                Vec::new()
            };
            
            match action {
                Some(WebhookCommands::List) => {
                    if webhooks.is_empty() {
                        println!("No webhooks configured.");
                    } else {
                        println!("Configured webhooks:");
                        for wh in &webhooks {
                            println!("  - {}: {} (events: {})", wh.name, wh.url, wh.events.join(", "));
                        }
                    }
                }
                Some(WebhookCommands::Add { name, url, events }) => {
                    webhooks.push(OutboundWebhook {
                        name,
                        url,
                        events,
                        headers: std::collections::HashMap::new(),
                        timeout_seconds: 30,
                    });
                    // Save webhooks
                    if let Ok(content) = serde_json::to_string_pretty(&webhooks) {
                        let _ = std::fs::write(&webhooks_path, content);
                    }
                    println!("Webhook added.");
                }
                Some(WebhookCommands::Remove { name }) => {
                    let original_len = webhooks.len();
                    webhooks.retain(|wh| wh.name != name);
                    if webhooks.len() < original_len {
                        if let Ok(content) = serde_json::to_string_pretty(&webhooks) {
                            let _ = std::fs::write(&webhooks_path, content);
                        }
                        println!("Webhook '{}' removed.", name);
                    } else {
                        println!("Webhook '{}' not found.", name);
                    }
                }
                Some(WebhookCommands::Test { url }) => {
                    println!("Testing webhook at: {}", url);
                    let client = WebhookClient::new(10);
                    match client.test(&url).await {
                        Ok(true) => println!("✓ Webhook is reachable!"),
                        Ok(false) => println!("✗ Webhook returned error status"),
                        Err(e) => println!("✗ Failed to connect: {}", e),
                    }
                }
                Some(WebhookCommands::Server { port }) => {
                    let port = port.unwrap_or(8080);
                    println!("Starting webhook server on port {}...", port);
                    println!("Note: Webhook server requires HTTP endpoint implementation.");
                }
                None => {
                    println!("Webhook commands:");
                    println!("  /webhook list - List configured webhooks");
                    println!("  /webhook add <name> <url> <events...> - Add webhook");
                    println!("  /webhook remove <name> - Remove webhook");
                    println!("  /webhook test <url> - Test webhook connectivity");
                    println!("  /webhook server [port] - Start webhook server");
                }
            }
        }
        Commands::RAG { action } => {
            // Initialize RAG components
            let data_dir = dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from("~/.local/share"))
                .join("crustaison");
            let vector_path = data_dir.join("vector_store");
            let mut vector_store = VectorStore::new(vector_path);
            let embedder = Embedder::new("http://localhost:18181".to_string(), "Qwen/Qwen3-Embedding-0.6B-GGUF".to_string());
            let rag_config = rag::RAGConfig {
                enabled: true,
                max_context_docs: 5,
                min_similarity: 0.3,
                chunk_size: 1000,
                chunk_overlap: 200,
            };
            let mut rag = RAGEngine::new(vector_store, embedder, rag_config);
            
            match action {
                Some(RAGCommands::Index { source, content }) => {
                    let ids = rag.index_document(&content, &source, Some(serde_json::json!({"source": source}))).await;
                    println!("Indexed {} chunks from {}", ids.len(), source);
                }
                Some(RAGCommands::Search { query }) => {
                    let results = rag.retrieve(&query, None).await;
                    println!("Found {} relevant documents:", results.len());
                    for doc in results.iter().take(5) {
                        println!("  - [{}] {}", 
                            doc.metadata.as_ref()
                                .and_then(|m| m.get("source").and_then(|s| s.as_str()))
                                .unwrap_or("unknown"),
                            &doc.text[..min(100, doc.text.len())]
                        );
                    }
                }
                Some(RAGCommands::Stats) => {
                    let stats = rag.stats();
                    println!("RAG Stats:");
                    println!("  Total documents: {}", stats.total_documents);
                    println!("  Max context docs: {}", stats.config.max_context_docs);
                    println!("  Min similarity: {}", stats.config.min_similarity);
                }
                Some(RAGCommands::Context { query }) => {
                    let context = rag.build_context(&query).await;
                    if context.is_empty() {
                        println!("No relevant context found.");
                    } else {
                        println!("{}", context);
                    }
                }
                None => {
                    println!("RAG commands:");
                    println!("  /rag index <source> <content> - Index a document");
                    println!("  /rag search <query> - Search for relevant documents");
                    println!("  /rag stats - Show RAG statistics");
                    println!("  /rag context <query> - Build context for a query");
                }
            }
        }
    }

    Ok(())
}
