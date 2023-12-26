/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
#![allow(clippy::expect_used)]

use serde::Deserialize;
use tracing::subscriber::set_global_default;
pub use tracing::{debug, error, info, instrument, warn};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_bunyan_formatter::{BunyanFormattingLayer, JsonStorageLayer};
use tracing_log::LogTracer;
use tracing_subscriber::{filter::LevelFilter, layer::SubscriberExt, Registry};

#[derive(Debug, Deserialize, Clone, Copy)]
pub enum LogLevel {
    TRACE,
    DEBUG,
    INFO,
    WARN,
    ERROR,
    OFF,
}

impl From<LogLevel> for LevelFilter {
    fn from(log_level: LogLevel) -> Self {
        match log_level {
            LogLevel::TRACE => LevelFilter::TRACE,
            LogLevel::DEBUG => LevelFilter::DEBUG,
            LogLevel::INFO => LevelFilter::INFO,
            LogLevel::WARN => LevelFilter::WARN,
            LogLevel::ERROR => LevelFilter::ERROR,
            LogLevel::OFF => LevelFilter::OFF,
        }
    }
}

#[derive(Debug, Deserialize, Clone, Copy)]
pub struct LoggerConfig {
    pub level: LogLevel,
    pub log_to_file: bool,
}

/// Sets up the application-wide logging/tracing using the provided configuration.
///
/// This function initializes the logging system for the application, directing logs to either the console, a file, or both based on the provided configuration.
/// It uses `tracing` for structured logging and `BunyanFormattingLayer` for formatting log messages.
///
/// # Arguments
///
/// * `logger_cfg` - Configuration for the logger, determining the logging level, and whether logs should be saved to a file.
///
/// # Returns
///
/// * `WorkerGuard` - A guard that ensures logs are flushed. Should be kept alive for the duration of the application's runtime to ensure that logs are flushed properly when the application shuts down.
///
/// # Panics
///
/// * If there's a failure initializing the logger or setting the global default subscriber.
///
/// # Example
///
/// ```rust
/// let logger_config = LoggerConfig {
///     level: Level::INFO,
///     log_to_file: true,
/// };
/// let _guard = setup_tracing(logger_config);
/// ```
///
/// Note: Ensure you keep `_guard` in scope to guarantee log flushing. If `_guard` is dropped, logs might not be flushed correctly on program termination.
pub fn setup_tracing(logger_cfg: LoggerConfig) -> WorkerGuard {
    LogTracer::init().expect("Failed to setup logger");

    let app_name = concat!(env!("CARGO_PKG_NAME"), "-", env!("CARGO_PKG_VERSION")).to_string();

    let (non_blocking_console_writer, guard) = tracing_appender::non_blocking(std::io::stdout());

    let bunyan_console_formatting_layer =
        BunyanFormattingLayer::new(app_name.to_owned(), non_blocking_console_writer);

    if logger_cfg.log_to_file {
        let non_blocking_file_writer =
            tracing_appender::rolling::daily("logs", format!("{app_name}.log"));
        let bunyan_file_formatting_layer =
            BunyanFormattingLayer::new(app_name.to_owned(), non_blocking_file_writer);

        let subscriber = Registry::default()
            .with(LevelFilter::from(logger_cfg.level))
            .with(JsonStorageLayer)
            .with(bunyan_file_formatting_layer)
            .with(bunyan_console_formatting_layer);

        set_global_default(subscriber).expect("Unable to set global tracing subscriber");
    } else {
        let subscriber = Registry::default()
            .with(LevelFilter::from(logger_cfg.level))
            .with(JsonStorageLayer)
            .with(bunyan_console_formatting_layer);

        set_global_default(subscriber).expect("Unable to set global tracing subscriber");
    }

    guard
}
