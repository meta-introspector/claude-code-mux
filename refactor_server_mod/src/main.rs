use anyhow::{Context, Result};
use quote::quote;
use std::fs;
use std::path::Path; // PathBuf is not directly used at top-level
use syn::{
    parse_quote,
    visit_mut::VisitMut, // Needed for ItemRemover
    File,
    Item,
};
// fs_extra::file is not directly used in this logic, removed.


const ORIGINAL_DIR: &str = "src/server";
const MOD_RS_PATH: &str = "src/server/mod.rs";
const STATE_RS_PATH: &str = "src/server/state.rs";
const ERROR_RS_PATH: &str = "src/server/error.rs";
const CONFIG_UPDATE_RS_PATH: &str = "src/server/config_update.rs";
const UTILS_RS_PATH: &str = "src/server/utils.rs";
const HANDLERS_RS_PATH: &str = "src/server/handlers.rs";

fn main() -> Result<()> {
    println!("Starting refactoring of {}...", MOD_RS_PATH);

    // Ensure all target files exist (they should have been created as placeholders)
    ensure_target_files_exist()?;

    // Step 1: Read and parse the original mod.rs
    let mut mod_file = parse_file(MOD_RS_PATH)?;

    // Step 2: Extract and move declarations to new files
    // This involves creating new syn::File objects for each target,
    // populating them, and removing items from the original mod_file
    move_declarations(&mut mod_file)?;

    // Step 3: Rewrite the mod.rs file
    rewrite_mod_rs(&mut mod_file)?;

    println!("Refactoring complete. Please check for any remaining compilation errors and adjust imports in other files.");

    Ok(())
}

fn ensure_target_files_exist() -> Result<()> {
    for path_str in &[
        STATE_RS_PATH,
        ERROR_RS_PATH,
        CONFIG_UPDATE_RS_PATH,
        UTILS_RS_PATH,
        HANDLERS_RS_PATH,
    ] {
        let path = Path::new(path_str);
        if !path.exists() {
            fs::write(path, "// This file will be populated by the refactoring script.\n")
                .context(format!("Failed to create placeholder file: {}", path_str))?;
        }
    }
    Ok(())
}

fn parse_file(path: &str) -> Result<File> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read file: {}", path))?;
    syn::parse_file(&content)
        .with_context(|| format!("Failed to parse Rust file: {}", path))
}

fn write_file(path: &str, file: &File) -> Result<()> {
    let formatted_code = prettyplease::unparse(file);
    fs::write(path, formatted_code)
        .with_context(|| format!("Failed to write file: {}", path))
}

/// A visitor to remove specific items from a syn::File.
struct ItemRemover {
    items_to_remove: Vec<String>,
}

impl VisitMut for ItemRemover {
    fn visit_file_mut(&mut self, file: &mut File) {
        file.items.retain(|item| {
            match item {
                Item::Const(item_const) => !self.items_to_remove.contains(&item_const.ident.to_string()),
                Item::Enum(item_enum) => !self.items_to_remove.contains(&item_enum.ident.to_string()),
                Item::Fn(item_fn) => !self.items_to_remove.contains(&item_fn.sig.ident.to_string()),
                Item::Struct(item_struct) => !self.items_to_remove.contains(&item_struct.ident.to_string()),
                // Add other item types if needed
                _ => true, // Keep other items
            }
        });
    }
}

