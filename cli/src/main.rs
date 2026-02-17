//! nanocode - minimal Claude code alternative in Rust

// CLI 应用程序 - 只负责渲染和用户交互

use clap::Parser;
use crossterm::style::{Attribute, Stylize};
use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;
use tokio::sync::mpsc;

use necocode::separator;

// 使用 core 库模块
use necocode_core::{AnthropicConfig, Config, CoreEvent, Session, StdinInputReader};

mod logging;

/// 初始化日志系统，返回是否成功
fn setup_logging(config: &Config) -> bool {
    let log_dir = Path::new(&config.cwd).join("logs");
    match logging::init_logging(&log_dir) {
        Ok(()) => true,
        Err(e) => {
            eprintln!("Failed to initialize logging: {}", e);
            false
        }
    }
}

/// AI编程助手 - Claude Code Rust实现
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct CliArgs {
    /// 直接发送消息并执行（非交互模式）
    #[arg(short = 'm', long = "message")]
    message: Option<String>,
}

/// 处理核心事件的异步任务（渲染逻辑）
async fn handle_core_events(mut receiver: mpsc::UnboundedReceiver<CoreEvent>) {
    while let Some(event) = receiver.recv().await {
        match event {
            CoreEvent::TextDelta(text) => {
                print!("{}", text);
                io::stdout().flush().unwrap();
            }
            CoreEvent::ToolCallStart { id, name } => {
                tracing::debug!(tool = %name, tool_id = %id, "Tool call started");
                println!("\n🔧 {} (id: {})", name.yellow().bold(), id);
            }
            CoreEvent::ToolExecuting { name } => {
                tracing::info!(tool = %name, "Tool executing");
                println!("{}⚙️ {}执行中...", Attribute::Bold, name);
            }
            CoreEvent::ToolResult { name, result } => {
                tracing::debug!(tool = %name, result_len = result.len(), "Tool result received");
                println!("\n📝 {} 结果:", name.green().bold());
                println!("{}", result);
                print!("{}", separator());
            }
            CoreEvent::Error(error) => {
                // 特殊处理 "Conversation cleared" 消息
                if error.contains("Conversation cleared") {
                    println!("{}", "⏺ Cleared conversation".green());
                } else {
                    tracing::error!(error = %error, "Core error occurred");
                    println!("\n{} 错误: {}", "❌".red(), error);
                }
                print!("{}", separator());
            }
            CoreEvent::MessageStart => {
                tracing::debug!("Message started");
                print!("{}", separator());
            }
            CoreEvent::MessageStop => {
                tracing::debug!("Message stopped");
                println!();
                print!("{}", separator());
            }
        }
        io::stdout().flush().unwrap();
    }
}

fn main() -> ExitCode {
    // 解析命令行参数
    let args = CliArgs::parse();

    // 加载基础配置
    let config = Config::from_env();

    // 初始化日志系统
    let _logging_enabled = setup_logging(&config);

    // 创建运行时
    //
    // SAFETY: Runtime creation failure is unrecoverable and should terminate the program.
    // We use expect() here because:
    // 1. This is in main() - there's no caller to handle the error
    // 2. Runtime creation failing is a fatal system error
    // 3. The error message is clear and actionable
    #[allow(clippy::expect_used)]
    let rt = tokio::runtime::Runtime::new().expect("Failed to create runtime");

    // 在运行时中加载 Anthropic 配置（支持异步模型验证和自动选择）
    let anthropic_config = rt.block_on(async { AnthropicConfig::from_env_with_validation().await });

    // 显示启动信息
    println!(
        "{} | {} | {} | {} | {}\n",
        "necocode".bold(),
        anthropic_config.model.clone().dim(),
        anthropic_config.masked_api_key().yellow(),
        anthropic_config.base_url.clone().dim(),
        config.cwd.clone().dim()
    );

    // 创建事件通道
    let (event_sender, event_receiver) = mpsc::unbounded_channel();

    // 创建 session
    let mut session = Session::new(anthropic_config.clone(), config.cwd.clone());

    // 启动事件处理任务（渲染）
    let handle = rt.spawn(async move {
        handle_core_events(event_receiver).await;
    });

    // 根据参数选择运行模式
    let result = rt.block_on(async {
        if let Some(message) = args.message {
            // 非交互模式：执行单次对话
            session.run_single(message, event_sender).await
        } else {
            // 交互模式：进入REPL
            let reader = StdinInputReader;
            session.run_interactive(reader, event_sender).await
        }
    });

    // 等待事件处理任务完成
    rt.block_on(async {
        handle.await.unwrap();
    });

    // 处理结果并返回退出码
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{} Error: {}", "❌".red(), e);
            ExitCode::FAILURE
        }
    }
}
