use crate::logging::LogEntry;
use crate::server::{AppState, AppError};
use axum::{extract::State, Json};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct LogQuery {
    pub level: Option<String>,
    pub search_term: Option<String>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct LogQueryResponse {
    pub logs: Vec<LogEntry>,
}

pub async fn query_logs_handler(
    State(state): State<Arc<AppState>>,
    Json(query): Json<LogQuery>,
) -> Result<Json<LogQueryResponse>, AppError> {
    let buffer = state.log_state.log_buffer.read().map_err(|_| {
        AppError::ParseError("Failed to acquire read lock on log buffer".to_string())
    })?;

    let logs: Vec<LogEntry> = buffer
        .iter()
        .filter(|entry| {
            let level_match = query
                .level
                .as_ref()
                .map_or(true, |level| entry.level.eq_ignore_ascii_case(level));
            let search_match = query.search_term.as_ref().map_or(true, |term| {
                entry.message.contains(term) || entry.target.contains(term)
            });
            let start_match = query
                .start_time
                .map_or(true, |start| entry.timestamp >= start);
            let end_match = query.end_time.map_or(true, |end| entry.timestamp <= end);

            level_match && search_match && start_match && end_match
        })
        .cloned()
        .rev() // Show most recent logs first
        .take(query.limit.unwrap_or(100))
        .collect();

    Ok(Json(LogQueryResponse { logs }))
}