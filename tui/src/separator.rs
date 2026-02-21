//! Terminal separator line utilities.

use crossterm::{style::Stylize, terminal};

/// Generate a separator line.
///
/// # Returns
///
/// A separator string using the terminal width (capped at 80 chars)
#[must_use]
pub fn separator() -> String {
    let (width, _) = terminal::size().unwrap_or((80, 24));
    let width = width.min(80);

    format!("{}\n", "─".repeat(width as usize).dim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_separator() {
        let sep = separator();
        assert!(sep.contains("\x1b[2m"));
        assert!(sep.contains("\x1b[0m"));
    }
}
