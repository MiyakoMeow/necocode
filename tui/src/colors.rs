//! Terminal styling utilities using crossterm.

use crossterm::style::{Attribute, Color};

/// 重置所有格式
pub const RESET: Attribute = Attribute::Reset;
/// 粗体文本
pub const BOLD: Attribute = Attribute::Bold;
/// 暗淡文本
pub const DIM: Attribute = Attribute::Dim;
/// 蓝色文本
pub const BLUE: Color = Color::Blue;
/// 绿色文本
pub const GREEN: Color = Color::Green;
/// 黄色文本
pub const YELLOW: Color = Color::Yellow;
/// 红色文本
pub const RED: Color = Color::Red;
