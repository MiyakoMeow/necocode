//! Anthropic API client for nanocode.
//!
//! Handles API calls and the agentic loop for tool execution.

pub mod models;
pub mod schema;

use crate::config::ProviderSettings;
use crate::events;
use crate::tools;
use anyhow::Result;
use futures::stream::Stream;
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::mpsc;

/// API error type
#[derive(Debug, Clone, thiserror::Error)]
pub enum ApiError {
    /// Network connection error
    #[error("Network error: {0}")]
    NetworkError(String),

    /// HTTP request error
    #[error("HTTP error {status}: {message}")]
    HttpError {
        /// HTTP status code
        status: u16,
        /// Error message
        message: String,
    },

    /// Data parsing error
    #[error("Parse error: {0}")]
    ParseError(String),

    /// Stream response error
    #[error("Stream error: {0}")]
    StreamError(String),

    /// API returned error
    #[error("API error: {0}")]
    Api(String),
}

/// Stream response event
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// Message start event, indicates the start of stream response
    MessageStart,
    /// Content block start event, contains content block type and index
    ContentBlockStart {
        /// Index position of the content block in the message
        index: u32,
        /// Specific content of the content block
        content_block: ContentBlock,
    },
    /// Content block delta event, contains delta data
    ContentBlockDelta {
        /// Index position of the content block in the message
        index: u32,
        /// Delta data
        delta: Delta,
    },
    /// Content block stop event, indicates a content block is completed
    ContentBlockStop {
        /// Index position of the content block in the message
        index: u32,
    },
    /// Message delta event, contains message-level delta data
    MessageDelta,
    /// Message stop event, indicates the end of stream response
    MessageStop,
    /// Error event, contains API error information
    Error {
        /// Error details
        error: ApiError,
    },
}

/// Content block type
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    /// Text content block, contains plain text content
    #[serde(rename = "text")]
    Text {
        /// Text content
        text: String,
    },
    /// Tool call content block, describes the function call to be executed
    #[serde(rename = "tool_use")]
    ToolUse {
        /// Unique identifier for the tool call
        id: String,
        /// Name of the tool (function)
        name: String,
        /// Input parameters for the tool call
        input: Value,
    },
}

/// Delta data
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum Delta {
    /// Text delta, contains newly added text content
    #[serde(rename = "text_delta")]
    Text {
        /// Incremental text content
        text: String,
    },
    /// JSON delta, contains incremental part of JSON structure data
    #[serde(rename = "input_json_delta")]
    InputJson {
        /// Partial JSON data, used to build complete JSON structure
        partial_json: String,
    },
}

/// SSE event stream type
pub type EventStream = Pin<Box<dyn Stream<Item = Result<StreamEvent, ApiError>> + Send>>;

/// Tool call collector
pub struct ToolCallCollector {
    /// List of pending tool calls
    calls: Vec<PendingToolCall>,
}

/// Pending tool call
#[derive(Debug, Clone)]
struct PendingToolCall {
    /// Unique identifier for the tool call
    id: String,
    /// Name of the tool (function)
    name: String,
    /// Buffer for tool input parameters, used to accumulate incremental JSON data
    input_buffer: String,
    /// Whether processing is completed
    completed: bool,
}

/// Completed tool call
#[derive(Debug, Clone, Serialize)]
pub struct ToolCall {
    /// Unique identifier for the tool call
    pub id: String,
    /// Name of the tool (function)
    pub name: String,
    /// Input parameters for the tool call
    pub input: Value,
}

impl ToolCallCollector {
    /// Create a new tool call collector
    #[must_use]
    pub fn new() -> Self {
        Self { calls: Vec::new() }
    }

