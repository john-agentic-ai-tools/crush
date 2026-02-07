use std::fmt;
use std::io;

/// CLI-specific errors
#[derive(Debug)]
pub enum CliError {
    /// Errors from crush-core library
    Core(crush_core::CrushError),
    /// Configuration errors
    Config(String),
    /// I/O errors
    Io(io::Error),
    /// Invalid user input
    InvalidInput(String),
    /// Operation was interrupted (Ctrl+C)
    Interrupted,
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            CliError::Core(e) => write!(f, "{}", user_friendly_message(e)),
            CliError::Config(msg) => write!(f, "Configuration error: {}", msg),
            CliError::Io(e) => write!(f, "I/O error: {}", e),
            CliError::InvalidInput(msg) => write!(f, "Invalid input: {}", msg),
            CliError::Interrupted => write!(f, "Operation cancelled"),
        }
    }
}

impl std::error::Error for CliError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CliError::Core(e) => Some(e),
            CliError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<crush_core::CrushError> for CliError {
    fn from(e: crush_core::CrushError) -> Self {
        // Convert Cancelled core error to Interrupted CLI error
        match e {
            crush_core::CrushError::Cancelled => CliError::Interrupted,
            other => CliError::Core(other),
        }
    }
}

impl From<io::Error> for CliError {
    fn from(e: io::Error) -> Self {
        CliError::Io(e)
    }
}

/// Convert crush-core errors to user-friendly messages
fn user_friendly_message(error: &crush_core::CrushError) -> String {
    match error {
        crush_core::CrushError::Plugin(crush_core::PluginError::NotFound(name)) => {
            format!(
                "Compression plugin '{}' not found. Run 'crush plugins list' to see available plugins.",
                name
            )
        }
        crush_core::CrushError::Validation(crush_core::ValidationError::CrcMismatch {
            expected,
            actual,
        }) => {
            format!(
                "File corrupted: CRC32 checksum mismatch (expected {:08x}, got {:08x}). The compressed file may be damaged.",
                expected, actual
            )
        }
        crush_core::CrushError::Validation(crush_core::ValidationError::InvalidHeader(_)) => {
            "Not a valid Crush archive: invalid file header".to_string()
        }
        crush_core::CrushError::Validation(crush_core::ValidationError::InvalidMagic(_)) => {
            "Not a valid Crush archive: invalid magic number".to_string()
        }
        crush_core::CrushError::Timeout(crush_core::TimeoutError::Timeout(duration)) => {
            format!("Compression timeout after {}s", duration.as_secs())
        }
        crush_core::CrushError::Timeout(crush_core::TimeoutError::PluginPanic) => {
            "Plugin panicked during execution".to_string()
        }
        _ => error.to_string(),
    }
}

/// Exit codes following Unix conventions
impl CliError {
    pub fn exit_code(&self) -> i32 {
        match self {
            CliError::Core(_) => 1,         // Operational error
            CliError::Config(_) => 2,       // Configuration error
            CliError::Io(_) => 1,           // Operational error
            CliError::InvalidInput(_) => 2, // Usage error
            CliError::Interrupted => 130,   // 128 + SIGINT (2)
        }
    }
}

pub type Result<T> = std::result::Result<T, CliError>;

#[cfg(test)]
mod tests {
    use super::*;
    use crush_core::{CrushError, PluginError, TimeoutError, ValidationError};
    use std::error::Error;

    #[test]
    fn test_error_display() {
        let config_err = CliError::Config("test config error".to_string());
        assert!(config_err.to_string().contains("Configuration error"));
        assert!(config_err.to_string().contains("test config error"));

        let io_err = CliError::Io(io::Error::new(io::ErrorKind::NotFound, "file not found"));
        assert!(io_err.to_string().contains("I/O error"));

        let invalid_input = CliError::InvalidInput("bad value".to_string());
        assert!(invalid_input.to_string().contains("Invalid input"));

        let interrupted = CliError::Interrupted;
        assert_eq!(interrupted.to_string(), "Operation cancelled");
    }

