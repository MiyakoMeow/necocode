//! nanocode - minimal Claude code alternative in Rust

// CLI application - only responsible for rendering and user interaction

use clap::Parser;
use crossterm::style::{Attribute, Stylize};
use std::io::{self, BufRead, Write};
use std::path::Path;
use std::process::ExitCode;
use tokio::sync::mpsc;

use neco::separator;

// Use core library modules
use neco_core::{App, Config, CoreEvent};

mod logging;

/// Initialize the logging system, returns success status
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

/// AI programming assistant - Claude Code Rust implementation
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct CliArgs {
    /// Send message directly and execute (non-interactive mode)
    #[arg(short = 'm', long = "message")]
    message: Option<String>,
}

/// Async task to handle core events (rendering logic)
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
                println!("{}⚙️ {} executing...", Attribute::Bold, name);
            }
            CoreEvent::ToolResult { name, result } => {
                tracing::debug!(tool = %name, result_len = result.len(), "Tool result received");
                println!("\n📝 {} Result:", name.green().bold());
                println!("{}", result);
                print!("{}", separator());
            }
            CoreEvent::Error(error) => {
                if error.contains("Conversation cleared") {
                    println!("{}", "⏺ Cleared conversation".green());
                } else {
                    tracing::error!(error = %error, "Core error occurred");
                    println!("\n{} Error: {}", "❌".red(), error);
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
    let args = CliArgs::parse();
    let config = Config::from_env();
    let _logging_enabled = setup_logging(&config);

    let (input_sender, input_receiver) = mpsc::unbounded_channel();

    let (event_receiver, main_handle, anthropic_config) =
        match App::run(config, input_receiver, args.message.clone()) {
            Ok(result) => result,
            Err(e) => {
                eprintln!("{} Failed to start: {}", "❌".red(), e);
                return ExitCode::FAILURE;
            }
        };

    println!(
        "{} | {} | {} | {}\n",
        "neco".bold(),
        anthropic_config.model.clone().dim(),
        anthropic_config.masked_api_key().yellow(),
        anthropic_config.base_url.clone().dim()
    );

    let event_rt = tokio::runtime::Runtime::new().expect("Failed to create event runtime");
    let render_handle = event_rt.spawn(async move {
        handle_core_events(event_receiver).await;
    });

    if args.message.is_none() {
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            match line {
                Ok(input) => {
                    if input_sender.send(input).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    }

    match event_rt.block_on(async {
        let _ = render_handle.await;
        main_handle.await.unwrap()
    }) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{} Error: {}", "❌".red(), e);
            ExitCode::FAILURE
        }
    }
}
