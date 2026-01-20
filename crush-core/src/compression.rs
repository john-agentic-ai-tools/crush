//! Compression functionality
//!
//! Provides the public `compress()` API that compresses data using the default
//! DEFLATE plugin and wraps it with a Crush header.

use crate::error::Result;
use crate::plugin::registry::get_default_plugin;
use crate::plugin::CrushHeader;
use crc32fast::Hasher;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

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
