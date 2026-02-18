//! Input abstractions for the core library.
//!
//! This module provides traits for input operations to allow
//! the core logic to be independent of specific input implementations.

mod stdin;

pub use stdin::StdinInputReader;

/// Trait for reading user input during interactive sessions.
///
/// This abstraction allows the core library to work with different
/// input sources (stdin, test mocks, etc.) without coupling to
/// specific input implementations.
#[async_trait::async_trait]
pub trait InputReader {
    /// Read a single line of input from the user.
    ///
    /// Returns `None` when EOF is reached or the input stream is closed.
    ///
    /// # Examples
    ///
    /// ```
    /// use neco_core::InputReader;
    ///
    /// # async fn test(mut reader: impl InputReader) {
    /// if let Some(line) = reader.read_line().await {
    ///     println!("User entered: {}", line);
    /// } else {
    ///     println!("User closed input");
    /// }
    /// # }
    /// ```
    async fn read_line(&mut self) -> Option<String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mock input reader for testing
    struct MockInputReader {
        lines: Vec<Option<String>>,
    }

    impl MockInputReader {
        fn new(lines: Vec<Option<String>>) -> Self {
            Self { lines }
        }
    }

    #[async_trait::async_trait]
    impl InputReader for MockInputReader {
        async fn read_line(&mut self) -> Option<String> {
            if self.lines.is_empty() {
                None
            } else {
                self.lines.remove(0)
            }
        }
    }

    #[tokio::test]
    async fn test_input_reader_read() {
        let mut reader = MockInputReader::new(vec![
            Some("hello".to_string()),
            Some("world".to_string()),
            None,
        ]);

        assert_eq!(reader.read_line().await, Some("hello".to_string()));
        assert_eq!(reader.read_line().await, Some("world".to_string()));
        assert_eq!(reader.read_line().await, None);
    }
}
