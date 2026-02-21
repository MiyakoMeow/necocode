//! File pattern matching tool using glob patterns.

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::Value;
use tracing;

use crate::tools::Tool;

/// Find files matching a glob pattern, sorted by modification time.
///
/// # Arguments
///
/// * `pat` - Glob pattern (e.g., "**/*.rs")
/// * `path` - Base directory for search (default: ".")
///
/// # Returns
///
/// Newline-separated list of matching files, or "none" if no matches
///
/// # Errors
///
/// Returns error if glob pattern is invalid.
#[allow(clippy::module_name_repetitions)]
pub fn glob_tool(pat: &str, path: Option<&str>) -> Result<String> {
    let base = path.unwrap_or(".");
    let pattern = format!("{}/{}", base.replace('\\', "/"), pat).replace("//", "/");

    let mut files = Vec::new();

    for entry in
        glob::glob(&pattern).with_context(|| format!("Failed to read glob pattern: {pattern}"))?
    {
        match entry {
            Ok(path) => {
                if path.is_file() {
                    let mtime =
                        path.metadata()
                            .ok()
                            .and_then(|m| m.modified().ok())
                            .map_or(0, |t| {
                                t.duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs()
                            });
                    files.push((path, mtime));
                }
            },
            Err(e) => {
                tracing::error!("Glob error: {e}");
            },
        }
    }

    // Sort by modification time (newest first)
    files.sort_by(|a, b| b.1.cmp(&a.1));

    if files.is_empty() {
        Ok("none".to_string())
    } else {
        let paths: Vec<String> = files
            .into_iter()
            .map(|(p, _)| p.display().to_string().replace('\\', "/"))
            .collect();
        Ok(paths.join("\n"))
    }
}

/// Glob tool wrapper.
#[allow(clippy::module_name_repetitions)]
pub struct GlobTool;

#[async_trait]
impl Tool for GlobTool {
    fn name(&self) -> &'static str {
        "glob"
    }

    fn description(&self) -> &'static str {
        "Fast file pattern matching tool that works with any codebase size and supports glob patterns like \"**/*.js\" or \"src/**/*.ts\". Returns matching file paths sorted by modification time. Use this tool when you need to find files by name patterns. This is especially useful for exploring codebase structure or finding files matching specific naming conventions."
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pat": {
                    "type": "string",
                    "description": "The glob pattern to match files against. Examples: \"**/*.rs\" to find all Rust files, \"src/**/*.ts\" for TypeScript files in src directory"
                },
                "path": {
                    "type": "string",
                    "description": "The directory to search in. Defaults to current directory if not specified"
                }
            },
            "required": ["pat"]
        })
    }

    async fn execute(&self, input: &Value) -> Result<String> {
        let pat = input
            .get("pat")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing pat"))?
            .to_string();
        let path = input
            .get("path")
            .and_then(|v| v.as_str())
            .map(std::string::ToString::to_string);

        tokio::task::spawn_blocking(move || glob_tool(&pat, path.as_deref()))
            .await
            .context("Task join error")?
    }
}