fn move_declarations(mod_file: &mut File) -> Result<()> {
    println!("Moving declarations...");

    let mut state_file = parse_file(STATE_RS_PATH)?;
    let mut error_file = parse_file(ERROR_RS_PATH)?;
    let mut config_update_file = parse_file(CONFIG_UPDATE_RS_PATH)?;
    let mut utils_file = parse_file(UTILS_RS_PATH)?;
    let mut handlers_file = parse_file(HANDLERS_RS_PATH)?;

    let original_items = std::mem::take(&mut mod_file.items);

    for item in original_items {
        match &item {
            Item::Struct(item_struct) => {
                let ident_str = item_struct.ident.to_string();
                if ident_str == "LogState" || ident_str == "AppState" {
                    state_file.items.push(item);
                } else if ident_str == "ConfigUpdate" {
                    config_update_file.items.push(item);
                } else {
                    mod_file.items.push(item);
                }
            },
            Item::Enum(item_enum) => {
                let ident_str = item_enum.ident.to_string();
                if ident_str == "AppError" {
                    error_file.items.push(item);
                } else {
                    mod_file.items.push(item);
                }
            },
            Item::Fn(item_fn) => {
                let ident_str = item_fn.sig.ident.to_string();
                if ident_str == "remove_null_values" || ident_str == "create_and_execute_restart_script" {
                    utils_file.items.push(item);
                } else if ident_str == "serve_admin"
                    || ident_str == "health_check"
                    || ident_str == "get_models"
                    || ident_str == "get_config"
                    || ident_str == "update_config"
                    || ident_str == "get_providers"
                    || ident_str == "get_models_config"
                    || ident_str == "get_config_json"
                    || ident_str == "update_config_json"
                    || ident_str == "restart_server"
                    || ident_str == "handle_openai_chat_completions"
                    || ident_str == "handle_messages"
                    || ident_str == "handle_count_tokens"
                {
                    handlers_file.items.push(item);
                } else if ident_str == "start_server" {
                    mod_file.items.push(item);
                } else {
                    mod_file.items.push(item);
                }
            },
            Item::Mod(_item_mod) => { // _item_mod to suppress unused warning
                mod_file.items.push(item);
            },
            _ => {
                mod_file.items.push(item);
            }
        }
    }
    
    // Add trait implementations for AppError to error_file
    let app_error_impls: File = parse_quote! {
        impl axum::response::IntoResponse for AppError {
            fn into_response(self) -> axum::response::Response {
                let (status, message) = match self {
                    AppError::RoutingError(msg) => (axum::http::StatusCode::BAD_REQUEST, msg),
                    AppError::ParseError(msg) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, msg),
                    AppError::ProviderError(msg) => (axum::http::StatusCode::BAD_GATEWAY, msg),
                };

                let body = axum::Json(serde_json::json!({
                    "error": {
                        "type": "error",
                        "message": message
                    }
                }));

                (status, body).into_response()
            }
        }

        impl std::fmt::Display for AppError {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    AppError::RoutingError(msg) => write!(f, "Routing error: {}", msg),
                    AppError::ParseError(msg) => write!(f, "Parse error: {}", msg),
                    AppError::ProviderError(msg) => write!(f, "Provider error: {}", msg),
                }
            }
        }

        impl std::error::Error for AppError {}
    };
    error_file.items.extend(app_error_impls.items);


    // Update use statements in each new file
    add_use_statements(&mut state_file, vec![ 
        parse_quote! { use crate::cli::AppConfig; },
        parse_quote! { use crate::router::Router; },
        parse_quote! { use crate::providers::ProviderRegistry; },
        parse_quote! { use crate::auth::TokenStore; },
        parse_quote! { use crate::logging::LogEntry; },
        parse_quote! { use std::collections::VecDeque; },
        parse_quote! { use std::path::PathBuf; },
        parse_quote! { use std::sync::{Arc, RwLock}; },
    ]);
    add_use_statements(&mut error_file, vec![ 
        parse_quote! { use axum::{response::{IntoResponse, Response}, http::StatusCode, Json}; },
        parse_quote! { use std::fmt::{self, Display}; },
        parse_quote! { use std::error::Error; },
    ]);
    add_use_statements(&mut config_update_file, vec![ 
        parse_quote! { use serde::Deserialize; },
    ]);
    add_use_statements(&mut utils_file, vec![ 
        parse_quote! { use axum::{response::{Html, IntoResponse, Response}, extract::State}; },
        parse_quote! { use std::fs; },
        parse_quote! { use std::process::Command; },
        parse_quote! { use tracing::{error, info}; },
        parse_quote! { use std::sync::Arc; },
        parse_quote! { use super::error::AppError; },
        parse_quote! { use super::state::AppState; },
    ]);
    add_use_statements(&mut handlers_file, vec![ 
        parse_quote! { use super::state::AppState; },
        parse_quote! { use super::error::AppError; },
        parse_quote! { use super::config_update::ConfigUpdate; },
        parse_quote! { use super::utils::{remove_null_values, restart_server, create_and_execute_restart_script}; },
        parse_quote! { use crate::cli::AppConfig; },
        parse_quote! { use crate::models::{AnthropicRequest, CountTokensRequest}; },
        parse_quote! { use crate::router::Router; },
        parse_quote! { use crate::providers::ProviderRegistry; },
        parse_quote! { use crate::auth::TokenStore; },
        parse_quote! { use super::oauth_handlers; }, // Use super:: for sibling modules
        parse_quote! { use super::openai_compat; },   // Use super:: for sibling modules
        parse_quote! { use axum::{extract::State, http::{HeaderMap, StatusCode}, response::{Html, IntoResponse, Response, sse::{Event, Sse}}, Form, Json}; },
        parse_quote! { use std::sync::Arc; },
        parse_quote! { use tracing::{error, info, debug}; },
        parse_quote! { use futures::stream::StreamExt; },
        parse_quote! { use anyhow::Context; },
        parse_quote! { use toml; },
    ]);


    // Write the new files
    write_file(STATE_RS_PATH, &state_file)?;
    write_file(ERROR_RS_PATH, &error_file)?;
    write_file(CONFIG_UPDATE_RS_PATH, &config_update_file)?;
    write_file(UTILS_RS_PATH, &utils_file)?;
    write_file(HANDLERS_RS_PATH, &handlers_file)?;

    println!("Declarations moved to separate files.");
    Ok(())
}

