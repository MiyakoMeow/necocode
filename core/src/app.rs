//! Application layer for neco using Actix Actor model.
//!
//! This module provides the main application abstraction that manages
//! sessions and actor-based event handling.

use actix::prelude::*;
use anyhow::Result;

use crate::config::{Config, ProviderSettings};
use crate::events::UiEvent;
use crate::session::{ProcessMessage, SessionActor};

/// Application builder for creating and running the actor system.
pub struct App;

impl App {
    /// Create a session actor and return its address.
    ///
    /// # Arguments
    ///
    /// * `provider_config` - Provider API configuration.
    /// * `config` - Application configuration.
    ///
    /// # Returns
    ///
    /// The session actor address.
    #[must_use]
    pub fn create_session(
        provider_config: ProviderSettings,
        config: &Config,
    ) -> Addr<SessionActor> {
        SessionActor::new(provider_config, &config.cwd).start()
    }

    /// Process a single message asynchronously.
    ///
    /// # Arguments
    ///
    /// * `session_addr` - Session actor address.
    /// * `message` - The message to process.
    /// * `ui_recipient` - UI event recipient.
    ///
    /// # Returns
    ///
    /// Result indicating success or failure.
    ///
    /// # Errors
    ///
    /// Returns an error if the session actor fails to process the message.
    pub async fn process_message(
        session_addr: &Addr<SessionActor>,
        message: String,
        ui_recipient: Recipient<UiEvent>,
    ) -> Result<()> {
        session_addr
            .send(ProcessMessage {
                content: message,
                ui_recipient,
            })
            .await?
    }

    /// Clear conversation history.
    ///
    /// # Arguments
    ///
    /// * `session_addr` - Session actor address.
    pub async fn clear_history(session_addr: &Addr<SessionActor>) {
        session_addr.send(crate::session::ClearHistory).await.ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[actix::test]
    async fn test_app_create_session() {
        let mut registry = crate::ProviderRegistry::global().write().await;
        registry.register_defaults();
        drop(registry);

        let provider_config = ProviderSettings::from_env().await.unwrap();
        let config = Config::from_env();
        let _session = App::create_session(provider_config, &config);
    }
}
