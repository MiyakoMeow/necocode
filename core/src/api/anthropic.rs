//! Anthropic API client for nanocode using Actor model.
//!
//! Handles API calls and the agentic loop for tool execution.

pub mod models;
pub mod schema;

use crate::config::ProviderSettings;
use crate::tools;
use anyhow::Result;
use futures::stream::Stream;
use reqwest::Client as HttpClient;
use serde::Deserialize;
use serde_json::{Value, json};
use std::pin::Pin;
use std::sync::Arc;

/// API error type.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ApiError {
    /// Network connection error.
    #[error("Network error: {0}")]
    NetworkError(String),

    /// HTTP request error.
    #[error("HTTP error {status}: {message}")]
    HttpError {
        /// HTTP status code.
        status: u16,
        /// Error message.
        message: String,
    },

    /// Data parsing error.
    #[error("Parse error: {0}")]
    ParseError(String),

    /// Stream response error.
    #[error("Stream error: {0}")]
    StreamError(String),

    /// API returned error.
    #[error("API error: {0}")]
    Api(String),
}

/// Stream event type for actor communication.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// Text delta event for incremental text output.
    TextDelta(String),
    /// Tool call start event.
    ToolCallStart {
        /// Unique identifier for the tool call.
        id: String,
        /// Name of the tool being called.
        name: String,
    },
    /// Tool executing event.
    ToolExecuting {
        /// Name of the tool being executed.
        name: String,
    },
    /// Tool result event.
    ToolResult {
        /// Name of the tool.
        name: String,
        /// Result of the tool execution.
        result: String,
    },
    /// Error event.
    Error(String),
    /// Message start event.
    MessageStart,
    /// Message stop event.
    MessageStop,
    /// Final assistant message.
    AssistantMessage {
        /// Text content.
        text: String,
    },
    /// Tool calls ready to execute.
    ToolCallsReady {
        /// Tool calls to execute.
        calls: Vec<ToolCall>,
    },
}

/// SSE event stream type.
pub type EventStream = Pin<Box<dyn Stream<Item = Result<StreamEvent, ApiError>> + Send>>;

/// Content block type.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    /// Text content block.
    #[serde(rename = "text")]
    Text {
        /// Text content.
        text: String,
    },
    /// Tool call content block.
    #[serde(rename = "tool_use")]
    ToolUse {
        /// Unique identifier for the tool call.
        id: String,
        /// Name of the tool.
        name: String,
        /// Input parameters.
        input: Value,
    },
}

/// Delta data type.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum Delta {
    /// Text delta.
    #[serde(rename = "text_delta")]
    Text {
        /// Incremental text content.
        text: String,
    },
    /// JSON delta for tool input.
    #[serde(rename = "input_json_delta")]
    InputJson {
        /// Partial JSON data.
        partial_json: String,
    },
}

/// Tool call structure.
#[derive(Debug, Clone)]
pub struct ToolCall {
    /// Unique identifier.
    pub id: String,
    /// Tool name.
    pub name: String,
    /// Input parameters.
    pub input: Value,
}

/// Tool call collector for aggregating tool calls from stream.
pub struct ToolCallCollector {
    /// Vector of pending tool calls being collected.
    calls: Vec<PendingToolCall>,
}

/// Pending tool call being aggregated from stream events.
#[derive(Debug, Clone)]
struct PendingToolCall {
    /// Unique identifier for the tool call.
    id: String,
    /// Name of the tool to execute.
    name: String,
    /// Buffer for accumulating JSON input data.
    input_buffer: String,
    /// Whether the tool call has been fully received.
    completed: bool,
}

impl ToolCallCollector {
    /// Create a new tool call collector.
    #[must_use]
    pub fn new() -> Self {
        Self { calls: Vec::new() }
    }

