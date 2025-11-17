use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};

use super::token_store::{OAuthToken, TokenStore};

/// PKCE verifier for OAuth flow
#[derive(Debug, Clone)]
pub struct PKCEVerifier {
    pub verifier: String,
    pub challenge: String,
}

impl PKCEVerifier {
    /// Generate a new PKCE code verifier and challenge
    pub fn generate() -> Self {
        // Generate random verifier (43-128 characters)
        let mut rng = rand::thread_rng();
        let random_bytes: Vec<u8> = (0..32).map(|_| rng.gen()).collect();
        let verifier = URL_SAFE_NO_PAD.encode(&random_bytes);

        // Generate challenge (SHA256 of verifier)
        let mut hasher = Sha256::new();
        hasher.update(verifier.as_bytes());
        let challenge_bytes = hasher.finalize();
        let challenge = URL_SAFE_NO_PAD.encode(&challenge_bytes);

        Self { verifier, challenge }
    }
}

/// Authorization URL with PKCE
#[derive(Debug, Clone)]
pub struct AuthorizationUrl {
    pub url: String,
    pub verifier: PKCEVerifier,
}

/// OAuth provider configuration
#[derive(Debug, Clone)]
pub struct OAuthConfig {
    pub client_id: String,
    pub auth_url: String,
    pub token_url: String,
    pub redirect_uri: String,
    pub scopes: Vec<String>,
}

impl OAuthConfig {
    /// Anthropic Claude Pro/Max OAuth configuration
    pub fn anthropic() -> Self {
        Self {
            client_id: "9d1c250a-e61b-44d9-88ed-5944d1962f5e".to_string(),
            auth_url: "https://claude.ai/oauth/authorize".to_string(),
            token_url: "https://console.anthropic.com/v1/oauth/token".to_string(),
            redirect_uri: "https://console.anthropic.com/oauth/code/callback".to_string(),
            scopes: vec![
                "org:create_api_key".to_string(),
                "user:profile".to_string(),
                "user:inference".to_string(),
            ],
        }
    }

    /// Anthropic Console (for API key creation)
    pub fn anthropic_console() -> Self {
        let mut config = Self::anthropic();
        config.auth_url = "https://console.anthropic.com/oauth/authorize".to_string();
        config
    }
}

/// OAuth client for handling authentication flows
pub struct OAuthClient {
    config: OAuthConfig,
    token_store: TokenStore,
    http_client: reqwest::Client,
}

impl OAuthClient {
    /// Create a new OAuth client
    pub fn new(config: OAuthConfig, token_store: TokenStore) -> Self {
        Self {
            config,
            token_store,
            http_client: reqwest::Client::new(),
        }
    }

    /// Generate authorization URL with PKCE
    pub fn get_authorization_url(&self) -> AuthorizationUrl {
        let pkce = PKCEVerifier::generate();

        let mut url = url::Url::parse(&self.config.auth_url)
            .expect("Invalid auth URL");

        url.query_pairs_mut()
            .append_pair("code", "true")
            .append_pair("client_id", &self.config.client_id)
            .append_pair("response_type", "code")
            .append_pair("redirect_uri", &self.config.redirect_uri)
            .append_pair("scope", &self.config.scopes.join(" "))
            .append_pair("code_challenge", &pkce.challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", &pkce.verifier);

        AuthorizationUrl {
            url: url.to_string(),
            verifier: pkce,
        }
    }

    /// Exchange authorization code for tokens
    pub async fn exchange_code(
        &self,
        code: &str,
        verifier: &str,
        provider_id: &str,
    ) -> Result<OAuthToken> {
        // Parse code (format: "code#state")
        let parts: Vec<&str> = code.split('#').collect();
        let auth_code = parts.get(0).context("Invalid code format")?;
        let state = parts.get(1).unwrap_or(&verifier);

        #[derive(Serialize)]
        struct TokenRequest {
            code: String,
            state: String,
            grant_type: String,
            client_id: String,
            redirect_uri: String,
            code_verifier: String,
        }

        let request = TokenRequest {
            code: auth_code.to_string(),
            state: state.to_string(),
            grant_type: "authorization_code".to_string(),
            client_id: self.config.client_id.clone(),
            redirect_uri: self.config.redirect_uri.clone(),
            code_verifier: verifier.to_string(),
        };

        #[derive(Deserialize)]
        struct TokenResponse {
            access_token: String,
            refresh_token: String,
            expires_in: i64,
        }

        let response = self.http_client
            .post(&self.config.token_url)
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .context("Failed to exchange code for token")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!("Token exchange failed: {} - {}", status, body));
        }

