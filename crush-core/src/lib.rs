//! Crush Core Library
//!
//! High-performance parallel compression library with pluggable compression algorithms.

pub mod error;
pub mod plugin;

pub use error::{CrushError, PluginError, Result, TimeoutError, ValidationError};
pub use plugin::{CompressionAlgorithm, CrushHeader, PluginMetadata, COMPRESSION_ALGORITHMS};

/// Placeholder function demonstrating public API structure.
///
/// This function will be replaced with actual compression functionality
/// in future features. It exists to validate:
/// - Documentation builds correctly
/// - Public APIs are exported
/// - Tests can call public functions
///
/// # Examples
///
/// ```
/// use crush_core::hello;
/// assert_eq!(hello(), "Hello from crush-core!");
/// ```
#[must_use]
pub fn hello() -> &'static str {
    "Hello from crush-core!"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hello() {
        assert_eq!(hello(), "Hello from crush-core!");
    }
}
