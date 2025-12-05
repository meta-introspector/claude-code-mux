use axum::{
    response::{Html, IntoResponse, Response},
    extract::State,
};
use std::fs;
use std::process::Command;
use tracing::{error, info};
use std::sync::Arc;

use super::error::AppError;
use super::state::AppState;


/// Remove null values from JSON (TOML doesn't support null)
pub fn remove_null_values(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            map.retain(|_, v| !v.is_null());
            for (_, v) in map.iter_mut() {
                remove_null_values(v);
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr.iter_mut() {
                remove_null_values(item);
            }
        }
        _ => {}
    }
}

/// Restart server automatically using shell script
pub async fn restart_server(State(state): State<Arc<AppState>>) -> Response {
    info!("🔄 Server restart requested via UI");

    let config_lock = state.config.read().await; // Acquire read lock
    let port = config_lock.server.port; // Access port through the lock

    // Create a shell script to handle restart
    match create_and_execute_restart_script(port) {
        Ok(_) => {
            info!("✅ Restart script initiated");

            let response = Html("<div class='px-4 py-3 rounded-xl bg-green-500/20 border border-green-500/50 text-foreground text-sm'><strong>✅ Server restarting...</strong><br/>Shutting down current instance and starting new one.</div>").into_response();

            // Shutdown current process after a short delay
            tokio::spawn(async {
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                info!("Shutting down for restart...");
                std::process::exit(0);
            });

            response
        }
        Err(e) => {
            error!("Failed to initiate restart: {}", e);
            Html(format!("<div class='px-4 py-3 rounded-xl bg-red-500/20 border border-red-500/50 text-foreground text-sm'><strong>❌ Restart failed</strong><br/>Error: {}</div>", e)).into_response()
        }
    }
}

/// Create and execute a shell script that waits for shutdown and restarts
pub fn create_and_execute_restart_script(port: u16) -> std::io::Result<()> {
    use std::process::Command;
    use std::fs;

    // Get current executable path and PID
    let exe_path = std::env::current_exe()?;
    let current_pid = std::process::id();

    info!("Creating restart script for PID: {} on port: {}", current_pid, port);

    #[cfg(unix)]
    {
        // Create shell script
        let script_content = format!(
            r"#!/bin/bash
# Wait for old process to exit
while kill -0 {} 2>/dev/null; do
    sleep 0.1
done
# Start new server
{} start --port {} > /dev/null 2>&1 &
",
            current_pid,
            exe_path.display(),
            port
        );

        let script_path = "/tmp/ccm_restart.sh";
        fs::write(script_path, script_content)?;

        // Make executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(script_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(script_path, perms)?;
        }

        // Execute script in background
        Command::new("sh")
            .arg(script_path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;

        info!("Restart script started");
    }

    #[cfg(windows)]
    {
        // Create batch script for Windows
        let script_content = format!(
            r###"@echo off
:wait
tasklist /FI "PID eq {0}" 2>NUL | find /I /N "ccm.exe">NUL
if "%ERRORLEVEL%"=="0" (
    timeout /t 1 /nobreak > nul
    goto wait
)
start "" "{1}" start --port {2}"###
,
            current_pid,
            exe_path.display(),
            port
        );

        let script_path = std::env::temp_dir().join("ccm_restart.bat");
        fs::write(&script_path, script_content)?;

        // Execute batch file
        Command::new("cmd")
            .args(&["/C", "start", "/B", script_path.to_str().unwrap()])
            .spawn()?;
    }

    Ok(())
}
