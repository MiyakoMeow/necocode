//! File pattern matching tool using glob patterns.

use anyhow::{Context, Result};

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
            }
            Err(e) => {
                eprintln!("Glob error: {e}");
            }
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
