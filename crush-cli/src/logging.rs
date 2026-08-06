use std::fs::File;
use std::path::Path;
use tracing::Level;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt;

/// Map verbose count to log level
/// - 0 (no -v flags) = INFO level
/// - 1 (-v) = DEBUG level
/// - 2+ (-vv) = TRACE level
pub fn verbose_to_level(verbose: u8) -> &'static str {
    match verbose {
        0 => "info",
        1 => "debug",
        _ => "trace", // 2 or more
    }
}

/// Map a configured level name to a [`Level`], falling back to `INFO`.
///
/// Split out of [`init_logging`] so it can be tested: `init_logging` installs a
/// process-global subscriber and can only run once, which makes it unusable
/// from a multi-test binary.
pub(crate) fn parse_level(level: &str) -> Level {
    match level {
        "error" => Level::ERROR,
        "warn" => Level::WARN,
        "debug" => Level::DEBUG,
        "trace" => Level::TRACE,
        // "info" and anything unrecognised
        _ => Level::INFO,
    }
}

/// Initialize logging with the given level and format
pub fn init_logging(level: &str, format: &str, log_file: Option<&Path>) {
    let level = parse_level(level);

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verbose_count_maps_to_level_name() {
        assert_eq!(verbose_to_level(0), "info");
        assert_eq!(verbose_to_level(1), "debug");
        assert_eq!(verbose_to_level(2), "trace");
        // Anything beyond -vv saturates at trace rather than wrapping.
        assert_eq!(verbose_to_level(3), "trace");
        assert_eq!(verbose_to_level(u8::MAX), "trace");
    }

    #[test]
    fn verbose_to_level_output_is_accepted_by_parse_level() {
        // The two halves must agree: every name `verbose_to_level` can emit has
        // to round-trip through `parse_level` to the level it names.
        assert_eq!(parse_level(verbose_to_level(0)), Level::INFO);
        assert_eq!(parse_level(verbose_to_level(1)), Level::DEBUG);
        assert_eq!(parse_level(verbose_to_level(2)), Level::TRACE);
    }

    #[test]
    fn parse_level_maps_every_configured_name() {
        assert_eq!(parse_level("error"), Level::ERROR);
        assert_eq!(parse_level("warn"), Level::WARN);
        assert_eq!(parse_level("info"), Level::INFO);
        assert_eq!(parse_level("debug"), Level::DEBUG);
        assert_eq!(parse_level("trace"), Level::TRACE);
    }

    #[test]
    fn parse_level_falls_back_to_info() {
        // Config validation should reject these first; this is the safety net.
        assert_eq!(parse_level(""), Level::INFO);
        assert_eq!(parse_level("verbose"), Level::INFO);
        // Matching is exact, so case variants fall through to the default.
        assert_eq!(parse_level("ERROR"), Level::INFO);
    }
}
