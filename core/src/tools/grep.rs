//! Regex search tool for file contents.

use anyhow::{Context, Result};
use tokio::fs;

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
    let base = path.unwrap_or(".");
    let pattern =
        regex::Regex::new(pat).with_context(|| format!("Invalid regex pattern: {pat}"))?;

    let mut hits = Vec::new();

    for entry in walkdir::WalkDir::new(base)
        .follow_links(true)
        .max_depth(100)
        .into_iter()
        .filter_map(std::result::Result::ok)
    {
        let filepath = entry.path();
        if filepath.is_file()
            && let Ok(content) = fs::read_to_string(filepath).await
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
}
