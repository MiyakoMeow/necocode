//! Nyan-code 核心库
//!
//! 提供纯业务逻辑，包括 API 客户端、配置管理、工具函数和事件类型。
//!
//! 这个库作为整个应用的入口点，重新导出了所有主要的公共类型和模块。

pub mod api;
pub mod command;
pub mod config;
pub mod events;
pub mod input;
pub mod session;
pub mod tools;

// 从 api::anthropic 重新导出常用类型
pub use api::anthropic::{AnthropicConfig, ApiError, Client};

// 从 api::anthropic::models 重新导出常用类型
pub use api::anthropic::models::{ModelInfo, ModelPreference};

// 从 config 重新导出 Config 类型
pub use config::Config;

// 从 events 重新导出 CoreEvent 类型
pub use events::CoreEvent;

// 从 command 重新导出 UserCommand 类型
pub use command::UserCommand;

// 从 input 重新导出输入类型
pub use input::{InputReader, StdinInputReader};

// 从 session 重新导出 Session 类型
pub use session::Session;
