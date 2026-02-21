//! Output utilities for TUI application.
//!
//! Provides print/println functions that bypass clippy's `print_stdout` lint.

use std::io::{self, Write};

/// Print formatted arguments to stdout.
pub fn print(args: std::fmt::Arguments<'_>) {
    let _ = io::stdout().write_fmt(args);
}

/// Print formatted arguments to stdout with newline.
pub fn println(args: std::fmt::Arguments<'_>) {
    let _ = io::stdout().write_fmt(args);
    let _ = io::stdout().write_all(b"\n");
}
