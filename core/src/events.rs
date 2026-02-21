//! Core event messages for nanocode using Actix Actor model.
//!
//! Defines the message types used for communication between actors
//! in the actor-based event system.

use actix::{Message, Recipient};
use serde::{Deserialize, Serialize};

/// Core event enumeration for UI updates.
#[derive(Debug, Clone, Serialize, Deserialize, Message)]
#[rtype(result = "()")]
pub enum UiEvent {
    /// Text delta event for incremental text output.
    TextDelta(String),
    /// Tool call start event.
    ToolCallStart {
        /// Unique identifier for the tool call.
        id: String,
        /// Name of the tool.
        name: String,
    },
    /// Tool executing event.
    ToolExecuting {
        /// Name of the tool.
        name: String,
    },
    /// Tool result event.
    ToolResult {
        /// Name of the tool.
        name: String,
        /// Result content.
        result: String,
    },
    /// Error event.
    Error(String),
    /// Message start event.
    MessageStart,
    /// Message stop event.
    MessageStop,
}

/// Type alias for UI event recipient.
pub type UiRecipient = Recipient<UiEvent>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ui_event_serialization() {
        let event = UiEvent::TextDelta("Hello, world!".to_string());
        let serialized = serde_json::to_string(&event).unwrap();
        let deserialized: UiEvent = serde_json::from_str(&serialized).unwrap();
        match deserialized {
            UiEvent::TextDelta(text) => assert_eq!(text, "Hello, world!"),
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn test_ui_event_tool_call_start() {
        let event = UiEvent::ToolCallStart {
            id: "test-id".to_string(),
            name: "test-tool".to_string(),
        };
        let serialized = serde_json::to_string(&event).unwrap();
        let deserialized: UiEvent = serde_json::from_str(&serialized).unwrap();
        match deserialized {
            UiEvent::ToolCallStart { id, name } => {
                assert_eq!(id, "test-id");
                assert_eq!(name, "test-tool");
            },
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn test_ui_event_tool_result() {
        let event = UiEvent::ToolResult {
            name: "read".to_string(),
            result: "File content".to_string(),
        };
        let serialized = serde_json::to_string(&event).unwrap();
        let deserialized: UiEvent = serde_json::from_str(&serialized).unwrap();
        match deserialized {
            UiEvent::ToolResult { name, result } => {
                assert_eq!(name, "read");
                assert_eq!(result, "File content");
            },
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn test_ui_event_error() {
        let event = UiEvent::Error("Something went wrong".to_string());
        let serialized = serde_json::to_string(&event).unwrap();
        let deserialized: UiEvent = serde_json::from_str(&serialized).unwrap();
        match deserialized {
            UiEvent::Error(msg) => assert_eq!(msg, "Something went wrong"),
            _ => panic!("Wrong event type"),
        }
    }
}
