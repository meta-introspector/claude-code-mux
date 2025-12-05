use crate::cli::AppConfig;
use crate::router::Router;
use crate::providers::ProviderRegistry;
use crate::auth::TokenStore;
use crate::logging::LogEntry;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock; // Changed from std::sync::RwLock

/// State for logging, including the in-memory buffer.
#[derive(Clone)]
pub struct LogState {
    pub log_buffer: Arc<tokio::sync::RwLock<VecDeque<LogEntry>>>, // Changed to tokio::sync::RwLock
    pub log_file_path: String,
}

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<tokio::sync::RwLock<AppConfig>>, // Changed to Arc<tokio::sync::RwLock<AppConfig>>
    pub router: Router,
    pub provider_registry: Arc<ProviderRegistry>,
    pub token_store: TokenStore,
    pub config_path: PathBuf,
    pub log_state: LogState,
}
