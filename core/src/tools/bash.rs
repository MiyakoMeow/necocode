//! Shell command execution tool.

use anyhow::{Context, Result};
use std::process::Stdio;
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;
use tokio::process::Command;

/// Execute a shell command and return output.
///
/// # Arguments
///
/// * `cmd` - Shell command to execute
///
/// # Returns
///
/// Command output, or "(empty)" if no output
#[cfg(unix)]
pub async fn bash_tool(cmd: &str) -> Result<String> {
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
        println!("  │ {line}");
        output_lines.push(line);
    }

    // Also capture stderr
    let stderr_reader = BufReader::new(stderr);
    let mut stderr_lines = stderr_reader.lines();
    while let Ok(Some(line)) = stderr_lines.next_line().await {
        println!("  │ {line}");
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
#[cfg(windows)]
pub async fn bash_tool(cmd: &str) -> Result<String> {
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
        println!("  │ {line}");
        output_lines.push(line);
    }

    // Also capture stderr
    let stderr_reader = BufReader::new(stderr);
    let mut stderr_lines = stderr_reader.lines();
    while let Ok(Some(line)) = stderr_lines.next_line().await {
        println!("  │ {line}");
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
