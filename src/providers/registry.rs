use super::{AnthropicProvider, ProviderConfig, OpenAIProvider, AnthropicCompatibleProvider, error::ProviderError};
use super::gemini::GeminiProvider;
use crate::auth::TokenStore;
use std::collections::HashMap;
use std::sync::Arc;

/// Provider registry that manages all configured providers
pub struct ProviderRegistry {
    /// Map of provider name -> provider instance
    providers: HashMap<String, Arc<Box<dyn AnthropicProvider>>>,
    /// Map of model name -> provider name for fast lookup
    model_to_provider: HashMap<String, String>,
}

impl ProviderRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
            model_to_provider: HashMap::new(),
        }
    }

    /// Create a new registry with configuration and token store
    pub async fn new_from_app_state_deps(config: Arc<tokio::sync::RwLock<crate::config::AppConfig>>, token_store: TokenStore) -> Result<Self, ProviderError> {
        let mut registry = Self::new();
        let app_config_read = config.read().await;

        // Populate registry with providers from app_config
        for provider_config in &app_config_read.providers {
            // Skip disabled providers
            if !provider_config.is_enabled() {
                continue;
            }

            // Get API key or OAuth provider ID
            let auth_credential = provider_config.get_auth_credential().ok_or_else(|| {
                ProviderError::ConfigError(
                    format!("Provider '{}' requires api_key or oauth_provider", provider_config.name)
                )
            })?;

            let provider: Box<dyn AnthropicProvider> = match provider_config.provider_type.as_str() {
                // OpenAI
                "openai" => Box::new(OpenAIProvider::new(
                    provider_config.name.clone(),
                    auth_credential, // Use auth_credential
                    provider_config.base_url.clone().unwrap_or_else(|| "https://api.openai.com/v1".to_string()),
                    provider_config.models.clone(),
                    provider_config.oauth_provider.clone(),
                    Some(token_store.clone()),
                )),

                // Anthropic-compatible providers
                "anthropic" => Box::new(AnthropicCompatibleProvider::new(
                    provider_config.name.clone(),
                    auth_credential, // Use auth_credential
                    provider_config.base_url.clone().unwrap_or_else(|| "https://api.anthropic.com".to_string()),
                    provider_config.models.clone(),
                    provider_config.oauth_provider.clone(),
                    Some(token_store.clone()),
                )),
                "z.ai" => Box::new(AnthropicCompatibleProvider::zai(
                    auth_credential,
                    provider_config.models.clone(),
                    Some(token_store.clone()),
                )),
                "minimax" => Box::new(AnthropicCompatibleProvider::minimax(
                    auth_credential,
                    provider_config.models.clone(),
                    Some(token_store.clone()),
                )),
                "zenmux" => Box::new(AnthropicCompatibleProvider::zenmux(
                    auth_credential,
                    provider_config.models.clone(),
                    Some(token_store.clone()),
                )),
                "kimi-coding" => Box::new(AnthropicCompatibleProvider::kimi_coding(
                    auth_credential,
                    provider_config.models.clone(),
                    Some(token_store.clone()),
                )),

                // OpenAI-compatible providers
                "openrouter" => Box::new(OpenAIProvider::openrouter(
                    provider_config.name.clone(),
                    auth_credential,
                    provider_config.models.clone(),
                )),
                "deepinfra" => Box::new(OpenAIProvider::deepinfra(
                    provider_config.name.clone(),
                    auth_credential,
                    provider_config.models.clone(),
                )),
                "novita" => Box::new(OpenAIProvider::novita(
                    provider_config.name.clone(),
                    auth_credential,
                    provider_config.models.clone(),
                )),
                "baseten" => Box::new(OpenAIProvider::baseten(
                    provider_config.name.clone(),
                    auth_credential,
                    provider_config.models.clone(),
                )),
                "together" => Box::new(OpenAIProvider::together(
                    provider_config.name.clone(),
                    auth_credential,
                    provider_config.models.clone(),
                )),
                "fireworks" => Box::new(OpenAIProvider::fireworks(
                    provider_config.name.clone(),
                    auth_credential,
                    provider_config.models.clone(),
                )),
                "groq" => Box::new(OpenAIProvider::groq(
                    provider_config.name.clone(),
                    auth_credential,
                    provider_config.models.clone(),
                )),
                "nebius" => Box::new(OpenAIProvider::nebius(
                    provider_config.name.clone(),
                    auth_credential,
                    provider_config.models.clone(),
                )),
                "cerebras" => Box::new(OpenAIProvider::cerebras(
                    provider_config.name.clone(),
                    auth_credential,
                    provider_config.models.clone(),
                )),
                "moonshot" => Box::new(OpenAIProvider::moonshot(
                    provider_config.name.clone(),
                    auth_credential,
                    provider_config.models.clone(),
                )),

                // Google Gemini (supports OAuth, API Key, Vertex AI)
                "gemini" => {
                    let api_key_opt = if provider_config.auth_type == super::AuthType::ApiKey {
                        Some(auth_credential.clone())
                    } else {
                        None
                    };

                    Box::new(GeminiProvider::new(
                        provider_config.name.clone(),
                        api_key_opt,
                        provider_config.base_url.clone(),
                        provider_config.models.clone(),
                        HashMap::new(), // custom headers
                        provider_config.oauth_provider.clone(),
                        Some(token_store.clone()),
                        None, // No project_id/location for Gemini (AI Studio/OAuth only)
                        None,
                    ))
                }

                "vertex-ai" => {
                    // Vertex AI provider (separate from Gemini)
                    // Uses Google Cloud Vertex AI with ADC authentication
                    Box::new(GeminiProvider::new(
                        provider_config.name.clone(),
                        None, // No API key for Vertex AI (uses ADC)
                        provider_config.base_url.clone(),
                        provider_config.models.clone(),
                        HashMap::new(), // custom headers
                        None, // No OAuth for Vertex AI
                        Some(token_store.clone()),
                        provider_config.project_id.clone(), // GCP project ID
                        provider_config.location.clone(),   // GCP location
                    ))
                }

                other => {
                    return Err(ProviderError::ConfigError(
                        format!("Unknown provider type: {}", other)
                    ));
                }
            };

            // Add provider to registry
            registry.providers.insert(provider_config.name.clone(), Arc::new(provider));

            // Populate model_to_provider map
            for model_name in &provider_config.models {
                registry.model_to_provider.insert(model_name.clone(), provider_config.name.clone());
            }
        }
        
        // Handle models with explicit mappings (overrides provider.models)
        for model_config in &app_config_read.models {
            for mapping in &model_config.mappings {
                // Check if provider exists
                if !registry.providers.contains_key(&mapping.provider) {
                    return Err(ProviderError::ConfigError(
                        format!("Model '{}' maps to unknown provider '{}'", model_config.name, mapping.provider)
                    ));
                }
                registry.model_to_provider.insert(model_config.name.clone(), mapping.provider.clone());
            }
        }

        Ok(registry)
    }

    /// Get a provider by name
    pub fn get_provider(&self, name: &str) -> Option<Arc<Box<dyn AnthropicProvider>>> {
        self.providers.get(name).cloned()
    }

    /// Get a provider for a specific model
    pub fn get_provider_for_model(&self, model: &str) -> Result<Arc<Box<dyn AnthropicProvider>>, ProviderError> {
        // First, check if we have a direct model → provider mapping
        if let Some(provider_name) = self.model_to_provider.get(model) {
            if let Some(provider) = self.providers.get(provider_name) {
                return Ok(provider.clone());
            }
        }

        // If no direct mapping, search through all providers
        for provider in self.providers.values() {
            if provider.supports_model(model) {
                return Ok(provider.clone());
            }
            // Check if model matches auto_map_regex of any provider
            // NOTE: This logic needs to be in router, not here.
        }

        Err(ProviderError::ModelNotSupported(model.to_string()))
    }

    /// List all available models
    pub fn list_models(&self) -> Vec<String> {
        self.model_to_provider.keys().cloned().collect()
    }

    /// List all providers
    pub fn list_providers(&self) -> Vec<String> {
        self.providers.keys().cloned().collect()
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AppConfig, ServerConfig, RouterConfig};
    use crate::models::{Message, MessageContent, ThinkingConfig};
    use anyhow::Result;

    fn create_test_config() -> AppConfig {
        AppConfig {
            server: ServerConfig::default(),
            router: RouterConfig {
                default: "default.model".to_string(),
                background: Some("background.model".to_string()),
                think: Some("think.model".to_string()),
                websearch: Some("websearch.model".to_string()),
                auto_map_regex: None,
                background_regex: None,
            },
            providers: vec![],
            models: vec![],
        }
    }

    #[tokio::test]
    async fn test_provider_registry_from_config() -> Result<()> {
        let config = create_test_config();
        let config_arc = Arc::new(tokio::sync::RwLock::new(config));
        let token_store = TokenStore::default()?;

        let registry = ProviderRegistry::new_from_app_state_deps(config_arc.clone(), token_store).await?;

        // Add some dummy providers to the config for testing
        let mut writable_config = config_arc.write().await;
        writable_config.providers.push(ProviderConfig {
            name: "openai-test".to_string(),
            provider_type: "openai".to_string(),
            auth_type: super::AuthType::ApiKey,
            api_key: Some("test-key".to_string()),
            oauth_provider: None,
            project_id: None,
            location: None,
            base_url: None,
            models: vec!["gpt-4o".to_string(), "gpt-3.5-turbo".to_string()],
            enabled: Some(true),
        });
        writable_config.providers.push(ProviderConfig {
            name: "anthropic-test".to_string(),
            provider_type: "anthropic".to_string(),
            auth_type: super::AuthType::ApiKey,
            api_key: Some("test-key".to_string()),
            oauth_provider: None,
            project_id: None,
            location: None,
            base_url: None,
            models: vec!["claude-3-opus".to_string()],
            enabled: Some(true),
        });
        drop(writable_config); // Drop the write lock

        let openai_provider = registry.get_provider_for_model("gpt-4o")?;
        assert_eq!(openai_provider.name(), "openai-test");

        let claude_provider = registry.get_provider_for_model("claude-3-opus")?;
        assert_eq!(claude_provider.name(), "anthropic-test");

        let unknown_provider = registry.get_provider_for_model("unknown-model");
        assert!(unknown_provider.is_err());

        Ok(())
    }

    #[test]
    fn test_empty_registry() {
        let registry = ProviderRegistry::new();
        assert!(registry.list_models().is_empty());
        assert!(registry.list_providers().is_empty());
    }

    #[test]
    fn test_get_provider_for_model_not_found() {
        let registry = ProviderRegistry::new();
        let result = registry.get_provider_for_model("gpt-4");
        assert!(result.is_err());
    }
}
