//! Standard input implementation for input reader.

use crate::input::InputReader;
use std::io;
use tokio::task;

/// Standard input reader that reads from the console.
///
/// This implementation provides a blocking stdin reader wrapped in an async interface.
pub struct StdinInputReader;

impl StdinInputReader {
    /// Create a new stdin input reader.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for StdinInputReader {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl InputReader for StdinInputReader {
    async fn read_line(&mut self) -> Option<String> {
        task::spawn_blocking(|| {
            let mut input = String::new();
            let bytes_read = io::stdin().read_line(&mut input).ok()?;
            if bytes_read == 0 { None } else { Some(input) }
        })
        .await
        .ok()
        .flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stdin_input_reader_default() {
        let _ = StdinInputReader;
    }

    #[test]
    fn test_stdin_input_reader_new() {
        let _ = StdinInputReader::new();
    }
}
