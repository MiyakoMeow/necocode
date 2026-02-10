//! File writing tool.

use anyhow::{Context, Result};
use tokio::fs;

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
pub async fn write_tool(path: &str, content: &str) -> Result<String> {
    fs::write(path, content)
        .await
        .with_context(|| format!("Failed to write file: {path}"))?;

    Ok("ok".to_string())
}
