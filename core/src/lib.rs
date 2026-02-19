//! Nyan-code core library
//!
//! Provides pure business logic, including API client, configuration management, tool functions, and event types.
//!
//! This library serves as the entry point for the entire application, re-exporting all major public types and modules.

pub mod api;
pub mod app;
pub mod command;
pub mod config;
pub mod events;
pub mod input;
pub mod session;
pub mod tools;

// Re-export common types from api::anthropic
pub use api::anthropic::{ApiError, Client};

// Re-export common types from api::anthropic::models
pub use api::anthropic::models::{ModelInfo, ModelPreference};

// Re-export provider types
pub use api::{ConfigFileProvider, Provider, ProviderRegistry};

// Re-export configuration types
pub use config::{AppConfig, Config, ProviderConfig, ProviderConfigFile};

// Re-export CoreEvent type from events
pub use events::CoreEvent;

// Re-export UserCommand type from command
pub use command::UserCommand;

// Re-export input types from input
pub use input::{InputReader, StdinInputReader};

// Re-export Session type from session
pub use session::Session;

// Re-export App type from app
pub use app::App;
