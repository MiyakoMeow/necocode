//! Application layer for necocode.
//!
//! This module provides the main application abstraction that manages
//! sessions, event channels, and the main execution loop.

use crate::events::CoreEvent;
use crate::input::InputReader;
use crate::session::Session;
use crate::{AnthropicConfig, Config};
use anyhow::Result;
use tokio::sync::mpsc;

/// Main application structure that manages the entire lifecycle.
///
/// # Examples
///
/// ```no_run
/// use necocode_core::{App, AnthropicConfig, Config, StdinInputReader};
///
/// # fn main() -> anyhow::Result<()> {
/// let anthropic_config = AnthropicConfig::from_env();
/// let config = Config::from_env();
/// let mut app = App::new(anthropic_config, config)?;
///
/// // Get event receiver for rendering
/// let event_receiver = app.take_event_receiver();
///
/// // Run in interactive mode
/// let reader = StdinInputReader;
/// app.run_interactive(reader)?;
/// # Ok(())
/// # }
/// ```
pub struct App {
    /// The session managing AI conversations
    session: Session,
    /// Sender for core events (used internally)
    event_sender: mpsc::UnboundedSender<CoreEvent>,
    /// Receiver for core events (to be passed to CLI for rendering)
    event_receiver: Option<mpsc::UnboundedReceiver<CoreEvent>>,
    /// Application config
    config: Config,
}

impl App {
    /// Create a new application instance.
    ///
    /// # Arguments
    ///
    /// * `anthropic_config` - Anthropic API configuration
    /// * `config` - Application configuration
    ///
    /// # Returns
    ///
    /// Returns a new App instance with initialized session and event channels.
    ///
    /// # Errors
    ///
    /// Returns an error if event channel creation fails.
    pub fn new(anthropic_config: AnthropicConfig, config: Config) -> Result<Self> {
        let (event_sender, event_receiver) = mpsc::unbounded_channel();
        let session = Session::new(anthropic_config, config.cwd.clone());

        Ok(Self {
            session,
            event_sender,
            event_receiver: Some(event_receiver),
            config,
        })
    }

    /// Take the event receiver for rendering.
    ///
    /// This method consumes the receiver and returns it to the caller,
    /// typically the CLI layer for event rendering.
    ///
    /// # Returns
    ///
    /// Returns the event receiver if available, None if already taken.
    ///
    /// # Panics
    ///
    /// Panics if called more than once (event receiver can only be taken once).
    #[must_use]
    pub fn take_event_receiver(&mut self) -> mpsc::UnboundedReceiver<CoreEvent> {
        self.event_receiver
            .take()
            .expect("Event receiver can only be taken once")
    }

    /// Get a reference to the event receiver (without taking ownership).
    ///
    /// # Returns
    ///
    /// Returns None if the receiver has already been taken.
    #[must_use]
    pub fn event_receiver(&self) -> Option<&mpsc::UnboundedReceiver<CoreEvent>> {
        self.event_receiver.as_ref()
    }

    /// Run the application in interactive mode (synchronous entry point).
    ///
    /// This method creates its own tokio runtime and runs the interactive
    /// session loop. This is the main entry point for the application.
    ///
    /// # Arguments
    ///
    /// * `reader` - Input reader for user input
    ///
    /// # Returns
    ///
    /// Returns Ok(()) on successful exit, Err on error.
    ///
    /// # Errors
    ///
    /// Returns an error if runtime creation fails or session execution fails.
    pub fn run_interactive(&mut self, reader: impl InputReader) -> Result<()> {
        // Create runtime for async execution
        let rt = tokio::runtime::Runtime::new()?;

        // Block on async interactive session
        rt.block_on(self.run_interactive_async(reader))
    }

    /// Run the application in interactive mode (async implementation).
    ///
    /// # Arguments
    ///
    /// * `reader` - Input reader for user input
    ///
    /// # Returns
    ///
    /// Returns Ok(()) on successful exit, Err on error.
    pub async fn run_interactive_async(&mut self, reader: impl InputReader) -> Result<()> {
        self.session
            .run_interactive(reader, self.event_sender.clone())
            .await
    }

    /// Run the application in single-message mode (synchronous entry point).
    ///
    /// This method creates its own tokio runtime and executes a single
    /// message with the AI.
    ///
    /// # Arguments
    ///
    /// * `message` - The message to send to the AI
    ///
    /// # Returns
    ///
    /// Returns Ok(()) on success, Err on error.
    ///
    /// # Errors
    ///
    /// Returns an error if runtime creation fails or execution fails.
    pub fn run_single(&mut self, message: String) -> Result<()> {
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(self.run_single_async(message))
    }

    /// Run the application in single-message mode (async implementation).
    ///
    /// # Arguments
    ///
    /// * `message` - The message to send to the AI
    ///
    /// # Returns
    ///
    /// Returns Ok(()) on success, Err on error.
    pub async fn run_single_async(&mut self, message: String) -> Result<()> {
        self.session
            .run_single(message, self.event_sender.clone())
            .await
    }

    /// Get reference to the internal session.
    ///
    /// This allows access to session methods if needed.
    #[must_use]
    pub const fn session(&self) -> &Session {
        &self.session
    }

    /// Get mutable reference to the internal session.
    ///
    /// This allows modification of the session state if needed.
    #[must_use]
    pub fn session_mut(&mut self) -> &mut Session {
        &mut self.session
    }

    /// Get reference to the application config.
    #[must_use]
    pub const fn config(&self) -> &Config {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::InputReader;

    // Mock input reader for testing
    struct MockInputReader {
        lines: Vec<Option<String>>,
    }

    impl MockInputReader {
        fn new(lines: Vec<Option<String>>) -> Self {
            Self { lines }
        }
    }

    #[async_trait::async_trait]
    impl InputReader for MockInputReader {
        async fn read_line(&mut self) -> Option<String> {
            if self.lines.is_empty() {
                None
            } else {
                self.lines.remove(0)
            }
        }
    }

    #[test]
    fn test_app_new() {
        let anthropic_config = AnthropicConfig::from_env();
        let config = Config::from_env();
        let app = App::new(anthropic_config, config);

        assert!(app.is_ok());
        let app = app.unwrap();
        assert!(app.event_receiver().is_some());
    }

    #[test]
    fn test_app_take_event_receiver() {
        let anthropic_config = AnthropicConfig::from_env();
        let config = Config::from_env();
        let mut app = App::new(anthropic_config, config).unwrap();

        // First take should succeed
        let _receiver1 = app.take_event_receiver();

        // Second access should return None
        assert!(app.event_receiver().is_none());
    }

    #[test]
    fn test_app_session_access() {
        let anthropic_config = AnthropicConfig::from_env();
        let config = Config::from_env();
        let app = App::new(anthropic_config, config).unwrap();

        // Should be able to access session
        let _session = app.session();
        let _config = app.config();
    }
}
