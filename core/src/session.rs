//! Session management using Actix Actor model.
//!
//! This module contains the session actor that handles conversations
//! with the AI through message-based communication.

use actix::prelude::*;
use anyhow::Context as _;
use anyhow::Result;
use serde_json::{Value, json};

use crate::api::anthropic::{Client, StreamEvent, ToolCall};
use crate::config::ProviderSettings;
use crate::events::UiEvent;

/// System prompt base.
const SYSTEM_PROMPT_BASE: &str = "Concise coding assistant. cwd:";

/// User message command.
#[derive(Debug, Clone, Message)]
#[rtype(result = "Result<()>")]
pub struct ProcessMessage {
    /// The message content.
    pub content: String,
    /// UI recipient for event notifications.
    pub ui_recipient: Recipient<UiEvent>,
}

/// Clear history command.
#[derive(Debug, Clone, Message)]
#[rtype(result = "()")]
pub struct ClearHistory;

/// Get history command.
#[derive(Debug, Clone, Message)]
#[rtype(result = "Vec<Value>")]
pub struct GetHistory;

/// Session actor for managing AI conversations.
#[allow(clippy::module_name_repetitions)]
pub struct SessionActor {
    /// API client for communicating with the LLM provider.
    client: Client,
    /// System prompt used for all messages.
    system_prompt: String,
    /// Tool schemas available to the AI.
    schema: Vec<Value>,
    /// Conversation history.
    messages: Vec<Value>,
}

impl SessionActor {
    /// Create a new session actor.
    #[must_use]
    pub fn new(config: ProviderSettings, cwd: &str) -> Self {
        let client = Client::new(config);
        let system_prompt = format!("{SYSTEM_PROMPT_BASE} {cwd}");
        let schema = crate::api::anthropic::schema::tool_schemas();

        Self {
            client,
            system_prompt,
            schema,
            messages: Vec::new(),
        }
    }

    /// Send an event to the UI recipient.
    fn send_event(recipient: &Recipient<UiEvent>, event: UiEvent) {
        recipient.do_send(event);
    }

    /// Run the session loop to process messages and handle tool calls.
    ///
    /// This method continuously processes API responses and executes tool calls
    /// until a final assistant message is received.
    async fn run_session_loop(
        client: Client,
        mut messages: Vec<Value>,
        system_prompt: String,
        schema: Vec<Value>,
        recipient: Recipient<UiEvent>,
    ) -> Result<Vec<Value>> {
        use futures::stream::StreamExt;

        loop {
            let mut stream = client
                .create_message_stream(&messages, &system_prompt, Some(&schema))
                .await
                .context("Failed to create message stream")?;

            let mut tool_calls: Vec<ToolCall> = Vec::new();
            let mut current_text = String::new();

            Self::send_event(&recipient, UiEvent::MessageStart);

            while let Some(event_result) = stream.next().await {
                let event = event_result.context("Stream error")?;

                match event {
                    StreamEvent::TextDelta(text) => {
                        Self::send_event(&recipient, UiEvent::TextDelta(text.clone()));
                        current_text.push_str(&text);
                    },
                    StreamEvent::ToolCallStart { id, name } => {
                        Self::send_event(&recipient, UiEvent::ToolCallStart { id, name });
                    },
                    StreamEvent::ToolCallsReady { calls } => {
                        tool_calls = calls;
                    },
                    StreamEvent::AssistantMessage { text } => {
                        if !text.is_empty() {
                            messages.push(json!({
                                "role": "assistant",
                                "content": [{"type": "text", "text": text}]
                            }));
                        }
                    },
                    StreamEvent::MessageStop => {
                        break;
                    },
                    StreamEvent::Error(error) => {
                        Self::send_event(&recipient, UiEvent::Error(error.clone()));
                        return Err(anyhow::anyhow!(error));
                    },
                    StreamEvent::ToolExecuting { .. }
                    | StreamEvent::ToolResult { .. }
                    | StreamEvent::MessageStart => {},
                }
            }

            Self::send_event(&recipient, UiEvent::MessageStop);

            if tool_calls.is_empty() {
                break;
            }

            let mut content_blocks = vec![json!({
                "type": "text",
                "text": current_text
            })];

            for call in &tool_calls {
                content_blocks.push(json!({
                    "type": "tool_use",
                    "id": call.id,
                    "name": call.name,
                    "input": call.input
                }));
            }

            messages.push(json!({
                "role": "assistant",
                "content": content_blocks
            }));

            for call in tool_calls {
                Self::send_event(
                    &recipient,
                    UiEvent::ToolExecuting {
                        name: call.name.clone(),
                    },
                );

                let result = client
                    .execute_tool(&call)
                    .await
                    .unwrap_or_else(|e| format!("error: {e}"));

                Self::send_event(
                    &recipient,
                    UiEvent::ToolResult {
                        name: call.name.clone(),
                        result: result.clone(),
                    },
                );

                messages.push(json!({
                    "role": "user",
                    "content": [{
                        "type": "tool_result",
                        "tool_use_id": call.id,
                        "content": result
                    }]
                }));
            }
        }

        Ok(messages)
    }
}

impl Actor for SessionActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        tracing::debug!("SessionActor started");
    }
}

impl Handler<ProcessMessage> for SessionActor {
    type Result = ResponseActFuture<Self, Result<()>>;

    fn handle(&mut self, msg: ProcessMessage, _ctx: &mut Self::Context) -> Self::Result {
        let content = msg.content;
        let recipient = msg.ui_recipient;

        self.messages.push(json!({
            "role": "user",
            "content": content,
        }));

        let messages = self.messages.clone();
        let system_prompt = self.system_prompt.clone();
        let schema = self.schema.clone();
        let client = self.client.clone();

        Box::pin(
            async move {
                Self::run_session_loop(client, messages, system_prompt, schema, recipient).await
            }
            .into_actor(self)
            .map(|result, act, _ctx| match result {
                Ok(updated_messages) => {
                    act.messages = updated_messages;
                    Ok(())
                },
                Err(e) => {
                    act.messages.pop();
                    Err(e)
                },
            }),
        )
    }
}

impl Handler<ClearHistory> for SessionActor {
    type Result = ();

    fn handle(&mut self, _msg: ClearHistory, _ctx: &mut Self::Context) {
        self.messages.clear();
        tracing::info!("Conversation history cleared");
    }
}

impl Handler<GetHistory> for SessionActor {
    type Result = Vec<Value>;

    fn handle(&mut self, _msg: GetHistory, _ctx: &mut Self::Context) -> Self::Result {
        self.messages.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_message_content() {
        let content = "Hello".to_string();
        assert_eq!(content, "Hello");
    }

    #[test]
    fn test_clear_history() {
        let _ = ClearHistory;
    }

    #[test]
    fn test_get_history() {
        let _ = GetHistory;
    }
}
