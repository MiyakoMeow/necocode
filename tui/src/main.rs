//! nanocode - minimal Claude code alternative in Rust

use anyhow::Context;
use clap::Parser;
use crossterm::style::{Attribute, Stylize};
use std::io::{self, BufRead, Write};
use std::path::Path;
use std::process::ExitCode;
use tokio::sync::mpsc;

mod colors;
mod logging;
mod output;
mod separator;

pub use colors::*;
pub use separator::separator;

use neco_core::{App, Config, CoreEvent, ProviderRegistry};

/// Initialize the logging system, returns success status.
fn setup_logging(config: &Config) -> bool {
    let log_dir = Path::new(&config.cwd).join("logs");
    match logging::init_logging(&log_dir) {
        Ok(()) => true,
        Err(e) => {
            tracing::error!("Failed to initialize logging: {e}");
            false
        },
    }
}

/// AI programming assistant - Claude Code Rust implementation
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct CliArgs {
    /// Send message directly and execute (non-interactive mode)
    #[arg(short = 'm', long = "message")]
    message: Option<String>,

    /// Model specification (e.g., "anthropic/claude-opus-4-5" or "claude-sonnet-4-5")
    #[arg(short = 'M', long = "model")]
    model: Option<String>,
}

/// Async task to handle core events (rendering logic).
async fn handle_core_events(
    mut receiver: mpsc::UnboundedReceiver<CoreEvent>,
) -> anyhow::Result<()> {
    while let Some(event) = receiver.recv().await {
        match event {
            CoreEvent::TextDelta(text) => {
                output::print(format_args!("{text}"));
                io::stdout().flush().context("Failed to flush stdout")?;
            },
            CoreEvent::ToolCallStart { id, name } => {
                tracing::debug!(tool = %name, tool_id = %id, "Tool call started");
                output::println(format_args!("\n🔧 {} (id: {})", name.yellow().bold(), id));
            },
            CoreEvent::ToolExecuting { name } => {
                tracing::info!(tool = %name, "Tool executing");
                output::println(format_args!("{}⚙️ {} executing...", Attribute::Bold, name));
            },
            CoreEvent::ToolResult { name, result } => {
                tracing::debug!(tool = %name, result_len = result.len(), "Tool result received");
                output::println(format_args!("\n📝 {} Result:", name.green().bold()));
                output::println(format_args!("{result}"));
                output::print(format_args!("{}", separator()));
            },
            CoreEvent::Error(error) => {
                if error.contains("Conversation cleared") {
                    output::println(format_args!("{}", "⏺ Cleared conversation".green()));
                } else {
                    tracing::error!(error = %error, "Core error occurred");
                    output::println(format_args!("\n{} Error: {}", "❌".red(), error));
                }
                output::print(format_args!("{}", separator()));
            },
            CoreEvent::MessageStart => {
                tracing::debug!("Message started");
                output::print(format_args!("{}", separator()));
            },
            CoreEvent::MessageStop => {
                tracing::debug!("Message stopped");
                output::println(format_args!(""));
                output::print(format_args!("{}", separator()));
            },
        }
        io::stdout().flush().context("Failed to flush stdout")?;
    }
    Ok(())
}

fn main() -> ExitCode {
    let args = CliArgs::parse();
    let config = Config::from_env();
    let _logging_enabled = setup_logging(&config);

    if let Err(e) = run(&args, config) {
        tracing::error!("Application error: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// Run the main application logic.
///
/// This function initializes the provider registry, starts the application,
/// and handles the event loop for both interactive and single-message modes.
///
/// # Arguments
///
/// * `args` - Parsed command line arguments
/// * `config` - Application configuration loaded from environment
///
/// # Returns
///
/// Returns `Ok(ExitCode::SUCCESS)` on successful execution,
/// or an error if initialization or execution fails.
fn run(args: &CliArgs, config: Config) -> anyhow::Result<ExitCode> {
    let rt = tokio::runtime::Runtime::new().context("Failed to create Tokio runtime")?;
    rt.block_on(async {
        let mut registry = ProviderRegistry::global().write().await;
        registry.register_defaults();
    });

    let (input_sender, input_receiver) = mpsc::unbounded_channel();

    let (event_receiver, main_handle, provider_config) = App::run(
        config,
        input_receiver,
        args.message.clone(),
        args.model.clone(),
        &rt,
    )
    .context("Failed to start application")?;

    output::println(format_args!(
        "{} | {} | {} | {}\n",
        "neco".bold(),
        provider_config.provider_display_name().green().bold(),
        provider_config.model.clone().dim(),
        provider_config.masked_api_key().yellow(),
    ));

    let render_handle = rt.spawn(async move {
        if let Err(e) = handle_core_events(event_receiver).await {
            tracing::error!("Render error: {e}");
        }
    });

    if args.message.is_none() {
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            match line {
                Ok(input) => {
                    if input_sender.send(input).is_err() {
                        break;
                    }
                },
                Err(_) => break,
            }
        }
    }

    let result = rt.block_on(async {
        let _ = render_handle.await;
        match main_handle.await {
            Ok(result) => result,
            Err(e) => Err(e).context("Main task failed"),
        }
    });

    result.context("Application execution failed")?;
    Ok(ExitCode::SUCCESS)
}