    /// Process an internal event and update tool calls.
    fn process_event(&mut self, event: &InternalEvent) {
        match event {
            InternalEvent::ContentBlockStart {
                content_block: ContentBlock::ToolUse { id, name, input },
                index,
            } => {
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
            InternalEvent::ContentBlockStart {
                content_block: ContentBlock::Text { .. },
                ..
            }
            | InternalEvent::MessageStart
            | InternalEvent::MessageDelta
            | InternalEvent::MessageStop
            | InternalEvent::Error { .. } => {},

            InternalEvent::ContentBlockDelta { delta, index } => {
                let Some(call) = self.calls.get_mut(*index as usize) else {
                    return;
                };
                let Delta::InputJson { partial_json } = delta else {
                    return;
                };
                call.input_buffer.push_str(partial_json);
            },

            InternalEvent::ContentBlockStop { index } => {
                let Some(call) = self.calls.get_mut(*index as usize) else {
                    return;
                };
                call.completed = true;
            },
        }
    }

    /// Check if there are completed tool calls.
    #[must_use]
    pub fn has_completed_calls(&self) -> bool {
        self.calls.iter().any(|c| c.completed)
    }

    /// Extract all completed tool calls.
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

    /// Check if the collector is active.
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

/// Internal event types parsed from SSE stream.
#[derive(Debug, Clone)]
enum InternalEvent {
    /// Message start event from API.
    MessageStart,
    /// Content block start event with index and content.
    ContentBlockStart {
        /// Index of the content block.
        index: u32,
        /// The content block (text or tool use).
        content_block: ContentBlock,
    },
    /// Content block delta event with incremental data.
    ContentBlockDelta {
        /// Index of the content block.
        index: u32,
        /// Delta data (text or JSON).
        delta: Delta,
    },
    /// Content block stop event.
    ContentBlockStop {
        /// Index of the content block.
        index: u32,
    },
    /// Message delta event.
    MessageDelta,
    /// Message stop event.
    MessageStop,
    /// Error event from API.
    Error {
        /// The error details.
        error: ApiError,
    },
}

/// Parse SSE response stream into internal events.
fn parse_sse_stream(
    response: reqwest::Response,
) -> Pin<Box<dyn Stream<Item = Result<InternalEvent, ApiError>> + Send>> {
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

            let chunk_str = match String::from_utf8(chunk.to_vec()) {
                Ok(s) => s,
                Err(e) => {
                    yield Err(ApiError::ParseError(format!("Invalid UTF-8: {e}")));
                    continue;
                }
            };

            buffer.push_str(&chunk_str);

            while let Some(newline_pos) = buffer.find('\n') {
                let line = buffer[..newline_pos].to_string();
                buffer = buffer[newline_pos + 1..].to_string();

                if line.is_empty() {
                    continue;
                }

                if let Some(data) = line.strip_prefix("data: ") {
                    if data == "[DONE]" {
                        yield Ok(InternalEvent::MessageStop);
                        continue;
                    }

                    match serde_json::from_str::<Value>(data) {
                        Ok(value) => {
                            if let Some(event_type) = value.get("type").and_then(|v| v.as_str()) {
                                let event = match event_type {
                                    "message_start" => InternalEvent::MessageStart,
                                    "content_block_start" => {
                                        if let Some(block) = value.get("content_block") {
                                            InternalEvent::ContentBlockStart {
                                                index: value.get("index")
                                                    .and_then(Value::as_u64)
                                                    .unwrap_or(0)
                                                    .try_into()
                                                    .unwrap_or(0),
                                                content_block: serde_json::from_value(block.clone())
                                                    .unwrap_or(ContentBlock::Text { text: String::new() }),
                                            }
                                        } else {
                                            continue;
                                        }
                                    },
                                    "content_block_delta" => {
                                        InternalEvent::ContentBlockDelta {
                                            index: value.get("index")
                                                .and_then(Value::as_u64)
                                                .unwrap_or(0)
                                                .try_into()
                                                .unwrap_or(0),
                                            delta: serde_json::from_value(
                                                value.get("delta").cloned().unwrap_or_default()
                                            ).unwrap_or(Delta::Text { text: String::new() }),
                                        }
                                    },
                                    "content_block_stop" => InternalEvent::ContentBlockStop {
                                        index: value.get("index")
                                            .and_then(Value::as_u64)
                                            .unwrap_or(0)
                                            .try_into()
                                            .unwrap_or(0),
                                    },
                                    "message_delta" => InternalEvent::MessageDelta,
                                    "message_stop" => InternalEvent::MessageStop,
                                    "error" => InternalEvent::Error {
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

/// API client for communicating with LLM providers.
#[derive(Clone)]
pub struct Client {
    /// HTTP client for making requests.
    http: HttpClient,
    /// Provider configuration (API key, base URL, model).
    config: ProviderSettings,
    /// Registry of available tools.
    tool_registry: Arc<tools::ToolRegistry>,
}

impl Client {
    /// Create a new API client.
    #[must_use]
    pub fn new(config: ProviderSettings) -> Self {
        Self {
            http: HttpClient::new(),
            config,
            tool_registry: Arc::new(tools::ToolRegistry::new()),
        }
    }

    /// Create a client with existing HTTP client.
    #[must_use]
    pub fn with_http(config: ProviderSettings, http: HttpClient) -> Self {
        Self {
            http,
            config,
            tool_registry: Arc::new(tools::ToolRegistry::new()),
        }
    }

    /// Get the tool registry.
    #[must_use]
    pub fn tool_registry(&self) -> &tools::ToolRegistry {
        &self.tool_registry
    }

    /// Get the configuration.
    #[must_use]
    pub fn config(&self) -> &ProviderSettings {
        &self.config
    }

    /// Create a message stream.
    ///
    /// # Errors
    ///
    /// Returns error if network request fails.
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

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(ApiError::HttpError {
                status: status.as_u16(),
                message: error_text,
            });
        }

        Ok(Self::process_stream_events(parse_sse_stream(response)))
    }

    /// Process internal event stream into public stream events.
    ///
    /// Converts low-level SSE events into high-level stream events for consumption.
    fn process_stream_events(
        internal_stream: Pin<Box<dyn Stream<Item = Result<InternalEvent, ApiError>> + Send>>,
    ) -> EventStream {
        use futures::stream::StreamExt;

        Box::pin(async_stream::stream! {
            let mut internal_stream = internal_stream;
            let mut tool_collector = ToolCallCollector::new();
            let mut current_text = String::new();

            yield Ok(StreamEvent::MessageStart);

            while let Some(event_result) = internal_stream.next().await {
                let event = match event_result {
                    Ok(e) => e,
                    Err(e) => {
                        yield Err(e);
                        continue;
                    }
                };

                tool_collector.process_event(&event);

                match &event {
                    InternalEvent::ContentBlockDelta {
                        delta: Delta::Text { text },
                        ..
                    } => {
                        yield Ok(StreamEvent::TextDelta(text.clone()));
                        current_text.push_str(text);
                    },
                    InternalEvent::ContentBlockStart {
                        content_block: ContentBlock::ToolUse { id, name, .. },
                        ..
                    } => {
                        yield Ok(StreamEvent::ToolCallStart {
                            id: id.clone(),
                            name: name.clone(),
                        });
                    },
                    InternalEvent::Error { error } => {
                        yield Ok(StreamEvent::Error(error.to_string()));
                    },
                    InternalEvent::MessageStop => {
                        if tool_collector.has_completed_calls() {
                            let calls = tool_collector.take_completed();
                            yield Ok(StreamEvent::ToolCallsReady { calls });
                        } else {
                            yield Ok(StreamEvent::AssistantMessage { text: current_text.clone() });
                            yield Ok(StreamEvent::MessageStop);
                        }
                        current_text.clear();
                        break;
                    },
                    _ => {},
                }
            }
        })
    }

    /// Execute a tool call.
    ///
    /// # Errors
    ///
    /// Returns error if tool execution fails.
    pub async fn execute_tool(&self, call: &ToolCall) -> Result<String> {
        let input_value = json!(call.input);
        self.tool_registry.execute(&call.name, &input_value).await
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
    fn test_tool_call_collector() {
        let mut collector = ToolCallCollector::new();

        collector.process_event(&InternalEvent::ContentBlockStart {
            index: 0,
            content_block: ContentBlock::ToolUse {
                id: "call_1".to_string(),
                name: "read".to_string(),
                input: json!(""),
            },
        });

        collector.process_event(&InternalEvent::ContentBlockDelta {
            index: 0,
            delta: Delta::InputJson {
                partial_json: r#"{"path":"test.rs"}"#.to_string(),
            },
        });

        collector.process_event(&InternalEvent::ContentBlockStop { index: 0 });

        assert!(collector.has_completed_calls());
        let calls = collector.take_completed();
        assert_eq!(calls.len(), 1);
        assert!(calls.first().is_some_and(|c| c.name == "read"));
    }

    #[test]
    fn test_stream_event_debug() {
        let event = StreamEvent::TextDelta("Hello".to_string());
        let _ = format!("{event:?}");
    }
}
