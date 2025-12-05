use axum::{
    extract::{Path, Query, State},
    response::{Html, IntoResponse, Redirect, Response},
};
use oauth2::{
    basic::BasicClient,
    reqwest::async_http_client,
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, RedirectUrl, Scope,
    TokenResponse, TokenUrl,
};
use serde::Deserialize;
use std::sync::Arc;
use tracing::{error, info};
use url::Url;

use super::{error::AppError, state::AppState};
use crate::auth::{OAuthClient, OAuthConfig, TokenStore}; // Updated import

// Define state query parameter
#[derive(Debug, Deserialize)]
pub struct AuthState {
    pub state: String,
}

#[derive(Debug, Deserialize)]
pub struct AuthCode {
    code: String,
    state: String,
}

// OAuth start handler
pub async fn oauth_start(
    Path(provider): Path<String>,
    State(app_state): State<Arc<AppState>>,
) -> Result<Redirect, AppError> {
    info!("OAuth start initiated for provider: {}", provider);

    let config = app_state
        .config
        .read()
        .await
        .oauth
        .get(&provider)
        .cloned()
        .ok_or_else(|| AppError::RoutingError(format!("OAuth provider {} not found", provider)))?;

    let client = create_oauth_client(config, app_state.clone()).await?;

    let (authorize_url, csrf_state) = client
        .authorize_url(CsrfToken::new_random)
        .add_scope(Scope::new("openid".to_string()))
        .add_scope(Scope::new("email".to_string()))
        .add_scope(Scope::new("profile".to_string()))
        .add_extra_param("access_type", "offline") // Changed from add_extra_arg
        .add_extra_param("prompt", "consent")       // Changed from add_extra_arg
        .and_extra_query_param("provider", provider.clone()) // Add provider to query params
        .url();

    // Store the csrf_state for verification in the callback
    app_state
        .token_store
        .save_csrf_token(provider, csrf_state.secret())
        .await;

    Ok(Redirect::to(authorize_url.as_str()))
}

// OAuth callback handler
pub async fn oauth_callback(
    Query(AuthCode { code, state }): Query<AuthCode>,
    Query(AuthState { state: provider_state }): Query<AuthState>, // Extract provider_state
    State(app_state): State<Arc<AppState>>,
) -> Result<Html<String>, AppError> {
    info!("OAuth callback received");

    // Extract the provider from the provider_state (which actually holds the provider name)
    let provider = app_state
        .token_store
        .get_csrf_token_provider(&state)
        .await
        .ok_or_else(|| AppError::ParseError("Invalid or expired CSRF token".to_string()))?;

    let config = app_state
        .config
        .read()
        .await
        .oauth
        .get(&provider)
        .cloned()
        .ok_or_else(|| AppError::RoutingError(format!("OAuth provider {} not found", provider)))?;

    // Verify the CSRF state token
    let csrf_token = app_state
        .token_store
        .retrieve_csrf_token(&provider)
        .await
        .ok_or_else(|| AppError::ParseError("CSRF token not found or expired".to_string()))?;

    if csrf_token != state {
        return Err(AppError::ParseError("CSRF token mismatch".to_string()));
    }

    let client = create_oauth_client(config, app_state.clone()).await?;

    let token_result = client
        .exchange_code(AuthorizationCode::new(code))
        .request_async(async_http_client)
        .await
        .map_err(|e| AppError::ProviderError(format!("Failed to exchange code for token: {}", e)))?;

    let id_token = token_result
        .id_token()
        .ok_or_else(|| AppError::ProviderError("Server did not return an ID token".to_string()))?;
    let claims = id_token
        .claims(&client.id_token_verifier(), &[])
        .map_err(|e| AppError::ProviderError(format!("Failed to verify ID token: {}", e)))?;

    info!("Successfully authenticated user: {}", claims.subject().as_str());

    // Store the tokens securely
    app_state.token_store.save_tokens(token_result).await;

    Ok(Html("<h1>Successfully logged in!</h1>".to_string()))
}

// Generic login page (if needed)
pub async fn oauth_login() -> Html<String> {
    Html("<h1>Login Page</h1><p>Please select an OAuth provider.</p>".to_string())
}

// Generic logout handler
pub async fn oauth_logout(State(app_state): State<Arc<AppState>>) -> Redirect {
    app_state.token_store.clear_tokens().await;
    Redirect::to("/admin")
}

// Helper to create OAuth client
async fn create_oauth_client(config: OAuthConfig, app_state: Arc<AppState>) -> Result<BasicClient, AppError> { // Made async
    let client_id = ClientId::new(config.client_id);
    let client_secret = config.client_secret.map(ClientSecret::new); // Handle Option<String>
    let auth_url = AuthUrl::new(config.auth_url)
        .map_err(|e| AppError::ParseError(format!("Invalid AuthUrl: {}", e)))?;
    let token_url = TokenUrl::new(config.token_url)
        .map_err(|e| AppError::ParseError(format!("Invalid TokenUrl: {}", e)))?;

    let redirect_url = app_state
        .config
        .read()
        .await
        .server
        .public_url
        .join("/oauth/callback")
        .map_err(|e| AppError::ParseError(format!("Invalid redirect URL: {}", e)))?;
    let redirect_url = RedirectUrl::new(redirect_url.to_string())
        .map_err(|e| AppError::ParseError(format!("Invalid RedirectUrl: {}", e)))?;

    let client = BasicClient::new(client_id, client_secret, auth_url, Some(token_url)) // Pass Option<ClientSecret>
        .set_redirect_uri(redirect_url);

    Ok(client)
}
