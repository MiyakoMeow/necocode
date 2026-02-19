//! Configuration management for neco

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Application configuration file.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppConfig {
    /// General settings
    #[serde(default)]
    pub general: GeneralConfig,
    /// Provider configurations
    #[serde(default)]
    pub model_providers: HashMap<String, ProviderConfigFile>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            general: GeneralConfig::default(),
            model_providers: Self::builtin_providers(),
        }
    }
}

impl AppConfig {
    /// Get built-in provider configurations.
    fn builtin_providers() -> HashMap<String, ProviderConfigFile> {
        let mut providers = HashMap::new();

        providers.insert(
            "anthropic".to_string(),
            ProviderConfigFile {
                base_url: Some("https://api.anthropic.com".to_string()),
                api_key: None,
                api_key_env: Some("ANTHROPIC_AUTH_TOKEN".to_string()),
                default_model: Some("claude-opus-4-5".to_string()),
            },
        );

        providers
    }

    /// Load configuration from file, merging with built-in defaults.
    ///
    /// This method loads user configuration from `~/.config/neco/config.toml`
    /// and merges it with built-in provider configurations. User-defined
    /// providers override built-in ones with the same name.
    ///
    /// # Returns
    ///
    /// The merged configuration.
    #[must_use]
    pub fn load() -> Self {
        let mut config = Self::default();

        if let Some(user_config_path) = Self::get_config_path()
            && let Ok(content) = std::fs::read_to_string(&user_config_path)
                && let Ok(user_config) = toml::from_str::<AppConfig>(&content) {
                    for (name, provider) in user_config.model_providers {
                        config.model_providers.insert(name, provider);
                    }
                    config.general = user_config.general;
                }

        config
    }

    /// Get the configuration file path.
    ///
    /// Follows XDG Base Directory specification:
    /// - `$XDG_CONFIG_HOME/neco/config.toml` (if XDG_CONFIG_HOME is set)
    /// - `~/.config/neco/config.toml` (default)
    ///
    /// # Returns
    ///
    /// The path to the configuration file, or `None` if home directory cannot be determined.
    fn get_config_path() -> Option<PathBuf> {
        let config_dir = dirs::config_dir().map(|p| p.join("neco"));

        if let Some(ref dir) = config_dir {
            return Some(dir.join("config.toml"));
        }

        None
    }
}

/// General application configuration.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct GeneralConfig {
    /// Currently active provider (optional, defaults to auto-detection)
    pub active_provider: Option<String>,
}

/// Provider configuration loaded from file.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProviderConfigFile {
    /// API base URL
    pub base_url: Option<String>,
    /// API key (default value)
    pub api_key: Option<String>,
    /// API key environment variable name (overrides api_key if set)
    pub api_key_env: Option<String>,
    /// Default model
    pub default_model: Option<String>,
}

/// Application configuration read from environment variables.
#[derive(Debug, Clone)]
pub struct Config {
    /// Current working directory
    pub cwd: String,
}

impl Config {
    /// Load configuration from environment variables.
    ///
    /// # Environment Variables
    ///
    /// None required - uses current working directory as default.
    ///
    /// # Returns
    ///
    /// Returns the configuration with defaults applied.
    #[must_use]
    pub fn from_env() -> Self {
        let cwd =
            std::env::current_dir().map_or_else(|_| ".".to_string(), |p| p.display().to_string());

        Self { cwd }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::from_env()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_config_default() {
        let config = AppConfig::default();
        assert!(config.model_providers.contains_key("anthropic"));
    }

    #[test]
    fn test_provider_config_file_anthropic() {
        let provider = AppConfig::default()
            .model_providers
            .get("anthropic")
            .cloned()
            .unwrap();

        assert_eq!(provider.api_key_env, Some("ANTHROPIC_AUTH_TOKEN".to_string()));
        assert_eq!(provider.api_key, None);
        assert_eq!(
            provider.base_url,
            Some("https://api.anthropic.com".to_string())
        );
    }

    #[test]
    fn test_config_from_env() {
        let config = Config::from_env();
        assert!(!config.cwd.is_empty());
    }
}
