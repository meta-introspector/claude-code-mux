pub mod state;
pub mod error;
pub mod config_update;
pub mod handlers;
pub mod utils;
pub mod openai_compat;

use std::{net::SocketAddr, sync::Arc, path::PathBuf}; // Added PathBuf
use axum::{
    body::Body,
    extract::{Extension, State},
    http::{Request, StatusCode},
    middleware::{from_fn, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
// use axum_extra::headers::{UserAgent, TypedHeader}; // Commented out
use tokio::signal;
use tracing::{info, warn};
use tracing_subscriber::prelude::{*, __tracing_subscriber_SubscriberExt}; // Added this

use crate::{
    providers::{self},
    router::Router as AppRouter, // Aliased to AppRouter
};

use self::{
    handlers::{
        get_config_json, get_models, get_models_config, get_providers, health_check,
        serve_admin, update_config, update_config_json, handle_openai_chat_completions,
    },
    openai_compat::{
        open_ai_compat_completions, open_ai_compat_models,
    },
    state::{AppState, LogState}, // Added LogState
};

use mcp_oauth_plugin::handlers as oauth_plugin_handlers; // Added plugin handlers import

pub async fn start_server(config: crate::config::AppConfig, config_path: PathBuf, log_state: LogState) -> Result<(), anyhow::Error> {
    // Check for "RUST_LOG" environment variable
    if std::env::var("RUST_LOG").is_err() {
        // If not set, set a default level
        std::env::set_var("RUST_LOG", "info");
    }

    // Initialize tracing with a subscriber that can be reloaded
    // Replaced telemetry::build_reloadable_tracing_layer() with setup from main.rs
    let filter =
        tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Starting server...");
    let config = crate::config::AppConfig::from_file(&config_path)?;
    let listen_port = config.server.port;

    let app_state = Arc::new(AppState::new(config, log_state, config_path.clone()).await?);

    // Initial check for providers to enable/disable routes
    let has_openai_provider = app_state
        .config
        .read()
        .await
        .providers
        .iter()
        .any(|p| p.provider_type == "openai");
    let has_anthropic_provider = app_state
        .config
        .read()
        .await
        .providers
        .iter()
        .any(|p| p.provider_type == "anthropic");

    let app = Router::new()
        .route("/", get(handlers::root))
        .route("/health", get(health_check))
        // Admin
        .route("/admin", get(serve_admin))
        .route("/api/config", get(handlers::get_config).post(update_config))
        .route("/api/config_json", get(get_config_json).post(update_config_json))
        .route("/api/models", get(get_models))
        .route("/api/models_config", get(get_models_config))
        .route("/api/providers", get(get_providers))
        .route("/api/restart", post(handlers::restart_server))
        .route("/api/shutdown", post(shutdown_server))
        // OAuth routes
        .route("/oauth/start/:provider", get(oauth_plugin_handlers::oauth_start))
        .route("/oauth/callback", get(oauth_plugin_handlers::oauth_callback))
        .route("/oauth/login", get(oauth_plugin_handlers::oauth_login))
        .route("/oauth/logout", get(oauth_plugin_handlers::oauth_logout))
        // OpenAI Compatible API
        .route("/v1/chat/completions", post(handle_openai_chat_completions))
        .route("/chat/completions", post(handle_openai_chat_completions)) // Changed this
        .route("/v1/models", get(open_ai_compat_models))
        .route("/models", get(get_models))
        .route("/completions", post(open_ai_compat_completions))
        .route("/messages", post(handle_openai_chat_completions)) // Changed this
        // Pass the router by extension
        .layer(Extension(app_state.router.clone()))

        // .layer(axum::middleware::from_fn_with_state( // Commented out
        //     app_state.clone(),
        //     handle_headers_middleware,
        // ))
        .with_state(app_state.clone());

    let addr = SocketAddr::from(([0, 0, 0, 0], listen_port));
    info!("listening on http://{}", addr);

    // Replaced axum::Server::bind with axum::serve for newer axum compatibility
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app.into_make_service())
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

/* // Commented out handle_headers_middleware function
async fn handle_headers_middleware(
    State(app_state): State<Arc<AppState>>,
    TypedHeader(user_agent): TypedHeader<UserAgent>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, AppError> {
    let mut app_config = app_state.config.write().await;
    app_config.user_agent = Some(user_agent.to_string());
    drop(app_config); // Drop the lock as soon as possible

    Ok(next.run(request).await)
}
*/

async fn shutdown_server(State(_app_state): State<Arc<AppState>>) -> impl IntoResponse {
    info!("Shutting down server...");
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        std::process::exit(0);
    });

    (
        StatusCode::OK,
        Html("<div class='px-4 py-3 rounded-xl bg-primary/20 border border-primary/50 text-foreground text-sm'>✅ Server shutting down...</div>".to_string())
    )
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    info!("signal received, starting graceful shutdown");
}