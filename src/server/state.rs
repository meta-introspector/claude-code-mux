use crate::config::AppConfig;
use crate::router::Router;
use crate::providers::ProviderRegistry;
use crate::logging::LogEntry;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use mcp_oauth_plugin::token_store::TokenStore as PluginTokenStore; // Renamed to avoid conflict
use mcp_oauth_plugin::oauth::OAuthConfig;
use mcp_oauth_plugin::handlers::PluginAppState; // Ensure this is available, even if just for context
use std::collections::HashMap;
use url::Url;

/// State for logging, including the in-memory buffer.
#[derive(Clone)]
pub struct LogState {
    pub log_buffer: Arc<tokio::sync::RwLock<VecDeque<LogEntry>>>,
    pub log_file_path: String,
}

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<tokio::sync::RwLock<AppConfig>>,
    pub router: Router,
    pub provider_registry: Arc<ProviderRegistry>,
    pub token_store: PluginTokenStore, // Updated type
    pub config_path: PathBuf,
    pub log_state: LogState,
    pub plugin_oauth_configs: Arc<tokio::sync::RwLock<HashMap<String, OAuthConfig>>>, // Added
    pub plugin_public_url: Url, // Added
    pub oauth_plugin_state: Arc<PluginAppState>, // Added
}
impl AppState {
    pub async fn new(app_config: crate::config::AppConfig, log_state: LogState, config_path: PathBuf) -> anyhow::Result<Self> {
        let config_arc = Arc::new(tokio::sync::RwLock::new(app_config.clone()));

        // Create TokenStore (from plugin)
        let token_store = PluginTokenStore::default()?;

        // Create ProviderRegistry
        let provider_registry = Arc::new(ProviderRegistry::new_from_app_state_deps(
            config_arc.clone(),
            token_store.clone(),
        ).await?);

        // Create Router
        let router = Router::new(app_config.clone()); // Pass app_config directly, not the Arc<RwLock>

        // Create PluginAppState for OAuth handlers
        let plugin_oauth_configs = Arc::new(tokio::sync::RwLock::new(app_config.oauth.clone()));
        let plugin_public_url = app_config.server.public_url.clone();

        let oauth_plugin_state = Arc::new(PluginAppState {
            token_store: token_store.clone(),
            oauth_configs: plugin_oauth_configs.clone(),
            public_url: plugin_public_url.clone(),
        });

        Ok(Self {
            config: config_arc, // Use the Arc<RwLock> for the shared config
            router,
            provider_registry,
            token_store, // TokenStore is now from plugin
            config_path, // Use the passed config_path
            log_state,
            plugin_oauth_configs, // Added
            plugin_public_url,    // Added
            oauth_plugin_state, // Added
        })
    }
}

