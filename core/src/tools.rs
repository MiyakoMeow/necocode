//! Tool implementations for nanocode.
//!
//! Provides six async tools: read, write, edit, glob, grep, bash.
//!
//! This module defines the tool abstraction layer including:
//! - Tool trait for uniform tool interface
//! - ToolRegistry for centralized tool management

use anyhow::Result;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;

pub mod bash;
pub mod edit;
pub mod glob;
pub mod grep;
pub mod read;
pub mod write;

pub use bash::BashTool;
pub use edit::EditTool;
pub use glob::GlobTool;
pub use grep::GrepTool;
pub use read::ReadTool;
pub use write::WriteTool;

pub use bash::bash_tool;
pub use edit::edit_tool;
pub use glob::glob_tool;
pub use grep::grep_tool;
pub use read::read_tool;
pub use write::write_tool;

/// Tool trait defining the interface for all tools.
///
/// All tools must implement this trait to be registered and executed
/// through the ToolRegistry.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Returns the name of the tool.
    fn name(&self) -> &str;

    /// Returns a description of what the tool does.
    fn description(&self) -> &str;

    /// Returns the JSON Schema for the tool's input parameters.
    fn input_schema(&self) -> Value;

    /// Executes the tool with the given input parameters.
    ///
    /// # Arguments
    ///
    /// * `input` - JSON value containing the tool input parameters
    ///
    /// # Returns
    ///
    /// The result of the tool execution as a string, or an error.
    async fn execute(&self, input: &Value) -> Result<String>;
}

/// Tool registry for managing and executing tools.
///
/// The registry maintains a collection of tools and provides methods
/// for tool registration, execution, and schema retrieval.
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    /// Create a new tool registry with all default tools registered.
    #[must_use]
    pub fn new() -> Self {
        let mut registry = Self {
            tools: HashMap::new(),
        };
        registry.register_all();
        registry
    }

    /// Register all default tools.
    fn register_all(&mut self) {
        self.register(Arc::new(ReadTool));
        self.register(Arc::new(WriteTool));
        self.register(Arc::new(EditTool));
        self.register(Arc::new(GlobTool));
        self.register(Arc::new(GrepTool));
        self.register(Arc::new(BashTool));
    }

    /// Register a tool with the registry.
    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    /// Execute a tool by name with the given input.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the tool to execute
    /// * `input` - JSON value containing the tool input parameters
    ///
    /// # Returns
    ///
    /// The result of the tool execution, or an error if the tool is not found.
    pub async fn execute(&self, name: &str, input: &Value) -> Result<String> {
        self.tools
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("Unknown tool: {name}"))?
            .execute(input)
            .await
    }

    /// Get all tool definitions for API requests.
    ///
    /// # Returns
    ///
    /// A vector of JSON values, each representing a tool definition.
    #[must_use]
    pub fn tool_definitions(&self) -> Vec<Value> {
        self.tools
            .values()
            .map(|tool| {
                json!({
                    "name": tool.name(),
                    "description": tool.description(),
                    "input_schema": tool.input_schema()
                })
            })
            .collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