    /// Process stream events, collect tool calls
    pub fn process_event(&mut self, event: &StreamEvent) {
        match event {
            StreamEvent::ContentBlockStart {
                content_block: ContentBlock::ToolUse { id, name, input },
                index,
            } => {
                // Extend calls vector to accommodate new index
                while self.calls.len() <= *index as usize {
                    self.calls.push(PendingToolCall {
                        id: String::new(),
                        name: String::new(),
                        input_buffer: String::new(),
                        completed: false,
                    });
                }

                if let Some(call) = self.calls.get_mut(*index as usize) {
                    *call = PendingToolCall {
                        id: id.clone(),
                        name: name.clone(),
                        input_buffer: match input {
                            Value::String(s) => s.clone(),
                            Value::Object(map) if !map.is_empty() => input.to_string(),
                            _ => String::new(),
                        },
                        completed: false,
                    };
                }
            },
            StreamEvent::ContentBlockStart {
                content_block: ContentBlock::Text { .. },
                ..
            }
            | StreamEvent::MessageStart
            | StreamEvent::MessageDelta
            | StreamEvent::MessageStop
            | StreamEvent::Error { .. } => {}, // These events don't need special handling

            StreamEvent::ContentBlockDelta { delta, index } => {
                let Some(call) = self.calls.get_mut(*index as usize) else {
                    return;
                };
                let Delta::InputJson { partial_json } = delta else {
                    return;
                };
                call.input_buffer.push_str(partial_json);
            },

            StreamEvent::ContentBlockStop { index } => {
                let Some(call) = self.calls.get_mut(*index as usize) else {
                    return;
                };
                call.completed = true;
            },
        }
    }

    /// Check if there are completed tool calls
    #[must_use]
    pub fn has_completed_calls(&self) -> bool {
        self.calls.iter().any(|c| c.completed)
    }

    /// Extract all completed tool calls and clear
    pub fn take_completed(&mut self) -> Vec<ToolCall> {
        let completed = self
            .calls
            .iter()
            .filter(|c| c.completed)
            .map(|c| ToolCall {
                id: c.id.clone(),
                name: c.name.clone(),
                input: serde_json::from_str(&c.input_buffer).unwrap_or_default(),
            })
            .collect();

        self.calls.clear();
        completed
    }

    /// Check if the collector is active (has pending tool calls)
    #[must_use]
    pub fn is_active(&self) -> bool {
        !self.calls.is_empty()
    }
}

impl Default for ToolCallCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse SSE response stream
fn parse_sse_stream(response: reqwest::Response) -> EventStream {
    use futures::stream::StreamExt;

    Box::pin(async_stream::stream! {
            let mut buffer = String::new();
            let mut stream = response.bytes_stream();

            while let Some(chunk_result) = stream.next().await {
                let chunk = match chunk_result {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        yield Err(ApiError::StreamError(format!("Failed to read stream: {e}")));
                        continue;
                    }
                };

                // Convert bytes to string and process
                let chunk_str = match String::from_utf8(chunk.to_vec()) {
                    Ok(s) => s,
                    Err(e) => {
                        yield Err(ApiError::ParseError(format!("Invalid UTF-8: {e}")));
                        continue;
                    }
                };

                buffer.push_str(&chunk_str);

                // Split by lines
                while let Some(newline_pos) = buffer.find('\n') {
                    let line = buffer[..newline_pos].to_string();
                    buffer = buffer[newline_pos + 1..].to_string();

                    // Skip empty lines
                    if line.is_empty() {
                        continue;
                    }

                    // SSE format parsing
                    if let Some(data) = line.strip_prefix("data: ") {
                        // Skip "[DONE]" marker
                        if data == "[DONE]" {
                            yield Ok(StreamEvent::MessageStop);
                            continue;
                        }

                        // Parse JSON
                        match serde_json::from_str::<Value>(data) {
                            Ok(value) => {
                                if let Some(event_type) = value.get("type").and_then(|v| v.as_str()) {
                                    let event = match event_type {
                                        "message_start" => StreamEvent::MessageStart,
                                        "content_block_start" => {
                                            if let Some(block) = value.get("content_block") {
    StreamEvent::ContentBlockStart {
                                            index: value.get("index").and_then(Value::as_u64).unwrap_or(0).try_into().unwrap_or(0),
                                            content_block: serde_json::from_value(block.clone())
                                                .unwrap_or(ContentBlock::Text { text: String::new() }),
                                        }
                                            } else {
                                                continue;
                                            }
                                        }
                                        "content_block_delta" => {
    StreamEvent::ContentBlockDelta {
                                                index: value.get("index").and_then(Value::as_u64).unwrap_or(0).try_into().unwrap_or(0),
                                                delta: serde_json::from_value(value.get("delta").cloned().unwrap_or_default())
                                                    .unwrap_or(Delta::Text { text: String::new() }),
                                            }
                                        }
    "content_block_stop" => StreamEvent::ContentBlockStop {
                                            index: value.get("index").and_then(Value::as_u64).unwrap_or(0).try_into().unwrap_or(0),
                                        },
                                        "message_delta" => StreamEvent::MessageDelta,
                                        "message_stop" => StreamEvent::MessageStop,
                                        "error" => StreamEvent::Error {
                                            error: ApiError::Api(
                                                value.get("error")
                                                    .and_then(|e| e.get("message"))
                                                    .and_then(|m| m.as_str())
                                                    .unwrap_or("Unknown error")
                                                    .to_string()
                                            ),
                                        },
                                        _ => continue,
                                    };
                                    yield Ok(event);
                                }
                            }
                            Err(e) => {
                                yield Err(ApiError::ParseError(format!("Failed to parse SSE data: {e}")));
                            }
                        }
                    }
                }
            }
        })
}

