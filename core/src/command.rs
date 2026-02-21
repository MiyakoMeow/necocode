//! User command types for interactive mode.
//!
//! Defines commands that can be issued by the user during the
//! interactive REPL loop.

use std::fmt;

/// User command that can be executed during the interactive session.
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::module_name_repetitions)]
pub enum UserCommand {
    /// Quit the interactive session
    Quit,

    /// Clear the conversation history
    Clear,

    /// Regular message to send to the AI
    Message(String),
}

impl fmt::Display for UserCommand {
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
    fn test_user_command_display() {
        assert_eq!(UserCommand::Quit.to_string(), "quit");
        assert_eq!(UserCommand::Clear.to_string(), "clear");
        assert_eq!(
            UserCommand::Message("test".to_string()).to_string(),
            "message: test"
        );
    }

    #[test]
    fn test_user_command_equality() {
        assert_eq!(UserCommand::Quit, UserCommand::Quit);
        assert_eq!(UserCommand::Clear, UserCommand::Clear);
        assert_eq!(
            UserCommand::Message("test".to_string()),
            UserCommand::Message("test".to_string())
        );
        assert_ne!(UserCommand::Quit, UserCommand::Clear);
    }
}
