//! Nyan-code core library
//!
//! Provides pure business logic, including API client, configuration management, tool functions, and event types.
//!
//! This library serves as the entry point for the entire application, re-exporting all major public types and modules.

use tracing as _;

pub mod api;
pub mod app;
pub mod command;
pub mod config;
pub mod events;
pub mod input;
pub mod session;
pub mod tools;

pub use api::anthropic::{ApiError, Client};

pub use api::{Provider, ProviderRegistry};

pub use config::{Config, Configuration, FileProvider, ProviderSettings};

pub use events::{UiEvent, UiRecipient};

pub use command::Command;

pub use input::{Reader, StdinReader};

pub use session::{ClearHistory, GetHistory, ProcessMessage, SessionActor};

pub use app::App;
