//! Shell command execution tool.

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::Value;
use std::process::Stdio;
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;
use tokio::process::Command;
use tracing;

use crate::tools::Tool;

/// Execute a shell command and return output.
///
/// # Arguments
///
/// * `cmd` - Shell command to execute
///
/// # Returns
///
/// Command output, or "(empty)" if no output
///
/// # Errors
///
/// Returns error if:
/// - Command fails to spawn
/// - Failed to capture stdout/stderr
/// - Command wait fails
#[cfg(unix)]
pub async fn bash(cmd: &str) -> Result<String> {
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("Failed to execute command: {cmd}"))?;

    let stdout = child.stdout.take().context("Failed to capture stdout")?;
    let stderr = child.stderr.take().context("Failed to capture stderr")?;

    let stdout_reader = BufReader::new(stdout);
    let mut output_lines = Vec::new();

    let mut lines = stdout_reader.lines();
    while let Ok(Some(line)) = lines.next_line().await {
        tracing::info!("  │ {line}");
        output_lines.push(line);
    }

    let stderr_reader = BufReader::new(stderr);
    let mut stderr_lines = stderr_reader.lines();
    while let Ok(Some(line)) = stderr_lines.next_line().await {
        tracing::info!("  │ {line}");
        output_lines.push(line);
    }

    let status = child
        .wait()
        .await
        .with_context(|| format!("Failed to wait for command: {cmd}"))?;

    if !status.success() {
        let code = status.code().unwrap_or(-1);
        output_lines.push(format!("(exit code: {code})"));
    }

    let result = output_lines.join("\n");
    if result.is_empty() {
        Ok("(empty)".to_string())
    } else {
        Ok(result.trim().to_string())
    }
}

/// Execute a shell command and return output (Windows version).
///
/// # Arguments
///
/// * `cmd` - Shell command to execute
///
/// # Returns
///
/// Command output, or "(empty)" if no output
///
/// # Errors
///
/// Returns error if:
/// - Command fails to spawn
/// - Failed to capture stdout/stderr
/// - Command wait fails
#[cfg(windows)]
pub async fn bash(cmd: &str) -> Result<String> {
    let mut child = Command::new("cmd")
        .args(["/C", cmd])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("Failed to execute command: {cmd}"))?;

    let stdout = child.stdout.take().context("Failed to capture stdout")?;
    let stderr = child.stderr.take().context("Failed to capture stderr")?;

    let stdout_reader = BufReader::new(stdout);
    let mut output_lines = Vec::new();

    let mut lines = stdout_reader.lines();
    while let Ok(Some(line)) = lines.next_line().await {
        tracing::info!("  │ {line}");
        output_lines.push(line);
    }

    let stderr_reader = BufReader::new(stderr);
    let mut stderr_lines = stderr_reader.lines();
    while let Ok(Some(line)) = stderr_lines.next_line().await {
        tracing::info!("  │ {line}");
        output_lines.push(line);
    }

    let status = child
        .wait()
        .await
        .with_context(|| format!("Failed to wait for command: {cmd}"))?;

    if !status.success() {
        let code = status.code().unwrap_or(-1);
        output_lines.push(format!("(exit code: {code})"));
    }

    let result = output_lines.join("\n");
    if result.is_empty() {
        Ok("(empty)".to_string())
    } else {
        Ok(result.trim().to_string())
    }
}

/// Bash tool wrapper.
pub struct Bash;

#[async_trait]
impl Tool for Bash {
    fn name(&self) -> &'static str {
        "bash"
    }

    fn description(&self) -> &'static str {
        "Execute a Bash command in a persistent shell session and return the output. This tool can run any shell command, including git, npm, docker, etc. Commands run in /home/miyakomeow/Codes/necocode by default. Use the workdir parameter if you need to run a command in a different directory. IMPORTANT: This tool is for terminal operations like git, npm, docker, etc. DO NOT use it for file operations (reading, writing, editing, searching, finding files) - use the specialized tools for those commands. Before executing the command, please follow these steps: 1. Directory Verification: If the command will create new directories or files, first use ls to verify the parent directory exists and is the correct location. For example, before running \"mkdir foo/bar\", first use ls foo to check that \"foo\" exists and is the intended parent location. 2. Command Execution: Always quote file paths that contain spaces with double quotes (e.g., rm \"path with spaces/file.txt\"). Examples of proper quoting: mkdir \"/Users/name/My Documents\" (correct), mkdir /Users/name/My Documents (incorrect - will fail), python \"/path/with spaces/script.py\" (correct), python /path/with spaces/script.py (incorrect - will fail). After ensuring proper quoting, execute the command. 3. Capture the output of the command."
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "cmd": {
                    "type": "string",
                    "description": "The bash command to execute"
                }
            },
            "required": ["cmd"]
        })
    }

    async fn execute(&self, input: &Value) -> Result<String> {
        let cmd = input
            .get("cmd")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing cmd"))?;
        bash(cmd).await
    }
}
