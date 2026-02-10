//! File reading tool with line number support.

use anyhow::{Context, Result};
use tokio::fs;

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
