use anyhow::Result;
use futures::stream::StreamExt;
use std::time::Duration;
use claude_code_mux::{
    cli::{AppConfig, ServerConfig},
    logging::SseTracingLayer,
    server::LogState,
};
use tracing_subscriber::prelude::*;

// Helper function to spawn the server in the background
async fn spawn_app() -> String {
    // Use a random port to avoid conflicts in parallel tests
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server_addr = format!("http://127.0.0.1:{}", port);

    // Drop the listener to free up the port for the server
    drop(listener);

    // We need to build the full logging and app state, similar to main.rs
    let (log_sender, _) = tokio::sync::broadcast::channel::<String>(100);
    let sse_layer = SseTracingLayer::new(log_sender.clone());
    let filter = tracing_subscriber::EnvFilter::new("info,tower_http=debug");

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .with(sse_layer)
        .init();

    let log_state = LogState {
        log_broadcast_sender: log_sender,
    };

    // Create a default config for testing purposes
    let config = AppConfig {
        server: ServerConfig {
            host: "127.0.0.1".to_string(),
            port,
            ..Default::default()
        },
        ..Default::default()
    };
    let config_path = std::path::PathBuf::from("config/default.toml");

    tokio::spawn(async move {
        claude_code_mux::server::start_server(config, config_path, log_state)
            .await
            .expect("Failed to start server");
    });

    // Give the server a moment to start
    tokio::time::sleep(Duration::from_millis(200)).await;

    server_addr
}

#[tokio::test]
async fn log_stream_produces_events() -> Result<()> {
    // Arrange: Start the server and get its address
    let server_addr = spawn_app().await;
    let client = reqwest::Client::new();

    // Act: Connect to the SSE stream
    let mut stream = client
        .get(format!("{}/api/logs/stream", server_addr))
        .send()
        .await?
        .bytes_stream();

    // Make a request to a different endpoint to generate a log
    let resp = client.get(format!("{}/health", server_addr)).send().await?;
    assert!(resp.status().is_success());

    // Assert: Check if we receive the corresponding log event
    let mut received_log = false;

    // Timeout for the test
    let test_timeout = Duration::from_secs(5);

    let result = tokio::time::timeout(test_timeout, async {
        while let Some(item) = stream.next().await {
            let chunk = item.unwrap();
            let line = String::from_utf8_lossy(&chunk);
            // The health check is logged by `tower-http`
            if line.contains("GET /health") && line.contains("200 OK") {
                received_log = true;
                break;
            }
        }
    })
    .await;

    assert!(result.is_ok(), "Test timed out waiting for log message");
    assert!(
        received_log,
        "Did not receive the expected log message from the SSE stream"
    );

    Ok(())
}

