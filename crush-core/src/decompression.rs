//! Decompression functionality
//!
//! Provides the public `decompress()` API that reads Crush-compressed data,
//! validates headers and checksums, routes to the correct plugin, and decompresses.

use crate::error::{PluginError, Result, ValidationError};
use crate::plugin::registry::get_plugin_by_magic;
use crate::plugin::{list_plugins, CrushHeader};
use crc32fast::Hasher;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// Decompress Crush-compressed data
///
/// Reads the Crush header to identify the compression plugin, validates the CRC32
/// checksum (if present), and decompresses the data using the appropriate plugin.
///
/// # Errors
///
/// Returns an error if:
/// - Input data is too short (less than 16-byte header)
/// - Header magic number is invalid
/// - Header version is unsupported
/// - Required plugin is not registered
/// - CRC32 checksum validation fails
/// - Decompression operation fails
///
/// # Examples
///
/// ```
/// use crush_core::{init_plugins, compress, decompress};
///
/// init_plugins().expect("Plugin initialization failed");
/// let data = b"Hello, world!";
/// let compressed = compress(data).expect("Compression failed");
/// let decompressed = decompress(&compressed).expect("Decompression failed");
/// assert_eq!(data.as_slice(), decompressed.as_slice());
/// ```
pub fn decompress(input: &[u8]) -> Result<Vec<u8>> {
    // Validate minimum size (header + CRC32 if present)
    if input.len() < CrushHeader::SIZE {
        return Err(ValidationError::InvalidHeader(format!(
            "Input too short: {} bytes, expected at least {}",
            input.len(),
            CrushHeader::SIZE
        ))
        .into());
    }

    // Parse header
    let header_bytes: [u8; CrushHeader::SIZE] = input[0..CrushHeader::SIZE]
        .try_into()
        .map_err(|_| ValidationError::InvalidHeader("Failed to read header".to_string()))?;
    let header = CrushHeader::from_bytes(&header_bytes)?;

    // Determine payload start based on CRC32 flag
    let payload_start = if header.has_crc32() {
        // CRC32 follows header (4 bytes)
        if input.len() < CrushHeader::SIZE + 4 {
            return Err(ValidationError::InvalidHeader(
                "Truncated: CRC32 flag set but no CRC32 data".to_string(),
            )
            .into());
        }

        // Extract and validate CRC32
        let stored_crc = u32::from_le_bytes([
            input[CrushHeader::SIZE],
            input[CrushHeader::SIZE + 1],
            input[CrushHeader::SIZE + 2],
            input[CrushHeader::SIZE + 3],
        ]);

        let payload = &input[CrushHeader::SIZE + 4..];

        // Calculate CRC32 of compressed payload
        let mut hasher = Hasher::new();
        hasher.update(payload);
        let computed_crc = hasher.finalize();

        if stored_crc != computed_crc {
            return Err(ValidationError::CrcMismatch {
                expected: stored_crc,
                actual: computed_crc,
            }
            .into());
        }

        CrushHeader::SIZE + 4
    } else {
        CrushHeader::SIZE
    };

    let compressed_payload = &input[payload_start..];

    // Find plugin by magic number from registry
    let plugin = get_plugin_by_magic(header.magic).ok_or_else(|| {
        let available = list_plugins()
            .iter()
            .map(|p| p.name)
            .collect::<Vec<_>>()
            .join(", ");

        PluginError::NotFound(format!(
            "No plugin found for magic number {:02X?}. \
             Available plugins: {}. \
             Did you call init_plugins()?",
            header.magic, available
        ))
    })?;

    // Create cancellation flag (not yet connected to timeout system)
    let cancel_flag = Arc::new(AtomicBool::new(false));

    // Decompress the payload
    let decompressed = plugin.decompress(compressed_payload, cancel_flag)?;

    // Validate decompressed size matches header
    let expected_size = usize::try_from(header.original_size).map_err(|_| {
        ValidationError::InvalidHeader("Original size exceeds platform limits".to_string())
    })?;

    if decompressed.len() != expected_size {
        return Err(ValidationError::CorruptedData(format!(
            "Size mismatch: header says {} bytes, got {} bytes",
            header.original_size,
            decompressed.len()
        ))
        .into());
    }

    Ok(decompressed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{compress, init_plugins};

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_decompress_valid() {
        init_plugins().unwrap();
        let original = b"Test data for decompression";
        let compressed = compress(original).unwrap();
        let decompressed = decompress(&compressed).unwrap();

        assert_eq!(original.as_slice(), decompressed.as_slice());
    }

    #[test]
    fn test_decompress_truncated() {
        let truncated = &[0x43, 0x52, 0x01, 0x00, 0x01]; // Only 5 bytes

        let result = decompress(truncated);
        assert!(result.is_err());
    }

    #[test]
    fn test_decompress_invalid_magic() {
        let mut invalid = vec![0xFF, 0xFF, 0xFF, 0xFF]; // Bad magic
        invalid.extend_from_slice(&[0u8; 12]); // Rest of header

        let result = decompress(&invalid);
        assert!(result.is_err());
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_decompress_corrupted_crc() {
        init_plugins().unwrap();
        let original = b"Data to corrupt";
        let mut compressed = compress(original).unwrap();

        // Corrupt the CRC32 (bytes 16-19)
        if compressed.len() > 16 {
            compressed[16] ^= 0xFF;
        }

        let result = decompress(&compressed);
        assert!(result.is_err());
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_decompress_corrupted_payload() {
        init_plugins().unwrap();
        let original = b"Data to corrupt";
        let mut compressed = compress(original).unwrap();

        // Corrupt a byte in the payload (after header + CRC32)
        if compressed.len() > 24 {
            compressed[24] ^= 0xFF;
        }

        let result = decompress(&compressed);
        // Should fail either in decompression or size validation
        assert!(result.is_err());
    }
}
