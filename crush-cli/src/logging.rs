use tracing::Level;
use tracing_subscriber::fmt;
use tracing_subscriber::EnvFilter;
use std::path::Path;
use std::fs::File;

/// Initialize logging with the given level and format
pub fn init_logging(level: &str, format: &str, log_file: Option<&Path>) {
    // Parse log level
    let level = match level {
        "error" => Level::ERROR,
        "warn" => Level::WARN,
        "info" => Level::INFO,
        "debug" => Level::DEBUG,
        "trace" => Level::TRACE,
        _ => Level::INFO,
    };

    // Create env filter
    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(level.as_str()))
        .unwrap_or_else(|_| EnvFilter::new("info"));

    // Set up subscriber based on format and output
    match (format, log_file) {
        ("json", Some(path)) => {
            let file = File::create(path).expect("Failed to create log file");
            fmt()
                .json()
                .with_env_filter(env_filter)
                .with_writer(move || file.try_clone().expect("Failed to clone file"))
                .init();
        }
        ("json", None) => {
            fmt()
                .json()
                .with_env_filter(env_filter)
                .with_writer(std::io::stderr)
                .init();
        }
        (_, Some(path)) => {
            let file = File::create(path).expect("Failed to create log file");
            fmt()
                .with_env_filter(env_filter)
                .with_writer(move || file.try_clone().expect("Failed to clone file"))
                .init();
        }
        _ => {
            fmt()
                .with_env_filter(env_filter)
                .with_writer(std::io::stderr)
                .init();
        }
    }
}
