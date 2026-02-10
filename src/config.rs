//! Configuration management for nanocode

use std::env;

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
        let cwd = env::current_dir().map_or_else(|_| ".".to_string(), |p| p.display().to_string());

        Self { cwd }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::from_env()
    }
}
