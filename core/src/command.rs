//! User command types for interactive mode.
//!
//! Defines commands that can be issued by the user during the
//! interactive REPL loop.

use std::fmt;

/// User command that can be executed during the interactive session.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Quit the interactive session
    Quit,

    /// Clear the conversation history
    Clear,

    /// Regular message to send to the AI
    Message(String),
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Quit => write!(f, "quit"),
            Self::Clear => write!(f, "clear"),
            Self::Message(msg) => write!(f, "message: {msg}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_display() {
        assert_eq!(Command::Quit.to_string(), "quit");
        assert_eq!(Command::Clear.to_string(), "clear");
        assert_eq!(
            Command::Message("test".to_string()).to_string(),
            "message: test"
        );
    }

    #[test]
    fn test_command_equality() {
        assert_eq!(Command::Quit, Command::Quit);
        assert_eq!(Command::Clear, Command::Clear);
        assert_eq!(
            Command::Message("test".to_string()),
            Command::Message("test".to_string())
        );
        assert_ne!(Command::Quit, Command::Clear);
    }
}
