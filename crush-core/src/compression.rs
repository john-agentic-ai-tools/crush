//! Compression functionality
//!
//! Provides the public `compress()` API that compresses data using the default
//! DEFLATE plugin and wraps it with a Crush header.

use crate::error::Result;
use crate::plugin::registry::{get_default_plugin, get_plugin_by_magic};
use crate::plugin::{CrushHeader, PluginSelector, ScoringWeights};
use crc32fast::Hasher;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// Compression options for plugin selection and scoring
#[derive(Debug, Clone)]
pub struct CompressionOptions {
    /// Optional plugin name for manual override
    plugin_name: Option<String>,

    /// Scoring weights for automatic selection
    weights: ScoringWeights,
}

impl CompressionOptions {
    /// Create new compression options with default settings
    #[must_use]
    pub fn new() -> Self {
        Self {
            plugin_name: None,
            weights: ScoringWeights::default(),
        }
    }

    /// Specify a plugin by name (manual override)
    #[must_use]
    pub fn with_plugin(mut self, name: &str) -> Self {
        self.plugin_name = Some(name.to_string());
        self
    }

    /// Set custom scoring weights
    #[must_use]
    pub fn with_weights(mut self, weights: ScoringWeights) -> Self {
        self.weights = weights;
        self
    }
}

impl Default for CompressionOptions {
    fn default() -> Self {
        Self::new()
    }
}

/// Compress data using the default compression algorithm
///
/// Uses the DEFLATE plugin (magic number `0x43525100`) to compress the input data.
/// The compressed output includes a 16-byte Crush header with:
/// - Magic number identifying the plugin
/// - Original uncompressed size
/// - CRC32 checksum of the compressed payload
///
/// # Errors
///
/// Returns an error if:
/// - No default plugin is available (should never happen - DEFLATE is always registered)
/// - Compression operation fails
/// - Cancellation is triggered (reserved for future timeout support)
///
/// # Examples
///
/// ```
/// use crush_core::{init_plugins, compress};
///
/// init_plugins().expect("Plugin initialization failed");
/// let data = b"Hello, world!";
/// let compressed = compress(data).expect("Compression failed");
/// assert!(!compressed.is_empty());
/// ```
pub fn compress(input: &[u8]) -> Result<Vec<u8>> {
    // Get the default DEFLATE plugin from registry
    let plugin = get_default_plugin().ok_or_else(|| {
        crate::error::PluginError::NotFound(
            "Default DEFLATE plugin not found. Call init_plugins() first.".to_string(),
        )
    })?;

    let default_magic = [0x43, 0x52, 0x01, 0x00];

    // Create cancellation flag (not yet connected to timeout system)
    let cancel_flag = Arc::new(AtomicBool::new(false));

    // Compress the data
    let compressed_payload = plugin.compress(input, cancel_flag)?;

    // Calculate CRC32 of compressed payload
    let mut hasher = Hasher::new();
    hasher.update(&compressed_payload);
    let crc32 = hasher.finalize();

    // Create header with original size and CRC32
    let header = CrushHeader::new(default_magic, input.len() as u64).with_crc32();

    // Build final output: header + compressed payload
    let mut output = Vec::with_capacity(CrushHeader::SIZE + compressed_payload.len());
    output.extend_from_slice(&header.to_bytes());
    output.extend_from_slice(&[
        (crc32 & 0xFF) as u8,
        ((crc32 >> 8) & 0xFF) as u8,
        ((crc32 >> 16) & 0xFF) as u8,
        ((crc32 >> 24) & 0xFF) as u8,
    ]);
    output.extend_from_slice(&compressed_payload);

    Ok(output)
}

/// Compress data with custom options (plugin selection, scoring weights)
///
/// Provides fine-grained control over plugin selection through either:
/// - Manual plugin override by name
/// - Automatic selection with custom scoring weights
///
/// # Errors
///
/// Returns an error if:
/// - Specified plugin is not found (manual override)
/// - No plugins are available (automatic selection)
/// - Compression operation fails
///
/// # Examples
///
/// ```
/// use crush_core::{init_plugins, compress_with_options, CompressionOptions};
///
/// init_plugins().expect("Plugin initialization failed");
/// let data = b"Hello, world!";
///
/// // Use default automatic selection
/// let options = CompressionOptions::default();
/// let compressed = compress_with_options(data, &options).expect("Compression failed");
///
/// // Manual override: specify plugin by name
/// let options = CompressionOptions::default().with_plugin("deflate");
/// let compressed = compress_with_options(data, &options).expect("Compression failed");
/// ```
pub fn compress_with_options(input: &[u8], options: &CompressionOptions) -> Result<Vec<u8>> {
    // Select plugin based on options
    let selector = PluginSelector::new(options.weights);

    let selected_metadata = if let Some(ref plugin_name) = options.plugin_name {
        // Manual override
        selector.select_by_name(plugin_name)?
    } else {
        // Automatic selection
        selector.select()?
    };

    // Get the actual plugin from registry
    let plugin = get_plugin_by_magic(selected_metadata.magic_number).ok_or_else(|| {
        crate::error::PluginError::NotFound(format!(
            "Plugin '{}' metadata found but not in registry",
            selected_metadata.name
        ))
    })?;

    // Create cancellation flag (not yet connected to timeout system)
    let cancel_flag = Arc::new(AtomicBool::new(false));

    // Compress the data
    let compressed_payload = plugin.compress(input, cancel_flag)?;

    // Calculate CRC32 of compressed payload
    let mut hasher = Hasher::new();
    hasher.update(&compressed_payload);
    let crc32 = hasher.finalize();

    // Create header with original size and CRC32
    let header = CrushHeader::new(selected_metadata.magic_number, input.len() as u64).with_crc32();

    // Build final output: header + CRC32 + compressed payload
    let mut output = Vec::with_capacity(CrushHeader::SIZE + 4 + compressed_payload.len());
    output.extend_from_slice(&header.to_bytes());
    output.extend_from_slice(&[
        (crc32 & 0xFF) as u8,
        ((crc32 >> 8) & 0xFF) as u8,
        ((crc32 >> 16) & 0xFF) as u8,
        ((crc32 >> 24) & 0xFF) as u8,
    ]);
    output.extend_from_slice(&compressed_payload);

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::init_plugins;

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_compress_basic() {
        init_plugins().unwrap();
        let data = b"Hello, Crush!";
        let compressed = compress(data).unwrap();

        // Should have at least the header
        assert!(compressed.len() >= CrushHeader::SIZE);

        // Header should have correct magic number
        assert_eq!(&compressed[0..4], &[0x43, 0x52, 0x01, 0x00]);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_compress_empty() {
        init_plugins().unwrap();
        let data = b"";
        let compressed = compress(data).unwrap();

        // Even empty data gets a header
        assert!(compressed.len() >= CrushHeader::SIZE);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_compress_large() {
        init_plugins().unwrap();
        let data = vec![0x42; 1_000_000]; // 1MB of repeated bytes
        let compressed = compress(&data).unwrap();

        // Should compress well (repeated data)
        assert!(compressed.len() < data.len() / 2);
    }
}
