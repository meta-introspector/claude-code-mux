use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::auth::{OAuthClient, OAuthConfig, TokenStore};

use super::AppState;

/// Request to start OAuth authorization flow
#[derive(Debug, Deserialize)]
pub struct OAuthAuthorizeRequest {
    /// Type of OAuth flow: "max" (Claude Pro/Max) or "console" (API key creation)
    #[serde(default = "default_oauth_type")]
    pub oauth_type: String,
}

fn default_oauth_type() -> String {
    "max".to_string()
}

/// Response with authorization URL
#[derive(Debug, Serialize)]
pub struct OAuthAuthorizeResponse {
    /// Authorization URL for user to visit
    pub url: String,
    /// PKCE verifier (store this for exchange step)
    pub verifier: String,
    /// Instructions for the user
    pub instructions: String,
}

/// Request to exchange authorization code for tokens
#[derive(Debug, Deserialize)]
pub struct OAuthExchangeRequest {
    /// Authorization code from OAuth callback
    pub code: String,
    /// PKCE verifier from authorize step
    pub verifier: String,
    /// Provider ID to store token under
    pub provider_id: String,
}

/// Response after successful token exchange
#[derive(Debug, Serialize)]
pub struct OAuthExchangeResponse {
    /// Success status
    pub success: bool,
    /// Message
    pub message: String,
    /// Provider ID
    pub provider_id: String,
    /// Token expiration timestamp (ISO 8601)
    pub expires_at: String,
}

/// Token information for listing
#[derive(Debug, Serialize)]
pub struct TokenInfo {
    pub provider_id: String,
    pub expires_at: String,
    pub is_expired: bool,
    pub needs_refresh: bool,
}

/// Get authorization URL
pub async fn oauth_authorize(
    State(state): State<Arc<AppState>>,
    Json(req): Json<OAuthAuthorizeRequest>,
) -> Result<Json<OAuthAuthorizeResponse>, (StatusCode, String)> {
    // Create OAuth config based on type
    let config = match req.oauth_type.as_str() {
        "max" => OAuthConfig::anthropic(),
        "console" => OAuthConfig::anthropic_console(),
        _ => return Err((
            StatusCode::BAD_REQUEST,
            "Invalid oauth_type. Must be 'max' or 'console'".to_string()
        )),
    };

    let oauth_client = OAuthClient::new(config, state.token_store.clone());
    let auth_url = oauth_client.get_authorization_url();

    let instructions = match req.oauth_type.as_str() {
        "max" => "Visit the URL above to authorize with your Claude Pro/Max account. After authorization, you'll receive a code. Paste it in the next step.".to_string(),
        "console" => "Visit the URL above to authorize and create an API key. After authorization, you'll receive a code. Paste it in the next step.".to_string(),
        _ => String::new(),
    };

    Ok(Json(OAuthAuthorizeResponse {
        url: auth_url.url,
        verifier: auth_url.verifier.verifier,
        instructions,
    }))
}

/// Exchange authorization code for tokens
pub async fn oauth_exchange(
    State(state): State<Arc<AppState>>,
    Json(req): Json<OAuthExchangeRequest>,
) -> Result<Json<OAuthExchangeResponse>, (StatusCode, String)> {
    let config = OAuthConfig::anthropic();
    let oauth_client = OAuthClient::new(config, state.token_store.clone());

    // Exchange code for tokens
    let token = oauth_client
        .exchange_code(&req.code, &req.verifier, &req.provider_id)
        .await
        .map_err(|e| (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to exchange code: {}", e)
        ))?;

    Ok(Json(OAuthExchangeResponse {
        success: true,
        message: "OAuth authentication successful! Token saved.".to_string(),
        provider_id: req.provider_id,
        expires_at: token.expires_at.to_rfc3339(),
    }))
}

/// List all OAuth tokens
pub async fn oauth_list_tokens(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<TokenInfo>>, (StatusCode, String)> {
    let all_tokens = state.token_store.all();

    let token_infos: Vec<TokenInfo> = all_tokens
        .into_iter()
        .map(|(_, token)| TokenInfo {
            provider_id: token.provider_id.clone(),
            expires_at: token.expires_at.to_rfc3339(),
            is_expired: token.is_expired(),
            needs_refresh: token.needs_refresh(),
        })
        .collect();

    Ok(Json(token_infos))
}

/// Delete OAuth token
#[derive(Debug, Deserialize)]
pub struct DeleteTokenRequest {
    pub provider_id: String,
}

pub async fn oauth_delete_token(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DeleteTokenRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    state.token_store
        .remove(&req.provider_id)
        .map_err(|e| (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to delete token: {}", e)
        ))?;

    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("Token for '{}' deleted", req.provider_id),
    })))
}

/// Refresh a token manually (for testing/debugging)
pub async fn oauth_refresh_token(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DeleteTokenRequest>,
) -> Result<Json<OAuthExchangeResponse>, (StatusCode, String)> {
    let config = OAuthConfig::anthropic();
    let oauth_client = OAuthClient::new(config, state.token_store.clone());

    let token = oauth_client
        .refresh_token(&req.provider_id)
        .await
        .map_err(|e| (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to refresh token: {}", e)
        ))?;

    Ok(Json(OAuthExchangeResponse {
        success: true,
        message: "Token refreshed successfully".to_string(),
        provider_id: req.provider_id,
        expires_at: token.expires_at.to_rfc3339(),
    }))
}