    #[test]
    fn test_exit_codes() {
        assert_eq!(
            CliError::Core(CrushError::Plugin(PluginError::NotFound(
                "test".to_string()
            )))
            .exit_code(),
            1
        );
        assert_eq!(CliError::Config("test".to_string()).exit_code(), 2);
        assert_eq!(
            CliError::Io(io::Error::new(io::ErrorKind::NotFound, "test")).exit_code(),
            1
        );
        assert_eq!(CliError::InvalidInput("test".to_string()).exit_code(), 2);
        assert_eq!(CliError::Interrupted.exit_code(), 130);
    }

    #[test]
    fn test_from_crush_error() {
        let core_err: CrushError = PluginError::NotFound("test".to_string()).into();
        let cli_err: CliError = core_err.into();
        assert!(matches!(cli_err, CliError::Core(_)));
    }

    #[test]
    fn test_from_io_error() {
        let io_err = io::Error::new(io::ErrorKind::NotFound, "test");
        let cli_err: CliError = io_err.into();
        assert!(matches!(cli_err, CliError::Io(_)));
    }

    #[test]
    fn test_error_source() {
        let core_err: CrushError = PluginError::NotFound("test".to_string()).into();
        let cli_err = CliError::Core(core_err);
        assert!(cli_err.source().is_some());

        let io_err = CliError::Io(io::Error::new(io::ErrorKind::NotFound, "test"));
        assert!(io_err.source().is_some());

        let config_err = CliError::Config("test".to_string());
        assert!(config_err.source().is_none());

        let interrupted = CliError::Interrupted;
        assert!(interrupted.source().is_none());
    }

    #[test]
    fn test_user_friendly_plugin_not_found() {
        let err = CrushError::Plugin(PluginError::NotFound("my_plugin".to_string()));
        let cli_err = CliError::Core(err);
        let msg = cli_err.to_string();
        assert!(msg.contains("my_plugin"));
        assert!(msg.contains("not found"));
        assert!(msg.contains("plugins list"));
    }

    #[test]
    fn test_user_friendly_crc_mismatch() {
        let err = CrushError::Validation(ValidationError::CrcMismatch {
            expected: 0x12345678,
            actual: 0x87654321,
        });
        let cli_err = CliError::Core(err);
        let msg = cli_err.to_string();
        assert!(msg.contains("corrupted"));
        assert!(msg.contains("CRC32"));
        assert!(msg.contains("12345678"));
        assert!(msg.contains("87654321"));
    }

    #[test]
    fn test_user_friendly_invalid_header() {
        let err = CrushError::Validation(ValidationError::InvalidHeader("bad header".to_string()));
        let cli_err = CliError::Core(err);
        let msg = cli_err.to_string();
        assert!(msg.contains("Not a valid Crush archive"));
        assert!(msg.contains("invalid file header"));
    }

    #[test]
    fn test_user_friendly_invalid_magic() {
        let err = CrushError::Validation(ValidationError::InvalidMagic([0xFF, 0xFF, 0xFF, 0xFF]));
        let cli_err = CliError::Core(err);
        let msg = cli_err.to_string();
        assert!(msg.contains("Not a valid Crush archive"));
        assert!(msg.contains("invalid magic number"));
    }

    #[test]
    fn test_user_friendly_timeout() {
        let err = CrushError::Timeout(TimeoutError::Timeout(std::time::Duration::from_secs(30)));
        let cli_err = CliError::Core(err);
        let msg = cli_err.to_string();
        assert!(msg.contains("timeout"));
        assert!(msg.contains("30s"));
    }

    #[test]
    fn test_user_friendly_plugin_panic() {
        let err = CrushError::Timeout(TimeoutError::PluginPanic);
        let cli_err = CliError::Core(err);
        let msg = cli_err.to_string();
        assert!(msg.contains("Plugin panicked"));
    }

    #[test]
    fn test_user_friendly_other_errors() {
        // Test that other error types fall through to default formatting
        let err = CrushError::Validation(ValidationError::CorruptedData("test".to_string()));
        let cli_err = CliError::Core(err);
        let msg = cli_err.to_string();
        assert!(msg.contains("Corrupted") || msg.contains("test"));
    }
}