/// API client.
pub struct Client {
    /// HTTP client for making API requests
    http: HttpClient,
    /// API configuration, contains key, base URL and other information
    config: ProviderSettings,
    /// Tool registry for executing tools
    tool_registry: Arc<tools::ToolRegistry>,
}

impl Client {
    /// Create a new API client with the given configuration.
    #[must_use]
    pub fn new(config: ProviderSettings) -> Self {
        Self {
            http: HttpClient::new(),
            config,
            tool_registry: Arc::new(tools::ToolRegistry::new()),
        }
    }

    /// Send stream message request
    ///
    /// # Arguments
    ///
    /// * `messages` - Conversation history
    /// * `system_prompt` - System prompt for the model
    /// * `tools` - Optional tool definitions
    ///
    /// # Returns
    ///
    /// Stream of events from the API
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Network request fails
    /// - API returns error response
    /// - Response parsing fails
    pub async fn create_message_stream(
        &self,
        messages: &[Value],
        system_prompt: &str,
        tools: Option<&[Value]>,
    ) -> Result<EventStream, ApiError> {
        let mut request_body = json!({
            "model": self.config.model,
            "max_tokens": 8192,
            "system": system_prompt,
            "messages": messages,
            "stream": true,
        });

        if let Some(tools_value) = tools
            && let Some(body_obj) = request_body.as_object_mut()
        {
            body_obj.insert("tools".to_string(), json!(tools_value));
        }

        let response = self
            .http
            .post(format!("{}/v1/messages", self.config.base_url))
            .header("x-api-key", &self.config.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&request_body)
            .send()
            .await
            .map_err(|e| ApiError::NetworkError(e.to_string()))?;

        // Check status code
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(ApiError::HttpError {
                status: status.as_u16(),
                message: error_text,
            });
        }

