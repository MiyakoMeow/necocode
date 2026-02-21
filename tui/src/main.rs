//! nanocode - minimal Claude code alternative in Rust using Actix Actor model.

use actix::prelude::*;
use anyhow::Context as _;
use anyhow::Result;
use clap::Parser;
use crossterm::style::{Attribute, Stylize};
use std::io::{self, BufRead, Write};
use std::path::Path;
use std::process::ExitCode;
use tokio as _;

mod colors;
mod logging;
mod output;
mod separator;

pub use colors::*;
pub use separator::separator;

use neco_core::{App, ClearHistory, Config, ProviderRegistry, ProviderSettings, UiEvent};

/// Initialize the logging system.
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

/// AI programming assistant - Claude Code Rust implementation.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct CliArgs {
    /// Send message directly and execute (non-interactive mode).
    #[arg(short = 'm', long = "message")]
    message: Option<String>,

    /// Model specification (e.g., "anthropic/claude-opus-4-5" or "claude-sonnet-4-5").
    #[arg(short = 'M', long = "model")]
    model: Option<String>,
}

/// UI Actor for handling UI events and rendering.
pub struct UiActor;

impl Default for UiActor {
    fn default() -> Self {
        Self::new()
    }
}

impl UiActor {
    /// Create a new UI actor.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Render a UI event to stdout.
    fn render_event(event: &UiEvent) {
        match event {
            UiEvent::TextDelta(text) => {
                output::print(format_args!("{text}"));
                io::stdout().flush().ok();
            },
            UiEvent::ToolCallStart { id, name } => {
                tracing::debug!(tool = %name, tool_id = %id, "Tool call started");
                output::println(format_args!(
                    "\n🔧 {} (id: {})",
                    name.clone().yellow().bold(),
                    id
                ));
            },
            UiEvent::ToolExecuting { name } => {
                tracing::info!(tool = %name, "Tool executing");
                output::println(format_args!("{}⚙️ {} executing...", Attribute::Bold, name));
            },
            UiEvent::ToolResult { name, result } => {
                tracing::debug!(tool = %name, result_len = result.len(), "Tool result received");
                output::println(format_args!("\n📝 {} Result:", name.clone().green().bold()));
                output::println(format_args!("{result}"));
                output::print(format_args!("{}", separator()));
            },
            UiEvent::Error(error) => {
                if error.contains("Conversation cleared") {
                    output::println(format_args!("{}", "⏺ Cleared conversation".green()));
                } else {
                    tracing::error!(error = %error, "Core error occurred");
                    output::println(format_args!("\n{} Error: {}", "❌".red(), error));
                }
                output::print(format_args!("{}", separator()));
            },
            UiEvent::MessageStart => {
                tracing::debug!("Message started");
                output::print(format_args!("{}", separator()));
            },
            UiEvent::MessageStop => {
                tracing::debug!("Message stopped");
                output::println(format_args!(""));
                output::print(format_args!("{}", separator()));
            },
        }
        io::stdout().flush().ok();
    }
}

impl Actor for UiActor {
    type Context = Context<Self>;
}

impl Handler<UiEvent> for UiActor {
    type Result = ();

    fn handle(&mut self, msg: UiEvent, _ctx: &mut Self::Context) {
        Self::render_event(&msg);
    }
}

fn main() -> ExitCode {
    let args = CliArgs::parse();
    let config = Config::from_env();
    let _logging_enabled = setup_logging(&config);

    if let Err(e) = run(&args, &config) {
        tracing::error!("Application error: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// Run the main application logic.
fn run(args: &CliArgs, config: &Config) -> Result<()> {
    actix::System::new().block_on(async {
        let mut registry = ProviderRegistry::global().write().await;
        registry.register_defaults();
        drop(registry);

        let provider_config = if let Some(model_str) = &args.model {
            ProviderSettings::from_model_string(model_str)?
        } else {
            ProviderSettings::from_env().await?
        };

        output::println(format_args!(
            "{} | {} | {} | {}\n",
            "neco".bold(),
            provider_config.provider_display_name().green().bold(),
            provider_config.model.clone().dim(),
            provider_config.masked_api_key().yellow(),
        ));

        let ui_actor = UiActor::new().start();
        let ui_recipient = ui_actor.recipient();

        let session_addr = App::create_session(provider_config, config);

        if let Some(message) = &args.message {
            App::process_message(&session_addr, message.clone(), ui_recipient)
                .await
                .context("Failed to process message")?;
        } else {
            let stdin = io::stdin();
            for line in stdin.lock().lines() {
                let Ok(input) = line else {
                    break;
                };

                let trimmed = input.trim();
                if trimmed.is_empty() {
                    continue;
                }

                match trimmed {
                    "/q" | "exit" => break,
                    "/c" => {
                        session_addr.send(ClearHistory).await.ok();
                        ui_recipient.do_send(UiEvent::Error("Conversation cleared".to_string()));
                    },
                    msg => {
                        App::process_message(&session_addr, msg.to_string(), ui_recipient.clone())
                            .await
                            .context("Failed to process message")?;
                    },
                }
            }
        }

        Ok(())
    })
}
