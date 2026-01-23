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
            CliError::Interrupted => write!(f, "Operation interrupted"),
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
        CliError::Core(e)
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
            CliError::Core(_) => 1,           // Operational error
            CliError::Config(_) => 2,         // Configuration error
            CliError::Io(_) => 1,             // Operational error
            CliError::InvalidInput(_) => 2,   // Usage error
            CliError::Interrupted => 130,     // 128 + SIGINT (2)
        }
    }
}

pub type Result<T> = std::result::Result<T, CliError>;
