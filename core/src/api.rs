//! API module for neco
//!
//! This module provides the provider system for supporting multiple LLM providers.

pub mod anthropic;

pub use crate::config::ProviderConfig;

use crate::config::{AppConfig, ProviderConfigFile};
use anyhow::Result;
use async_trait::async_trait;
use indexmap::IndexMap;
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
}

impl ProviderConfig {
    /// Parse model string and create ProviderConfig.
    ///
    /// # Arguments
    ///
    /// * `model_str` - Model specification in either "provider/model" or "model" format
    ///
    /// # Examples
    ///
    /// * "zhipuai/glm-4.7" → uses zhipuai provider with glm-4.7 model
    /// * "glm-4.7" → uses default model provider (configured or "anthropic") with glm-4.7 model
    ///
    /// # Errors
    ///
    /// Returns error if provider is not found or API key is missing.
    ///
    /// # Returns
    ///
    /// The provider configuration with the specified model.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use neco_core::ProviderConfig;
    /// # async fn test() -> anyhow::Result<()> {
    /// let config = ProviderConfig::from_model_string("zhipuai/glm-4.7").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn from_model_string(model_str: &str) -> Result<Self> {
        let app_config = AppConfig::load();

        let (provider_name, model) = if model_str.contains('/') {
            let parts: Vec<&str> = model_str.splitn(2, '/').collect();
            (parts[0], parts[1])
        } else {
            (app_config.get_default_model_provider(), model_str)
        };

        let provider_file = app_config
            .get_provider_config(provider_name)
            .ok_or_else(|| {
                anyhow::anyhow!("Provider '{}' not found in configuration", provider_name)
            })?;

        let provider = Arc::new(ConfigFileProvider::new(
            provider_name.to_string(),
            provider_file.clone(),
        ));

        let mut config = provider.load_config();

        if config.api_key.is_empty() {
            let env_var = provider_file.api_key_env.as_deref().unwrap_or("API_KEY");

            return Err(anyhow::anyhow!(
                "API key is missing for provider '{}'. Set the {} environment variable or configure api_key in config file",
                provider_name,
                env_var
            ));
        }

        config.model = model.to_string();

        Ok(config)
    }

    /// Load configuration from environment with automatic provider detection.
    ///
    /// This method automatically detects available providers and loads their configuration.
    /// Provider detection follows the registration order in the registry.
    ///
    /// # Returns
    ///
    /// The provider configuration.
    pub async fn from_env() -> Self {
        let registry = ProviderRegistry::global().read().await;
        let provider = registry.detect_provider().await;
        drop(registry);

        provider.load_config()
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
        self.name.as_str()
    }

    fn is_available(&self) -> bool {
        self.config.api_key.is_some()
            || self
                .config
                .api_key_env
                .as_ref()
                .is_some_and(|env| std::env::var(env).is_ok())
    }

    fn load_config(&self) -> ProviderConfig {
        let api_key = self
            .config
            .api_key_env
            .as_ref()
            .and_then(|env| std::env::var(env).ok())
            .or_else(|| self.config.api_key.clone())
            .unwrap_or_default();

        let base_url = self
            .config
            .base_url
            .clone()
            .unwrap_or_else(|| "https://api.anthropic.com".to_string());

        let model = self
            .config
            .default_model
            .clone()
            .unwrap_or_else(|| "claude-opus-4-5".to_string());

        ProviderConfig {
            name: self.name.clone(),
            base_url,
            model,
            api_key,
        }
    }
}

/// Provider registry (singleton pattern).
pub struct ProviderRegistry {
    /// Registered providers
    providers: IndexMap<String, Arc<dyn Provider>>,
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
            providers: IndexMap::new(),
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
        self.providers.insert(provider.name().to_string(), provider);
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
                base_url: Some("https://api.test.com".to_string()),
                api_key: None,
                api_key_env: Some("TEST_API_KEY".to_string()),
                default_model: Some("test-model".to_string()),
            },
        ));

        registry.register(provider.clone());

        assert!(registry.get_provider("test").is_some());
    }

    #[test]
    fn test_provider_config_masked_api_key() {
        let config = ProviderConfig {
            name: "test".to_string(),
            base_url: "https://api.test.com".to_string(),
            model: "test-model".to_string(),
            api_key: "sk-test1234abcd".to_string(),
        };

        assert_eq!(config.masked_api_key(), "sk-t...abcd");
    }

    #[test]
    fn test_provider_config_masked_api_key_short() {
        let config = ProviderConfig {
            name: "test".to_string(),
            base_url: "https://api.test.com".to_string(),
            model: "test-model".to_string(),
            api_key: "short".to_string(),
        };

        assert_eq!(config.masked_api_key(), "*****");
    }

    #[test]
    fn test_provider_config_masked_api_key_empty() {
        let config = ProviderConfig {
            name: "test".to_string(),
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
    }

    #[tokio::test]
    async fn test_from_model_string_missing_api_key() {
        // Temporarily remove the API key environment variable
        let original_key = std::env::var("ANTHROPIC_AUTH_TOKEN").ok();

        unsafe {
            std::env::remove_var("ANTHROPIC_AUTH_TOKEN");
        }

        let result =
            ProviderConfig::from_model_string("anthropic/claude-3-5-sonnet-20241022").await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("API key is missing"));
        assert!(err_msg.contains("anthropic"));
        assert!(err_msg.contains("ANTHROPIC_AUTH_TOKEN"));

        // Restore original environment variable
        if let Some(key) = original_key {
            unsafe {
                std::env::set_var("ANTHROPIC_AUTH_TOKEN", key);
            }
        }
    }
}
