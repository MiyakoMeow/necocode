//! File editing tool with string replacement.

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::Value;
use tokio::fs;

use crate::tools::Tool;

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
///
/// # Errors
///
/// Returns error if:
/// - File read fails
/// - File write fails
pub async fn edit(path: &str, old: &str, new: &str, all: Option<bool>) -> Result<String> {
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

/// Edit tool wrapper.
pub struct Edit;

#[async_trait]
impl Tool for Edit {
    fn name(&self) -> &'static str {
        "edit"
    }

    fn description(&self) -> &'static str {
        "Edit a file by replacing old string with new string. This tool will replace the first occurrence of the old string with the new string. If there are multiple occurrences of the old string, you must either make the old string more specific to match only once, or set all=true to replace all occurrences. This tool is useful for making small changes to files without rewriting the entire file."
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "The absolute or relative path to the file to edit"
                },
                "old": {
                    "type": "string",
                    "description": "The string to replace. To be successfully replaced, the old string must be unique and must match exactly (including whitespace and indentation)"
                },
                "new": {
                    "type": "string",
                    "description": "The new string to replace the old string with"
                },
                "all": {
                    "type": "boolean",
                    "description": "If true, replace all occurrences of the old string with the new string. If false (default), only replace the first occurrence. Use this only when you want to replace all occurrences."
                }
            },
            "required": ["path", "old", "new"]
        })
    }

    async fn execute(&self, input: &Value) -> Result<String> {
        let path = input
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing path"))?;
        let old = input
            .get("old")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing old"))?;
        let new = input
            .get("new")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing new"))?;
        let all = input.get("all").and_then(serde_json::Value::as_bool);
        edit(path, old, new, all).await
    }
}