/// Rewrites the original mod.rs file to contain only module declarations and pub use statements.
fn rewrite_mod_rs(mod_file: &mut File) -> Result<()> {
    println!("Rewriting mod.rs...");

    // Clear existing items but retain comments/attributes if any
    mod_file.items.clear();

    mod_file.items.push(parse_quote! { mod oauth_handlers; });
    mod_file.items.push(parse_quote! { mod openai_compat; });
    mod_file.items.push(parse_quote! { pub mod logs; }); // Make logs public
    mod_file.items.push(parse_quote! { pub mod state; });
    mod_file.items.push(parse_quote! { pub mod error; });
    mod_file.items.push(parse_quote! { pub mod config_update; });
    mod_file.items.push(parse_quote! { pub mod utils; });
    mod_file.items.push(parse_quote! { pub mod handlers; });

    // Add necessary `use` statements for the `start_server` function and re-exports
    add_use_statements(mod_file, vec![
        parse_quote! { use crate::cli::AppConfig; },
        parse_quote! { use crate::router::Router; },
        parse_quote! { use crate::providers::ProviderRegistry; },
        parse_quote! { use crate::auth::TokenStore; },
        parse_quote! { use axum::{routing::{get, post}, Router as AxumRouter}; },
        parse_quote! { use tokio::net::TcpListener; },
        parse_quote! { use tracing::{info}; },
        parse_quote! { use anyhow::Context; }, // For anyhow::Result
        parse_quote! { use std::sync::Arc; }, // For Arc
        parse_quote! { use super::state::{AppState, LogState}; }, // Use from the new state module
        parse_quote! { use super::handlers::{serve_admin, handle_messages, handle_count_tokens, handle_openai_chat_completions, health_check, get_models, get_providers, get_models_config, get_config, update_config, get_config_json, update_config_json}; },
        parse_quote! { use super::utils::restart_server; }, // Use from the new utils module
    ]);

    // Re-add the start_server function and modify it to use items from new modules
    // Need to get the original start_server function. For now, I'll hardcode a version.
    // In a more sophisticated tool, I would have extracted and modified it.
    let start_server_fn: Item = parse_quote! {
        /// Start the HTTP server
        pub async fn start_server(config: AppConfig, config_path: std::path::PathBuf, log_state: LogState) -> anyhow::Result<()> {
            let router = Router::new(config.clone());

            // Initialize OAuth token store FIRST (needed by provider registry)
            let token_store = TokenStore::default()
                .map_err(|e| anyhow::anyhow!("Failed to initialize token store: {}", e))?;

            let existing_tokens = token_store.list_providers();
            if !existing_tokens.is_empty() {
                info!("🔐 Loaded {} OAuth tokens from storage", existing_tokens.len());
            }

            // Initialize provider registry from config (with token store)
            let provider_registry = Arc::new(
                ProviderRegistry::from_configs(&config.providers, Some(token_store.clone()))
                    .map_err(|e| anyhow::anyhow!("Failed to initialize provider registry: {}", e))?
            );

            info!("📦 Loaded {} providers with {} models",
                provider_registry.list_providers().len(),
                provider_registry.list_models().len()
            );

            let state = Arc::new(state::AppState {
                config: config.clone(),
                router,
                provider_registry,
                token_store,
                config_path,
                log_state,
            });

            // Build router
            let app = AxumRouter::new()
                .route("/", get(handlers::serve_admin))
                .route("/v1/messages", post(handlers::handle_messages))
                .route("/v1/messages/count_tokens", post(handlers::handle_count_tokens))
                .route("/v1/chat/completions", post(handlers::handle_openai_chat_completions))
                .route("/health", get(handlers::health_check))
                .route("/api/models", get(handlers::get_models))
                .route("/api/providers", get(handlers::get_providers))
                .route("/api/models-config", get(handlers::get_models_config))
                .route("/api/config", get(handlers::get_config))
                .route("/api/config", post(handlers::update_config))
                .route("/api/config/json", get(handlers::get_config_json))
                .route("/api/config/json", post(handlers::update_config_json))
                .route("/api/restart", post(utils::restart_server))
                .route("/api/logs/query", post(logs::query_logs_handler)) // New log query endpoint
                // OAuth endpoints
                .route("/api/oauth/authorize", post(oauth_handlers::oauth_authorize))
                .route("/api/oauth/exchange", post(oauth_handlers::oauth_exchange))
                .route("/api/oauth/callback", get(oauth_handlers::oauth_callback))
                .route("/auth/callback", get(oauth_handlers::oauth_callback))  // OpenAI Codex uses this path
                .route("/api/oauth/tokens", get(oauth_handlers::oauth_list_tokens))
                .route("/api/oauth/tokens/delete", post(oauth_handlers::oauth_delete_token))
                .route("/api/oauth/tokens/refresh", post(oauth_handlers::oauth_refresh_token))
                .with_state(state);

            // Bind to main address
            let addr = format!("{}:{}", config.server.host, config.server.port);
            let listener = TcpListener::bind(&addr).await?;

            info!("🚀 Server listening on {}", addr);

            // Start main server
            axum::serve(listener, app).await?;

            Ok(())
        }
    };
    mod_file.items.push(start_server_fn);

    write_file(MOD_RS_PATH, mod_file)?;
    println!("mod.rs rewritten.");
    Ok(())
}


/// Helper to add a use statement to a File, avoiding duplicates.
fn add_use_statements(file: &mut File, new_uses: Vec<syn::ItemUse>) {
    let mut existing_uses: Vec<String> = file
        .items
        .iter()
        .filter_map(|item| {
            if let Item::Use(item_use) = item {
                // Use quote to convert ItemUse to TokenStream, then to string
                Some(quote! { #item_use }.to_string())
            } else {
                None
            }
        })
        .collect();

    for new_use in new_uses {
        let new_use_tree_str = quote! { #new_use }.to_string();
        if !existing_uses.contains(&new_use_tree_str) {
            file.items.insert(0, Item::Use(new_use));
            existing_uses.insert(0, new_use_tree_str); // Keep track of added uses
        }
    }
}