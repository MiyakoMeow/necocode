//! File editing tool with string replacement.

use anyhow::{Context, Result};
use tokio::fs;

/// Edit file by replacing old string with new string.
///
/// # Arguments
///
/// * `path` - File path to edit
/// * `old` - Old string to replace
/// * `new` - New string to replace with
/// * `all` - Replace all occurrences if true, else require unique match
///
/// # Returns
///
/// "ok" on success, error message on failure
pub async fn edit_tool(path: &str, old: &str, new: &str, all: Option<bool>) -> Result<String> {
    let content = fs::read_to_string(path)
        .await
        .with_context(|| format!("Failed to read file: {path}"))?;

    if !content.contains(old) {
        return Ok("error: old_string not found".to_string());
    }

    let count = content.matches(old).count();
    let replace_all = all.unwrap_or(false);

    if !replace_all && count > 1 {
        return Ok(format!(
            "error: old_string appears {count} times, must be unique (use all=true)"
        ));
    }

    let replacement = if replace_all {
        content.replacen(old, new, count)
    } else {
        content.replacen(old, new, 1)
    };

    fs::write(path, replacement)
        .await
        .with_context(|| format!("Failed to write file: {path}"))?;

    Ok("ok".to_string())
}
