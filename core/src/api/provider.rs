//! Provider system for supporting multiple LLM providers.
//!
//! This module defines a flexible provider system that allows runtime registration
//! of different LLM providers through a configuration file.

use crate::config::{AppConfig, ProviderConfigFile};
use crate::api::anthropic::models::{ModelPreference, fetch_available_models, recommend_model, validate_model};
use async_trait::async_trait;
use reqwest::Client as HttpClient;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Provider trait - all LLM providers must implement this interface.
#[async_trait]
pub trait Provider: Send + Sync {
    /// Unique provider identifier.
    fn name(&self) -> &str;

    /// Display name for UI purposes.
    fn display_name(&self) -> &str;

    /// Check if this provider is available (by checking environment variables).
    fn is_available(&self) -> bool;

    /// Load configuration from environment variables.
    fn load_config(&self) -> ProviderConfig;

    /// Validate and recommend model (async).
    async fn validate_and_recommend_model(
        &self,
        current_model: &str,
        validate: bool,
        preference: Option<ModelPreference>,
    ) -> String;
}

/// Provider configuration (unified for all providers).
#[derive(Debug, Clone)]
pub struct ProviderConfig {
    /// Provider name
    pub provider_name: String,
    /// API base URL
    pub base_url: String,
    /// Model name
    pub model: String,
    /// API key
    pub api_key: String,
}

impl ProviderConfig {
    /// Load configuration from environment with automatic provider detection.
    ///
    /// This method automatically detects available providers and loads their configuration.
    /// Provider detection follows the registration order in the registry.
    ///
    /// # Returns
    ///
    /// The provider configuration with model validation if enabled.
    pub async fn from_env_with_validation() -> Self {
        let registry = ProviderRegistry::global().read().await;
        let provider = registry.detect_provider().await;
        drop(registry);

        let should_validate = std::env::var("NEOCODE_VALIDATE_MODEL")
            .ok()
            .and_then(|v| v.parse::<bool>().ok())
            .unwrap_or(false);

        let preference = match std::env::var("NEOCODE_MODEL_PREFERENCE").as_deref() {
            Ok("opus") => Some(ModelPreference::Opus),
            Ok("sonnet") => Some(ModelPreference::Sonnet),
            Ok("haiku") => Some(ModelPreference::Haiku),
            _ => None,
        };

        let mut config = provider.load_config();

        config.model = provider
            .validate_and_recommend_model(&config.model, should_validate, preference)
            .await;

        config
    }

    /// Load configuration from environment (synchronous version without validation).
    ///
    /// # Returns
    ///
    /// The provider configuration.
    #[must_use]
    pub fn from_env() -> Self {
        tokio::runtime::Runtime::new()
            .expect("Failed to create runtime")
            .block_on(async { Self::from_env_with_validation().await })
    }

    /// Get provider display name.
    #[must_use]
    pub fn provider_display_name(&self) -> &str {
        match self.provider_name.as_str() {
            "anthropic" => "Anthropic",
            "zhipuai" => "ZhipuAI",
            _ => &self.provider_name,
        }
    }

    /// Get masked API key for display (shows only first and last 4 chars).
    #[must_use]
    pub fn masked_api_key(&self) -> String {
        let key = &self.api_key;
        if key.len() > 8 {
            format!("{}...{}", &key[..4], &key[key.len() - 4..])
        } else if !key.is_empty() {
            "*".repeat(key.len())
        } else {
            "(no key)".to_string()
        }
    }
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self::from_env()
    }
}

/// Provider loaded from configuration file.
pub struct ConfigFileProvider {
    /// Provider name
    name: String,
    /// Provider configuration
    config: ProviderConfigFile,
}

impl ConfigFileProvider {
    /// Create a new configuration file provider.
    #[must_use]
    pub fn new(name: String, config: ProviderConfigFile) -> Self {
        Self { name, config }
    }
}

#[async_trait]
impl Provider for ConfigFileProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn display_name(&self) -> &str {
        self.config
            .display_name.as_deref()
            .unwrap_or(self.name.as_str())
    }

    fn is_available(&self) -> bool {
        std::env::var(&self.config.api_key_env).is_ok()
            || self
                .config
                .api_key_env_fallback
                .as_ref()
                .is_some_and(|f| std::env::var(f).is_ok())
    }

    fn load_config(&self) -> ProviderConfig {
        let api_key = std::env::var(&self.config.api_key_env)
            .ok()
            .or_else(|| {
                self.config
                    .api_key_env_fallback
                    .as_ref()
                    .and_then(|f| std::env::var(f).ok())
            })
            .unwrap_or_default();

        let base_url = self
            .config
            .base_url_env
            .as_ref()
            .and_then(|env| std::env::var(env).ok())
            .or_else(|| self.config.base_url.clone())
            .unwrap_or_else(|| "https://api.anthropic.com".to_string());

        let model = self
            .config
            .model_env
            .as_ref()
            .and_then(|env| std::env::var(env).ok())
            .or_else(|| self.config.default_model.clone())
            .unwrap_or_else(|| "claude-opus-4-5".to_string());

        ProviderConfig {
            provider_name: self.name.clone(),
            base_url,
            model,
            api_key,
        }
    }

    async fn validate_and_recommend_model(
        &self,
        current_model: &str,
        validate: bool,
        preference: Option<ModelPreference>,
    ) -> String {
        if !validate {
            return current_model.to_string();
        }

        let config = self.load_config();
        let http_client = HttpClient::new();

        match fetch_available_models(&http_client, &config.base_url, &config.api_key).await {
            Ok(available_models) => {
                if current_model.is_empty()
                    || !validate_model(current_model, &available_models)
                {
                    if let Some(recommended) = recommend_model(&available_models, preference) {
                        eprintln!("🤖 Auto-selected model: {}", recommended);
                        recommended
                    } else {
                        current_model.to_string()
                    }
                } else {
                    eprintln!("✓ Model validated: {}", current_model);
                    current_model.to_string()
                }
            }
            Err(_) => current_model.to_string(),
        }
    }
}

