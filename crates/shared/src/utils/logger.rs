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

pub fn setup_tracing(logger_cfg: LoggerConfig) -> WorkerGuard {
    LogTracer::init().expect("Failed to setup logger");

    let app_name = concat!(env!("CARGO_PKG_NAME"), "-", env!("CARGO_PKG_VERSION")).to_string();

    let (non_blocking_console_writer, guard) = tracing_appender::non_blocking(std::io::stdout());

    let bunyan_console_formatting_layer =
        BunyanFormattingLayer::new(app_name.clone(), non_blocking_console_writer);

    if logger_cfg.log_to_file {
        let non_blocking_file_writer =
            tracing_appender::rolling::daily("logs", format!("{app_name}.log"));
        let bunyan_file_formatting_layer =
            BunyanFormattingLayer::new(app_name.clone(), non_blocking_file_writer);

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
