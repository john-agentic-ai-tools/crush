//! Default DEFLATE compression plugin
//!
//! Provides standard DEFLATE compression using the flate2 crate.
//! This plugin is always available and serves as the default compression algorithm.

use crate::error::{PluginError, Result};
use crate::plugin::{CompressionAlgorithm, PluginMetadata, COMPRESSION_ALGORITHMS};
use flate2::read::{DeflateDecoder, DeflateEncoder};
use flate2::Compression;
use linkme::distributed_slice;
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// DEFLATE compression plugin (RFC 1951)
///
/// Uses flate2's DEFLATE implementation with default compression level (6).
/// This is the standard compression algorithm used by gzip, zlib, and PNG.
pub struct DeflatePlugin;

impl CompressionAlgorithm for DeflatePlugin {
    fn name(&self) -> &'static str {
        "deflate"
    }

    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: "deflate",
            version: "1.0.0",
            // Magic number: CR (Crush) + V1 + ID 0x00 (default DEFLATE)
            magic_number: [0x43, 0x52, 0x01, 0x00],
            // Measured throughput: ~200 MB/s compression (typical on modern CPU)
            throughput: 200.0,
            // Compression ratio: ~0.35 (65% size reduction on text)
            compression_ratio: 0.35,
            description: "Standard DEFLATE compression (RFC 1951)",
        }
    }

    fn compress(&self, input: &[u8], cancel_flag: Arc<AtomicBool>) -> Result<Vec<u8>> {
        // Check cancellation before starting
        if cancel_flag.load(Ordering::Acquire) {
            return Err(PluginError::Cancelled.into());
        }

        let mut encoder = DeflateEncoder::new(input, Compression::default());
        let mut compressed = Vec::new();

        // Read compressed data in chunks, checking cancellation periodically
        let mut buffer = vec![0u8; 64 * 1024]; // 64KB chunks
        loop {
            if cancel_flag.load(Ordering::Acquire) {
                return Err(PluginError::Cancelled.into());
            }

            match encoder.read(&mut buffer) {
                Ok(0) => break, // EOF
                Ok(n) => compressed.extend_from_slice(&buffer[..n]),
                Err(e) => {
                    return Err(PluginError::OperationFailed(format!(
                        "DEFLATE compression failed: {e}"
                    ))
                    .into())
                }
            }
        }

        Ok(compressed)
    }

    fn decompress(&self, input: &[u8], cancel_flag: Arc<AtomicBool>) -> Result<Vec<u8>> {
        // Check cancellation before starting
        if cancel_flag.load(Ordering::Acquire) {
            return Err(PluginError::Cancelled.into());
        }

        let mut decoder = DeflateDecoder::new(input);
        let mut decompressed = Vec::new();

        // Read decompressed data in chunks, checking cancellation periodically
        let mut buffer = vec![0u8; 64 * 1024]; // 64KB chunks
        loop {
            if cancel_flag.load(Ordering::Acquire) {
                return Err(PluginError::Cancelled.into());
            }

            match decoder.read(&mut buffer) {
                Ok(0) => break, // EOF
                Ok(n) => decompressed.extend_from_slice(&buffer[..n]),
                Err(e) => {
                    return Err(PluginError::OperationFailed(format!(
                        "DEFLATE decompression failed: {e}"
                    ))
                    .into())
                }
            }
        }

        Ok(decompressed)
    }

    fn detect(&self, _file_header: &[u8]) -> bool {
        // DEFLATE plugin accepts all data (it's the default fallback)
        // File type detection is primarily for routing during compression.
        // For decompression, we use the magic number in the Crush header.
        true
    }
}

/// Register DEFLATE plugin at compile-time
#[distributed_slice(COMPRESSION_ALGORITHMS)]
static DEFLATE_PLUGIN: &dyn CompressionAlgorithm = &DeflatePlugin;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deflate_metadata() {
        let plugin = DeflatePlugin;
        let metadata = plugin.metadata();

        assert_eq!(metadata.name, "deflate");
        assert_eq!(metadata.magic_number, [0x43, 0x52, 0x01, 0x00]);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_deflate_roundtrip() {
        let plugin = DeflatePlugin;
        let cancel_flag = Arc::new(AtomicBool::new(false));

        let original = b"Hello, DEFLATE! This is a test of the compression algorithm.";
        let compressed = plugin.compress(original, Arc::clone(&cancel_flag)).unwrap();
        let decompressed = plugin.decompress(&compressed, cancel_flag).unwrap();

        assert_eq!(original.as_slice(), decompressed.as_slice());
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_deflate_empty() {
        let plugin = DeflatePlugin;
        let cancel_flag = Arc::new(AtomicBool::new(false));

        let original = b"";
        let compressed = plugin.compress(original, Arc::clone(&cancel_flag)).unwrap();
        let decompressed = plugin.decompress(&compressed, cancel_flag).unwrap();

        assert_eq!(original.as_slice(), decompressed.as_slice());
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_deflate_cancellation() {
        let plugin = DeflatePlugin;
        let cancel_flag = Arc::new(AtomicBool::new(true)); // Pre-cancelled

        let original = b"This should be cancelled";
        let result = plugin.compress(original, cancel_flag);

        assert!(result.is_err());
        // Verify it's a cancellation error by checking the error message
        let err = result.unwrap_err();
        assert!(
            matches!(err, crate::error::CrushError::Plugin(crate::error::PluginError::Cancelled)),
            "Expected PluginError::Cancelled, got: {err:?}"
        );
    }
}
