//! Terminal styling utilities using crossterm.

use crossterm::style::{Attribute, Color};

/// Reset all formatting
pub const RESET: Attribute = Attribute::Reset;
/// Bold text
pub const BOLD: Attribute = Attribute::Bold;
/// Dim text
pub const DIM: Attribute = Attribute::Dim;
/// Blue text
pub const BLUE: Color = Color::Blue;
/// Green text
pub const GREEN: Color = Color::Green;
/// Yellow text
pub const YELLOW: Color = Color::Yellow;
/// Red text
pub const RED: Color = Color::Red;
