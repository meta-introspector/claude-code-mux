use axum::{
    response::{IntoResponse, Response},
    http::StatusCode,
    Json,
};
use std::fmt::{self, Display};
use std::error::Error;

/// Application error types
#[derive(Debug)]
pub enum AppError {
    RoutingError(String),
    ParseError(String),
    ProviderError(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AppError::RoutingError(msg) => (StatusCode::BAD_REQUEST, msg),
            AppError::ParseError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
            AppError::ProviderError(msg) => (StatusCode::BAD_GATEWAY, msg),
        };

        let body = Json(serde_json::json!({
            "error": {
                "type": "error",
                "message": message
            }
        }));

        (status, body).into_response()
    }
}

impl Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::RoutingError(msg) => write!(f, "Routing error: {}", msg),
            AppError::ParseError(msg) => write!(f, "Parse error: {}", msg),
            AppError::ProviderError(msg) => write!(f, "Provider error: {}", msg),
        }
    }
}

impl Error for AppError {}