        // Return parsed SSE stream
        Ok(parse_sse_stream(response))
    }

    /// Run the agentic loop: keep calling API until no more tool calls.
    ///
    /// # Arguments
    ///
    /// * `messages` - Mutable reference to conversation history
    /// * `system_prompt` - System prompt for the model
    /// * `tools` - Tool definitions
    /// * `event_sender` - Optional sender for core events
    ///
    /// # Returns
    ///
    /// Ok(()) on success, Err on failure
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - API request fails
    /// - Tool execution fails
    /// - Response processing fails
    pub async fn run_agent_loop_stream(
        &self,
        messages: &mut Vec<Value>,
        system_prompt: &str,
        tools: &[Value],
        event_sender: Option<&mpsc::UnboundedSender<events::CoreEvent>>,
    ) -> Result<(), ApiError> {
        use futures::stream::StreamExt;

        let mut tool_collector = ToolCallCollector::new();
        // Initial check of collector state
        let _ = !tool_collector.is_active();
        let mut current_text = String::new();

        loop {
            // Send message start event
            if let Some(sender) = event_sender {
                let _ = sender.send(events::CoreEvent::MessageStart);
            }

            // Create stream request
            let mut stream = self
                .create_message_stream(messages, system_prompt, Some(tools))
                .await?;

            // Process stream events
            while let Some(event_result) = stream.next().await {
                let event = event_result?;

                match &event {
                    StreamEvent::ContentBlockDelta {
                        delta: Delta::Text { text },
                        ..
                    } => {
                        // Send text delta event
                        if let Some(sender) = event_sender {
                            let _ = sender.send(events::CoreEvent::TextDelta(text.clone()));
                        }
                        current_text.push_str(text);
                    },

                    StreamEvent::ContentBlockStart {
                        content_block: ContentBlock::ToolUse { id, name, .. },
                        ..
                    } => {
                        // Send tool call start event
                        if let Some(sender) = event_sender {
                            let _ = sender.send(events::CoreEvent::ToolCallStart {
                                id: id.clone(),
                                name: name.clone(),
                            });
                        }
                    },

                    StreamEvent::Error { error } => {
                        // Send error event
                        if let Some(sender) = event_sender {
                            let _ = sender.send(events::CoreEvent::Error(error.to_string()));
                        }
                    },

                    StreamEvent::MessageStop => {
                        // Send message stop event
                        if let Some(sender) = event_sender {
                            let _ = sender.send(events::CoreEvent::MessageStop);
                        }
                        break;
                    },

                    _ => {
                        // Other events don't need special handling
                    },
                }

                // Process event for tool collection after match
                tool_collector.process_event(&event);
            }

            // Check if there are completed tool calls
            if tool_collector.has_completed_calls() {
                let tool_calls = tool_collector.take_completed();

                // Build assistant message content
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

                // Save assistant message
                messages.push(json!({
                    "role": "assistant",
                    "content": content_blocks
                }));

                // Execute tools
                for call in tool_calls {
                    // Send tool execution start event
                    if let Some(sender) = event_sender {
                        let _ = sender.send(events::CoreEvent::ToolExecuting {
                            name: call.name.clone(),
                        });
                    }

                    let result = self
                        .run_tool(
                            &call.name,
                            call.input.as_object().ok_or_else(|| {
                                ApiError::ParseError(format!(
                                    "Tool input is not an object for tool: {}",
                                    call.name
                                ))
                            })?,
                        )
                        .await;

                    // Send tool result event
                    if let Some(sender) = event_sender {
                        let _ = sender.send(events::CoreEvent::ToolResult {
                            name: call.name.clone(),
                            result: result.clone(),
                        });
                    }

                    // Add tool result to message history
                    messages.push(json!({
                        "role": "user",
                        "content": [{
                            "type": "tool_result",
                            "tool_use_id": call.id,
                            "content": result
                        }]
                    }));
                }

                // Clear current text and continue loop
                current_text = String::new();
            } else if !current_text.is_empty() {
                // No tool calls, save final reply and exit
                messages.push(json!({
                    "role": "assistant",
                    "content": [{
                        "type": "text",
                        "text": current_text
                    }]
                }));
                break;
            } else {
                break;
            }
        }

        Ok(())
    }

    /// Execute a single tool call.
    ///
    /// # Arguments
    ///
    /// * `name` - Tool name
    /// * `input` - Tool input parameters
    ///
    /// # Returns
    ///
    /// Tool result as string, or error message
    async fn run_tool(&self, name: &str, input: &serde_json::Map<String, Value>) -> String {
        let input_value = json!(input);
        match self.tool_registry.execute(name, &input_value).await {
            Ok(result) => result,
            Err(e) => format!("error: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_block_deserialize() {
        let text_json = json!({"type": "text", "text": "Hello"});
        let text: ContentBlock = serde_json::from_value(text_json).unwrap();
        match text {
            ContentBlock::Text { text: t } => assert_eq!(t, "Hello"),
            ContentBlock::ToolUse { .. } => {
                panic!("Expected Text variant but got ToolUse in test");
            },
        }

        let tool_json = json!({
            "type": "tool_use",
            "id": "call_123",
            "name": "read",
            "input": {"path": "test.rs"}
        });
        let tool: ContentBlock = serde_json::from_value(tool_json).unwrap();
        match tool {
            ContentBlock::ToolUse { id, name, .. } => {
                assert_eq!(id, "call_123");
                assert_eq!(name, "read");
            },
            ContentBlock::Text { .. } => {
                panic!("Expected ToolUse variant but got Text in test");
            },
        }
    }

    #[test]
    fn test_delta_deserialize() {
        let text_delta_json = json!({"type": "text_delta", "text": "Hello"});
        let delta: Delta = serde_json::from_value(text_delta_json).unwrap();
        match delta {
            Delta::Text { text } => assert_eq!(text, "Hello"),
            Delta::InputJson { .. } => {
                panic!("Expected Text variant but got InputJson in test");
            },
        }

        let json_delta_json = json!({"type": "input_json_delta", "partial_json": "{}"});
        let delta: Delta = serde_json::from_value(json_delta_json).unwrap();
        match delta {
            Delta::InputJson { partial_json } => assert_eq!(partial_json, "{}"),
            Delta::Text { .. } => {
                panic!("Expected InputJson variant but got Text in test");
            },
        }
    }

    #[test]
    fn test_tool_call_collector_multiple() {
        let mut collector = ToolCallCollector::new();

        // Simulate first tool call
        collector.process_event(&StreamEvent::ContentBlockStart {
            index: 0,
            content_block: ContentBlock::ToolUse {
                id: "call_1".to_string(),
                name: "read".to_string(),
                input: json!(""),
            },
        });

        collector.process_event(&StreamEvent::ContentBlockDelta {
            index: 0,
            delta: Delta::InputJson {
                partial_json: r#"{"path":"file1.rs"}"#.to_string(),
            },
        });

        collector.process_event(&StreamEvent::ContentBlockStop { index: 0 });

        // Simulate second tool call
        collector.process_event(&StreamEvent::ContentBlockStart {
            index: 1,
            content_block: ContentBlock::ToolUse {
                id: "call_2".to_string(),
                name: "write".to_string(),
                input: json!(""),
            },
        });

        collector.process_event(&StreamEvent::ContentBlockDelta {
            index: 1,
            delta: Delta::InputJson {
                partial_json: r#"{"path":"file2.rs","content":"test"}"#.to_string(),
            },
        });

        collector.process_event(&StreamEvent::ContentBlockStop { index: 1 });

        // Verify collection
        assert!(collector.has_completed_calls());
        let calls = collector.take_completed();
        assert_eq!(calls.len(), 2);
        // Use let-else pattern instead of if-else
        let Some(first_call) = calls.first() else {
            return; // Empty vector: test passes early
        };
        let Some(second_call) = calls.get(1) else {
            return; // Single element vector: test passes early
        };
        assert_eq!(first_call.name, "read");
        assert_eq!(second_call.name, "write");
    }

    #[test]
    fn test_tool_call_collector_incomplete() {
        let mut collector = ToolCallCollector::new();

        // Simulate tool call start but without end
        collector.process_event(&StreamEvent::ContentBlockStart {
            index: 0,
            content_block: ContentBlock::ToolUse {
                id: "call_123".to_string(),
                name: "read".to_string(),
                input: json!(""),
            },
        });

        collector.process_event(&StreamEvent::ContentBlockDelta {
            index: 0,
            delta: Delta::InputJson {
                partial_json: r#"{"file_path":"test.rs"}"#.to_string(),
            },
        });

        // Verify incomplete
        assert!(!collector.has_completed_calls());
        assert!(collector.is_active());
    }
}
