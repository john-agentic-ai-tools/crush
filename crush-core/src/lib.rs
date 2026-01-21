//! Crush Core Library
//!
//! High-performance parallel compression library with pluggable compression algorithms.
//!
//! # Quick Start
//!
//! ```
//! use crush_core::{init_plugins, compress, decompress};
//!
//! init_plugins().expect("Plugin initialization failed");
//! let data = b"Hello, Crush!";
//! let compressed = compress(data).expect("Compression failed");
//! let decompressed = decompress(&compressed).expect("Decompression failed");
//! assert_eq!(data.as_slice(), decompressed.as_slice());
//! ```

pub mod compression;
pub mod decompression;
pub mod error;
pub mod plugin;

pub use compression::{compress, compress_with_options, CompressionOptions};
pub use decompression::decompress;
pub use error::{CrushError, PluginError, Result, TimeoutError, ValidationError};
pub use plugin::{
    calculate_plugin_score, init_plugins, list_plugins, CompressionAlgorithm, CrushHeader,
    PluginMetadata, PluginSelector, ScoringWeights, COMPRESSION_ALGORITHMS,
};

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
