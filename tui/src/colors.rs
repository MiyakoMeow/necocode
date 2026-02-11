//! ANSI color codes for terminal UI.

/// 重置所有格式
pub const RESET: &str = "\x1b[0m";
/// 粗体文本
pub const BOLD: &str = "\x1b[1m";
/// 暗淡文本
pub const DIM: &str = "\x1b[2m";
/// 蓝色文本
pub const BLUE: &str = "\x1b[34m";
/// 绿色文本
pub const GREEN: &str = "\x1b[32m";
/// 黄色文本
pub const YELLOW: &str = "\x1b[33m";
/// 红色文本
pub const RED: &str = "\x1b[31m";
