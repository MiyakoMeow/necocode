//! File reading tool with line number support.

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::Value;
use tokio::fs;

use crate::tools::Tool;

/// Read file contents with line numbers.
///
/// # Arguments
///
/// * `path` - File path to read
/// * `offset` - Starting line number (0-based, default 0)
/// * `limit` - Maximum number of lines to read (default: all)
///
/// # Returns
///
/// File contents with line numbers in format "    1| line content"
pub async fn read_tool(path: &str, offset: Option<usize>, limit: Option<usize>) -> Result<String> {
    let content = fs::read_to_string(path)
        .await
        .with_context(|| format!("Failed to read file: {path}"))?;

    let lines: Vec<&str> = content.lines().collect();
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(lines.len());

    let selected: Vec<&str> = lines.iter().skip(offset).take(limit).copied().collect();

    let result = selected
        .iter()
        .enumerate()
        .map(|(idx, line)| format!("{:4}| {}", offset + idx + 1, line))
        .collect::<Vec<_>>()
        .join("\n");

    Ok(result)
}

/// Read tool wrapper.
pub struct ReadTool;

#[async_trait]
impl Tool for ReadTool {
    fn name(&self) -> &'static str {
        "read"
    }

    fn description(&self) -> &'static str {
        "Read a file or directory. If reading a directory, list the files in the directory. If reading a file, this tool will return the contents of the file as a string. This tool is useful for reading code, configuration files, documentation, and any other text-based files. The output includes line numbers to make it easy to reference specific lines."
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "The absolute or relative path to the file or directory to read"
                },
                "offset": {
                    "type": "integer",
                    "description": "The line number to start reading from (1-indexed). Only valid when reading files, not directories. This parameter can be used with limit to read a specific range of lines."
                },
                "limit": {
                    "type": "integer",
                    "description": "The maximum number of lines to read. Only valid when reading files, not directories. This parameter can be used with offset to read a specific range of lines."
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, input: &Value) -> Result<String> {
        let path = input
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing path"))?;
        let offset = input
            .get("offset")
            .and_then(serde_json::Value::as_i64)
            .and_then(|v| usize::try_from(v).ok());
        let limit = input
            .get("limit")
            .and_then(serde_json::Value::as_i64)
            .and_then(|v| usize::try_from(v).ok());
        read_tool(path, offset, limit).await
    }
}