        let token_response: TokenResponse = response.json().await
            .context("Failed to parse token response")?;

        let expires_at = Utc::now() + chrono::Duration::seconds(token_response.expires_in);

        let token = OAuthToken {
            provider_id: provider_id.to_string(),
            access_token: token_response.access_token,
            refresh_token: token_response.refresh_token,
            expires_at,
            enterprise_url: None,
        };

        // Save token
        self.token_store.save(token.clone())?;

        Ok(token)
    }

    /// Refresh an access token
    pub async fn refresh_token(&self, provider_id: &str) -> Result<OAuthToken> {
        let existing_token = self.token_store.get(provider_id)
            .context("No token found for provider")?;

        #[derive(Serialize)]
        struct RefreshRequest {
            grant_type: String,
            refresh_token: String,
            client_id: String,
        }

        let request = RefreshRequest {
            grant_type: "refresh_token".to_string(),
            refresh_token: existing_token.refresh_token.clone(),
            client_id: self.config.client_id.clone(),
        };

        #[derive(Deserialize)]
        struct TokenResponse {
            access_token: String,
            refresh_token: String,
            expires_in: i64,
        }

        let response = self.http_client
            .post(&self.config.token_url)
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .context("Failed to refresh token")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!("Token refresh failed: {} - {}", status, body));
        }

        let token_response: TokenResponse = response.json().await
            .context("Failed to parse token response")?;

        let expires_at = Utc::now() + chrono::Duration::seconds(token_response.expires_in);

        let token = OAuthToken {
            provider_id: provider_id.to_string(),
            access_token: token_response.access_token,
            refresh_token: token_response.refresh_token,
            expires_at,
            enterprise_url: existing_token.enterprise_url,
        };

        // Save refreshed token
        self.token_store.save(token.clone())?;

        Ok(token)
    }

    /// Get a valid access token (refreshing if needed)
    pub async fn get_valid_token(&self, provider_id: &str) -> Result<String> {
        let token = self.token_store.get(provider_id)
            .context("No token found for provider")?;

        if token.needs_refresh() {
            let refreshed = self.refresh_token(provider_id).await?;
            Ok(refreshed.access_token)
        } else {
            Ok(token.access_token)
        }
    }

    /// Create an API key using OAuth token (for Anthropic Console flow)
    pub async fn create_api_key(&self, provider_id: &str) -> Result<String> {
        let access_token = self.get_valid_token(provider_id).await?;

        #[derive(Deserialize)]
        struct ApiKeyResponse {
            raw_key: String,
        }

        let response = self.http_client
            .post("https://api.anthropic.com/api/oauth/claude_cli/create_api_key")
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", access_token))
            .send()
            .await
            .context("Failed to create API key")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!("API key creation failed: {} - {}", status, body));
        }

        let api_key_response: ApiKeyResponse = response.json().await
            .context("Failed to parse API key response")?;

        Ok(api_key_response.raw_key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pkce_generation() {
        let pkce = PKCEVerifier::generate();

        // Verifier should be base64 URL-safe encoded
        assert!(!pkce.verifier.is_empty());
        assert!(!pkce.challenge.is_empty());

        // Challenge should be different from verifier
        assert_ne!(pkce.verifier, pkce.challenge);
    }

    #[test]
    fn test_authorization_url() {
        let config = OAuthConfig::anthropic();
        let token_store = TokenStore::new(std::env::temp_dir().join("test_tokens.json")).unwrap();
        let client = OAuthClient::new(config, token_store);

        let auth_url = client.get_authorization_url();

        assert!(auth_url.url.contains("client_id="));
        assert!(auth_url.url.contains("code_challenge="));
        assert!(auth_url.url.contains("code_challenge_method=S256"));
        assert!(auth_url.url.contains("scope="));
    }
}
