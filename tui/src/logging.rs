//! Logging initialization and configuration for neco.

use anyhow::Result;
use std::path::Path;
use tracing_appender::{non_blocking, rolling};
use tracing_subscriber::{EnvFilter, Registry, fmt, prelude::*};

/// Initialize tracing subscriber with file and console output.
///
/// # Log Levels
///
/// By default, the console outputs only WARN and ERROR level logs.
/// The file appender captures all log levels for debugging purposes.
///
/// To customize the log level, set the `RUST_LOG` environment variable:
///
/// ```sh
/// # Show INFO and above on console
/// RUST_LOG=info neco -m "hello"
///
/// # Show DEBUG and above on console
/// RUST_LOG=debug neco -m "hello"
///
/// # Show TRACE for a specific module
/// RUST_LOG=neco::api=trace neco -m "hello"
/// ```
///
/// # Arguments
///
/// * `log_dir` - Directory where log files will be written
///
/// # Returns
///
/// `Ok(())` if logging was initialized successfully
///
/// # Errors
///
/// Returns an error if:
/// - Log directory cannot be created
/// - Subscriber cannot be set as global default
pub fn init_logging(log_dir: &Path) -> Result<()> {
    // Create log file directory
    std::fs::create_dir_all(log_dir)?;

    // Configure file appender (daily rotation)
    let file_appender = rolling::daily(log_dir, "neco.log");

    // Create non-blocking writer
    let (non_blocking_file, _guard) = non_blocking(file_appender);

    // Configure environment filter (default: WARN for console, INFO for file)
    let env_filter = EnvFilter::builder()
        .with_default_directive(tracing::Level::WARN.into())
        .from_env_lossy();

    // Configure subscriber
    let subscriber = Registry::default()
        .with(env_filter)
        .with(
            fmt::layer()
                .with_writer(std::io::stdout)
                .event_format(fmt::format().compact()),
        )
        .with(
            fmt::layer()
                .with_writer(non_blocking_file)
                .event_format(fmt::format().with_ansi(false).with_target(false)),
        );

    tracing::subscriber::set_global_default(subscriber)?;
    Ok(())
}
