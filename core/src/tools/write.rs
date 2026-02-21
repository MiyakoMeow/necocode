//! File writing tool.

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::Value;
use tokio::fs;

use crate::tools::Tool;

/// Write content to a file.
///
/// # Arguments
///
/// * `path` - File path to write
/// * `content` - Content to write
///
/// # Returns
///
/// "ok" on success
///
/// # Errors
///
/// Returns error if:
/// - File write fails
pub async fn write(path: &str, content: &str) -> Result<String> {
    fs::write(path, content)
        .await
        .with_context(|| format!("Failed to write file: {path}"))?;

    Ok("ok".to_string())
}

/// Write tool wrapper.
pub struct Write;

#[async_trait]
impl Tool for Write {
    fn name(&self) -> &'static str {
        "write"
    }

    fn description(&self) -> &'static str {
        "Write content to a file. This tool will create the file if it does not exist, or overwrite the file if it already exists. This tool is useful for creating new files, modifying existing files, and saving code. Use this tool when you need to create or modify files in the codebase."
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "The absolute or relative path to the file to write"
                },
                "content": {
                    "type": "string",
                    "description": "The content to write to the file"
                }
            },
            "required": ["path", "content"]
        })
    }

    async fn execute(&self, input: &Value) -> Result<String> {
        let path = input
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing path"))?;
        let content = input
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing content"))?;
        write(path, content).await
    }
}