/// Provider registry (singleton pattern).
pub struct ProviderRegistry {
    /// Registered providers
    providers: HashMap<String, Arc<dyn Provider>>,
    /// Default provider name
    default_provider: Option<String>,
}

impl ProviderRegistry {
    /// Get the global registry instance.
    ///
    /// # Returns
    ///
    /// A reference to the global registry wrapped in a RwLock.
    pub fn global() -> &'static RwLock<Self> {
        use std::sync::OnceLock;
        static REGISTRY: OnceLock<RwLock<ProviderRegistry>> = OnceLock::new();
        REGISTRY.get_or_init(|| {
            let registry = Self::new();
            RwLock::new(registry)
        })
    }

    /// Create a new registry.
    #[must_use]
    fn new() -> Self {
        Self {
            providers: HashMap::new(),
            default_provider: Some("anthropic".to_string()),
        }
    }

    /// Register built-in providers.
    ///
    /// This method should be called during application initialization.
    pub async fn register_defaults(&mut self) {
        let app_config = AppConfig::load();

        for (name, provider_config) in app_config.model_providers {
            let provider = Arc::new(ConfigFileProvider::new(name.clone(), provider_config));
            self.providers.insert(name, provider);
        }
    }

    /// Register a new provider.
    ///
    /// # Arguments
    ///
    /// * `provider` - The provider to register
    ///
    /// If a provider with the same name already exists, it will be replaced.
    pub fn register(&mut self, provider: Arc<dyn Provider>) {
        self.providers
            .insert(provider.name().to_string(), provider);
    }

    /// Auto-detect and select the first available provider.
    ///
    /// Providers are checked in registration order. The first provider with
    /// its required environment variables set will be selected.
    ///
    /// # Returns
    ///
    /// The detected provider, or the default provider if none are available.
    pub async fn detect_provider(&self) -> Arc<dyn Provider> {
        for provider in self.providers.values() {
            if provider.is_available() {
                return provider.clone();
            }
        }

        self.default_provider
            .as_ref()
            .and_then(|name| self.providers.get(name))
            .or_else(|| self.providers.values().next())
            .cloned()
            .expect("No providers registered")
    }

    /// Get a provider by name.
    ///
    /// # Arguments
    ///
    /// * `name` - The provider name
    ///
    /// # Returns
    ///
    /// The provider if found, `None` otherwise.
    #[must_use]
    pub fn get_provider(&self, name: &str) -> Option<Arc<dyn Provider>> {
        self.providers.get(name).cloned()
    }

    /// Get all registered providers.
    ///
    /// # Returns
    ///
    /// A vector of all registered providers.
    #[must_use]
    pub fn all_providers(&self) -> Vec<Arc<dyn Provider>> {
        self.providers.values().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_provider_registry() {
        let mut registry = ProviderRegistry::new();

        let provider = Arc::new(ConfigFileProvider::new(
            "test".to_string(),
            ProviderConfigFile {
                display_name: Some("Test Provider".to_string()),
                base_url: Some("https://api.test.com".to_string()),
                api_key_env: "TEST_API_KEY".to_string(),
                api_key_env_fallback: None,
                default_model: Some("test-model".to_string()),
                model_env: None,
                base_url_env: None,
            },
        ));

        registry.register(provider.clone());

        assert!(registry.get_provider("test").is_some());
    }

    #[test]
    fn test_provider_config_masked_api_key() {
        let config = ProviderConfig {
            provider_name: "test".to_string(),
            base_url: "https://api.test.com".to_string(),
            model: "test-model".to_string(),
            api_key: "sk-test1234abcd".to_string(),
        };

        assert_eq!(config.masked_api_key(), "sk-t...abcd");
    }

    #[test]
    fn test_provider_config_masked_api_key_short() {
        let config = ProviderConfig {
            provider_name: "test".to_string(),
            base_url: "https://api.test.com".to_string(),
            model: "test-model".to_string(),
            api_key: "short".to_string(),
        };

        assert_eq!(config.masked_api_key(), "*****");
    }

    #[test]
    fn test_provider_config_masked_api_key_empty() {
        let config = ProviderConfig {
            provider_name: "test".to_string(),
            base_url: "https://api.test.com".to_string(),
            model: "test-model".to_string(),
            api_key: String::new(),
        };

        assert_eq!(config.masked_api_key(), "(no key)");
    }

    #[test]
    fn test_app_config_load() {
        let config = AppConfig::load();
        assert!(config.model_providers.contains_key("anthropic"));
        assert!(config.model_providers.contains_key("zhipuai"));
    }
}
