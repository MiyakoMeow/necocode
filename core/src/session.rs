//! Session management for interactive and single-message modes.
//!
//! This module contains the main session logic that handles both
//! interactive REPL loops and single-message execution.

use crate::Client;
use crate::command::Command;
use crate::config::ProviderSettings;
use crate::events::CoreEvent;
use crate::input::Reader;
use anyhow::{Context, Result};
use serde_json::json;
use tokio::sync::mpsc;

/// Session for managing conversations with the AI.
///
/// A session maintains conversation state and provides methods for
/// both interactive and single-message execution modes.
pub struct Session {
    /// API client for communicating with Anthropic
    client: Client,
    /// System prompt to use for all messages
    system_prompt: String,
    /// Tool schemas available to the AI
    schema: Vec<serde_json::Value>,
    /// Conversation history
    messages: Vec<serde_json::Value>,
}

impl Session {
    /// Create a new session with the given configuration.
    ///
    /// # Arguments
    ///
    /// * `config` - Provider API configuration
    /// * `cwd` - Current working directory for context
    #[must_use]
    pub fn new(config: ProviderSettings, cwd: &str) -> Self {
        let client = Client::new(config);
        let system_prompt = format!("Concise coding assistant. cwd: {cwd}");
        let schema = crate::api::anthropic::schema::tool_schemas();

        Self {
            client,
            system_prompt,
            schema,
            messages: Vec::new(),
        }
    }

    /// Run the session in interactive mode.
    ///
    /// This method enters a REPL loop, continuously reading user input
    /// and processing commands until the user quits.
    ///
    /// # Arguments
    ///
    /// * `reader` - Input reader for getting user input
    /// * `event_sender` - Sender for core events
    ///
    /// # Returns
    ///
    /// Ok(()) on normal exit, Err on error
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Command handling fails
    /// - Agent loop fails
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use neco_core::{Session, StdinReader, ProviderSettings};
    /// use tokio::sync::mpsc;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let (event_sender, _) = mpsc::unbounded_channel();
    /// let config = ProviderSettings::from_env().await?;
    /// let mut session = Session::new(config, "/path");
    /// let reader = StdinReader;
    ///
    /// session.run_interactive(reader, event_sender).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn run_interactive(
        &mut self,
        mut reader: impl Reader,
        event_sender: mpsc::UnboundedSender<CoreEvent>,
    ) -> Result<()> {
        loop {
            let Some(user_input) = reader.read_line().await else {
                break;
            };

            let user_input = user_input.trim();
            if user_input.is_empty() {
                continue;
            }

            let command = Self::parse_input(user_input);

            let should_continue = self.handle_command(command, &event_sender).await?;
            if !should_continue {
                break;
            }
        }

        Ok(())
    }

    /// Run the session in single-message mode.
    ///
    /// This method processes a single message and returns immediately.
    ///
    /// # Arguments
    ///
    /// * `message` - The message to send to the AI
    /// * `event_sender` - Sender for core events
    ///
    /// # Returns
    ///
    /// Ok(()) on success, Err on error
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Command handling fails
    /// - Agent loop fails
    pub async fn run_single(
        &mut self,
        message: String,
        event_sender: mpsc::UnboundedSender<CoreEvent>,
    ) -> Result<()> {
        let command = Command::Message(message);
        self.handle_command(command, &event_sender).await?;
        Ok(())
    }

    /// Clear the conversation history.
    pub fn clear_history(&mut self) {
        self.messages.clear();
    }

    /// Get reference to the API client.
    #[must_use]
    pub const fn client(&self) -> &Client {
        &self.client
    }

    /// Get reference to the messages history.
    #[must_use]
    pub fn messages(&self) -> &[serde_json::Value] {
        &self.messages
    }

    /// Get mutable reference to the messages history.
    #[must_use]
    pub fn messages_mut(&mut self) -> &mut Vec<serde_json::Value> {
        &mut self.messages
    }

    /// Get reference to the system prompt.
    #[must_use]
    pub fn system_prompt(&self) -> &str {
        &self.system_prompt
    }

    /// Get reference to the tool schemas.
    #[must_use]
    pub fn schema(&self) -> &[serde_json::Value] {
        &self.schema
    }

    /// Run the agent loop with the current session state.
    ///
    /// # Arguments
    ///
    /// * `event_sender` - Sender for core events
    ///
    /// # Returns
    ///
    /// Ok(()) on success, Err on error
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - API request fails
    /// - Tool execution fails
    /// - Response processing fails
    pub async fn run_agent_loop(
        &mut self,
        event_sender: &mpsc::UnboundedSender<CoreEvent>,
    ) -> Result<()> {
        self.client
            .run_agent_loop_stream(
                &mut self.messages,
                &self.system_prompt,
                &self.schema,
                Some(event_sender),
            )
            .await
            .context("Agent loop error")
    }

    /// Parse user input into a command.
    ///
    /// # Arguments
    ///
    /// * `input` - Raw user input string
    ///
    /// # Returns
    ///
    /// Parsed command
    fn parse_input(input: &str) -> Command {
        match input {
            "/q" | "exit" => Command::Quit,
            "/c" => Command::Clear,
            msg => Command::Message(msg.to_string()),
        }
    }

    /// Handle a user command.
    ///
    /// # Arguments
    ///
    /// * `command` - The command to handle
    /// * `event_sender` - Sender for core events
    ///
    /// # Returns
    ///
    /// Ok(true) to continue the loop, Ok(false) to exit, Err on error
    async fn handle_command(
        &mut self,
        command: Command,
        event_sender: &mpsc::UnboundedSender<CoreEvent>,
    ) -> Result<bool> {
        match command {
            Command::Quit => Ok(false),
            Command::Clear => {
                self.clear_history();
                let _ = event_sender.send(CoreEvent::Error("Conversation cleared".to_string()));
                Ok(true)
            },
            Command::Message(msg) => {
                self.messages.push(json!({
                    "role": "user",
                    "content": msg,
                }));

                if let Err(e) = self
                    .client
                    .run_agent_loop_stream(
                        &mut self.messages,
                        &self.system_prompt,
                        &self.schema,
                        Some(event_sender),
                    )
                    .await
                {
                    let _ = event_sender.send(CoreEvent::Error(format!("Error: {e}")));
                }

                Ok(true)
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_parse_input() {
        assert_eq!(Session::parse_input("/q"), Command::Quit);
        assert_eq!(Session::parse_input("exit"), Command::Quit);
        assert_eq!(Session::parse_input("/c"), Command::Clear);
        assert_eq!(
            Session::parse_input("hello"),
            Command::Message("hello".to_string())
        );
    }

    #[tokio::test]
    async fn test_session_clear_history() {
        let mut registry = crate::ProviderRegistry::global().write().await;
        registry.register_defaults();
        drop(registry);

        let config = ProviderSettings::from_env().await.unwrap();
        let mut session = Session::new(config, "/test");

        session
            .messages
            .push(json!({"role": "user", "content": "test"}));
        assert!(!session.messages.is_empty());

        session.clear_history();
        assert!(session.messages.is_empty());
    }

    #[tokio::test]
    async fn test_session_new() {
        let mut registry = crate::ProviderRegistry::global().write().await;
        registry.register_defaults();
        drop(registry);

        let config = ProviderSettings::from_env().await.unwrap();
        let session = Session::new(config, "/test");

        assert!(session.messages.is_empty());
        assert!(!session.system_prompt.is_empty());
        assert!(!session.schema.is_empty());
    }
}
