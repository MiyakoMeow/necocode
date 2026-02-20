//! Regex search tool for file contents.

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::Value;

use crate::tools::Tool;

/// Search files for regex pattern matches.
///
/// # Arguments
///
/// * `pat` - Regular expression pattern
/// * `path` - Base directory for search (default: ".")
///
/// # Returns
///
/// Newline-separated matches in format "path:line:content", up to 50 matches
pub async fn grep_tool(pat: &str, path: Option<&str>) -> Result<String> {
    let base = path.unwrap_or(".").to_string();
    let pattern =
        regex::Regex::new(pat).with_context(|| format!("Invalid regex pattern: {pat}"))?;

    tokio::task::spawn_blocking(move || {
        let mut hits = Vec::new();

        for entry in walkdir::WalkDir::new(&base)
            .follow_links(true)
            .max_depth(100)
            .into_iter()
            .filter_map(std::result::Result::ok)
        {
            let filepath = entry.path();
            if filepath.is_file()
                && let Ok(content) = std::fs::read_to_string(filepath)
            {
                for (line_num, line) in content.lines().enumerate() {
                    if pattern.is_match(line) {
                        hits.push(format!(
                            "{}:{}:{}",
                            filepath.display().to_string().replace('\\', "/"),
                            line_num + 1,
                            line.trim()
                        ));
                    }
                }
            }
        }

        if hits.is_empty() {
            Ok("none".to_string())
        } else {
            Ok(hits.iter().take(50).cloned().collect::<Vec<_>>().join("\n"))
        }
    })
    .await
    .context("Task join error")?
}

/// Grep tool wrapper.
pub struct GrepTool;

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str {
        "grep"
    }

    fn description(&self) -> &str {
        "Fast content search tool that works with any codebase size. Searches file contents using regular expressions and supports full regex syntax (eg \"log.*Error\", \"function\\s+\\w+\", etc.). Returns file paths and line numbers with at least one match sorted by modification time. Use this tool when you need to find files containing specific patterns. This is especially useful for finding where functions are defined, where variables are used, or searching for specific error messages."
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pat": {
                    "type": "string",
                    "description": "The regular expression pattern to search for in file contents. Supports full regex syntax. Examples: \"async fn\" to find async functions, \"TODO|FIXME\" to find todos, \"struct \\w+\" to find struct definitions"
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
            .ok_or_else(|| anyhow::anyhow!("Missing pat"))?;
        let path = input.get("path").and_then(|v| v.as_str());
        grep_tool(pat, path).await
    }
}
