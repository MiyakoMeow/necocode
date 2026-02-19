//! Application layer for neco.
//!
//! This module provides the main application abstraction that manages
//! sessions, event channels, and the main execution loop.

use crate::command::UserCommand;
use crate::config::{Config, ProviderConfig};
use crate::events::CoreEvent;
use crate::input::InputReader;
use crate::session::Session;
use anyhow::Result;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

/// Main application structure that manages the entire lifecycle.
///
/// # Examples
///
/// ```no_run
/// use neco_core::{App, ProviderConfig, Config, StdinInputReader};
///
/// # fn main() -> anyhow::Result<()> {
/// let rt = tokio::runtime::Runtime::new()?;
/// let (provider_config, config) = rt.block_on(async {
///     let provider_config = ProviderConfig::from_env_with_validation().await;
///     let config = Config::from_env();
///     (provider_config, config)
/// });
/// let mut app = App::new(provider_config, config)?;
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
    /// * `provider_config` - Provider API configuration
    /// * `config` - Application configuration
    ///
    /// # Returns
    ///
    /// Returns a new App instance with initialized session and event channels.
    ///
    /// # Errors
    ///
    /// Returns an error if event channel creation fails.
    pub fn new(provider_config: ProviderConfig, config: Config) -> Result<Self> {
        let (event_sender, event_receiver) = mpsc::unbounded_channel();
        let session = Session::new(provider_config, config.cwd.clone());

        Ok(Self {
            session,
            event_sender,
            event_receiver: Some(event_receiver),
            config,
        })
    }

    /// Internal constructor that uses existing event channels.
    ///
    /// # Arguments
    ///
    /// * `provider_config` - Provider API configuration
    /// * `config` - Application configuration
    /// * `event_sender` - Pre-created event sender
    ///
    /// # Returns
    ///
    /// Returns a new App instance.
    #[must_use]
    fn new_internal(
        provider_config: ProviderConfig,
        config: Config,
        event_sender: mpsc::UnboundedSender<CoreEvent>,
    ) -> Self {
        let session = Session::new(provider_config, config.cwd.clone());

        Self {
            session,
            event_sender,
            event_receiver: None,
            config,
        }
    }

    /// Unified entry point for the application.
    ///
    /// This method creates the runtime, loads configuration, and starts the main loop.
    ///
    /// # Arguments
    ///
    /// * `config` - Application configuration
    /// * `input_receiver` - Channel for receiving user input
    /// * `message` - Optional single message to process (non-interactive mode)
    /// * `model_arg` - Optional model specification (e.g., "provider/model" or "model")
    ///
    /// # Returns
    ///
    /// Returns a tuple containing:
    /// - The event receiver for rendering
    /// - The main loop handle
    /// - The loaded Provider config for display
    ///
    /// # Errors
    ///
    /// Returns an error if runtime creation fails or initialization fails.
    pub fn run(
        config: Config,
        input_receiver: mpsc::UnboundedReceiver<String>,
        message: Option<String>,
        model_arg: Option<String>,
        rt: &tokio::runtime::Runtime,
    ) -> Result<(
        mpsc::UnboundedReceiver<CoreEvent>,
        JoinHandle<Result<()>>,
        ProviderConfig,
    )> {
        let (event_receiver, handle, provider_config) = rt.block_on(async move {
            let provider_config = if let Some(model_str) = model_arg {
                ProviderConfig::from_model_string(&model_str).await?
            } else {
                ProviderConfig::from_env_with_validation().await
            };
            let (event_sender, event_receiver) = mpsc::unbounded_channel();
            let mut app = Self::new_internal(provider_config.clone(), config, event_sender);

            let handle = if let Some(msg) = message {
                tokio::spawn(async move { app.run_single_async(msg).await })
            } else {
                tokio::spawn(async move { app.run_interactive_with_input(input_receiver).await })
            };

            Ok::<_, anyhow::Error>((event_receiver, handle, provider_config))
        })?;

        Ok((event_receiver, handle, provider_config))
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

    /// Run interactive mode with input from a channel.
    ///
    /// This method reads user input from the provided channel and processes commands.
    ///
    /// # Arguments
    ///
    /// * `input_receiver` - Channel for receiving user input
    ///
    /// # Returns
    ///
    /// Returns Ok(()) on successful exit, Err on error.
    pub async fn run_interactive_with_input(
        &mut self,
        mut input_receiver: mpsc::UnboundedReceiver<String>,
    ) -> Result<()> {
        loop {
            let Some(user_input) = input_receiver.recv().await else {
                break;
            };

            let user_input = user_input.trim();
            if user_input.is_empty() {
                continue;
            }

            let command = Self::parse_input(user_input);
            let should_continue = self.handle_command(command).await?;
            if !should_continue {
                break;
            }
        }

        Ok(())
    }

    /// Parse user input into a command.
    ///
    /// # Arguments
    ///
    /// * `input` - Raw user input string
    ///
    /// # Returns
    /// Parsed command
    #[must_use]
    fn parse_input(input: &str) -> UserCommand {
        match input {
            "/q" | "exit" => UserCommand::Quit,
            "/c" => UserCommand::Clear,
            msg => UserCommand::Message(msg.to_string()),
        }
    }

    /// Handle a user command.
    ///
    /// # Arguments
    ///
    /// * `command` - The command to handle
    ///
    /// # Returns
    ///
    /// Ok(true) to continue the loop, Ok(false) to exit, Err on error
    async fn handle_command(&mut self, command: UserCommand) -> Result<bool> {
        match command {
            UserCommand::Quit => Ok(false),
            UserCommand::Clear => {
                self.session.clear_history();
                let _ = self
                    .event_sender
                    .send(CoreEvent::Error("Conversation cleared".to_string()));
                Ok(true)
            }
            UserCommand::Message(_msg) => {
                if let Err(e) = self.session.run_agent_loop(&self.event_sender).await {
                    let _ = self
                        .event_sender
                        .send(CoreEvent::Error(format!("Error: {}", e)));
                }
                Ok(true)
            }
        }
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

    #[tokio::test]
    async fn test_app_new() {
        let mut registry = crate::ProviderRegistry::global().write().await;
        registry.register_defaults().await;
        drop(registry);

        let provider_config = ProviderConfig::from_env_with_validation().await;
        let config = Config::from_env();
        let app = App::new(provider_config, config);

        assert!(app.is_ok());
        let app = app.unwrap();
        assert!(app.event_receiver().is_some());
    }

    #[tokio::test]
    async fn test_app_take_event_receiver() {
        let mut registry = crate::ProviderRegistry::global().write().await;
        registry.register_defaults().await;
        drop(registry);

        let provider_config = ProviderConfig::from_env_with_validation().await;
        let config = Config::from_env();
        let mut app = App::new(provider_config, config).unwrap();

        // First take should succeed
        let _receiver1 = app.take_event_receiver();

        // Second access should return None
        assert!(app.event_receiver().is_none());
    }

    #[tokio::test]
    async fn test_app_session_access() {
        let mut registry = crate::ProviderRegistry::global().write().await;
        registry.register_defaults().await;
        drop(registry);

        let provider_config = ProviderConfig::from_env_with_validation().await;
        let config = Config::from_env();
        let app = App::new(provider_config, config).unwrap();

        // Should be able to access session
        let _session = app.session();
        let _config = app.config();
    }
}
