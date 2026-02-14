//! Logging initialization and configuration for necocode.

use anyhow::Result;
use std::path::Path;
use tracing_appender::{non_blocking, rolling};
use tracing_subscriber::{Registry, fmt, prelude::*};

/// Initialize tracing subscriber with file and console output.
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
    // 创建日志文件目录
    std::fs::create_dir_all(log_dir)?;

    // 配置文件appender（每天轮转）
    let file_appender = rolling::daily(log_dir, "necocode.log");

    // 创建非阻塞writer
    let (non_blocking_file, _guard) = non_blocking(file_appender);

    // 配置subscriber
    let subscriber = Registry::default()
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
