//! Terminal separator line utilities.

use crate::colors;
use std::env;

/// Generate a separator line.
///
/// # Returns
///
/// A separator string using the terminal width (capped at 80 chars)
pub fn separator() -> String {
    let width = env::var("COLUMNS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(80)
        .min(80);

    format!(
        "{}\0{:\0>width$}{}\n",
        colors::DIM,
        "",
        colors::RESET,
        width = width
    )
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
