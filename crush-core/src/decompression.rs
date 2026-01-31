//! Decompression functionality
//!
//! Provides the public `decompress()` API that reads Crush-compressed data,
//! validates headers and checksums, routes to the correct plugin, and decompresses.

use crate::error::{PluginError, Result, ValidationError};
use crate::plugin::registry::get_plugin_by_magic;
use crate::plugin::{list_plugins, CrushHeader, FileMetadata};
use crc32fast::Hasher;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

#[derive(Debug)]
pub struct DecompressionResult {
    pub data: Vec<u8>,
    pub metadata: FileMetadata,
}

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
/// assert_eq!(data.as_slice(), decompressed.data.as_slice());
/// ```
pub fn decompress(input: &[u8]) -> Result<DecompressionResult> {
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

    let mut payload_start = CrushHeader::SIZE;

    // Handle CRC32
    if header.has_crc32() {
        if input.len() < payload_start + 4 {
            return Err(ValidationError::InvalidHeader(
                "Truncated: CRC32 flag set but no CRC32 data".to_string(),
            )
            .into());
        }
        let stored_crc = u32::from_le_bytes([
            input[payload_start],
            input[payload_start + 1],
            input[payload_start + 2],
            input[payload_start + 3],
        ]);
        payload_start += 4;

        let payload_for_crc = &input[payload_start..];
        let mut hasher = Hasher::new();
        hasher.update(payload_for_crc);
        let computed_crc = hasher.finalize();

        if stored_crc != computed_crc {
            return Err(ValidationError::CrcMismatch {
                expected: stored_crc,
                actual: computed_crc,
            }
            .into());
        }
    }

    // Handle metadata
    let metadata = if header.has_metadata() {
        if input.len() < payload_start + 2 {
            return Err(ValidationError::InvalidHeader(
                "Truncated: metadata flag set but no metadata length".to_string(),
            )
            .into());
        }
        let metadata_len =
            u16::from_le_bytes([input[payload_start], input[payload_start + 1]]) as usize;
        payload_start += 2;

        if input.len() < payload_start + metadata_len {
            return Err(ValidationError::InvalidHeader(
                "Truncated: metadata length exceeds payload size".to_string(),
            )
            .into());
        }
        let metadata_bytes = &input[payload_start..payload_start + metadata_len];
        payload_start += metadata_len;

        FileMetadata::from_bytes(metadata_bytes)?
    } else {
        FileMetadata::default()
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

    Ok(DecompressionResult {
        data: decompressed,
        metadata,
    })
}

#[cfg(test)]
#[allow(clippy::expect_used)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::unreadable_literal)]
mod tests {
    use super::*;
    use crate::{compress, init_plugins};

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_decompress_valid() {
        init_plugins().unwrap();
        let original = b"Test data for decompression";
        let compressed = compress(original).unwrap();
        let decompressed = decompress(&compressed).unwrap().data;

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
    fn test_decompress_with_metadata() {
        use crate::plugin::FileMetadata;
        use crate::{compress_with_options, CompressionOptions};

        init_plugins().expect("Failed to init");
        let original = b"Data with metadata";
        let metadata = FileMetadata {
            mtime: Some(1234567890),
            #[cfg(unix)]
            permissions: Some(0o644),
        };
        let options = CompressionOptions::default().with_file_metadata(metadata.clone());
        let compressed = compress_with_options(original, &options).expect("Compression failed");

        let result = decompress(&compressed).expect("Decompression failed");

        assert_eq!(original.as_slice(), result.data.as_slice());
        assert_eq!(result.metadata.mtime, metadata.mtime);
        #[cfg(unix)]
        assert_eq!(result.metadata.permissions, metadata.permissions);
    }

    #[test]
    fn test_decompress_truncated_crc32() {
        init_plugins().expect("Failed to init");
        let original = b"Test";
        let mut compressed = compress(original).expect("Compression failed");

        // Truncate to remove CRC32 bytes
        compressed.truncate(CrushHeader::SIZE); // Just header, no CRC32

        let result = decompress(&compressed);
        assert!(result.is_err()); // Should error about missing CRC32
    }

    #[test]
    fn test_decompress_truncated_metadata_length() {
        use crate::plugin::FileMetadata;
        use crate::{compress_with_options, CompressionOptions};

        init_plugins().expect("Failed to init");
        let original = b"Test";
        let metadata = FileMetadata {
            mtime: Some(1234567890),
            #[cfg(unix)]
            permissions: Some(0o755),
        };
        let options = CompressionOptions::default().with_file_metadata(metadata);
        let mut compressed = compress_with_options(original, &options).expect("Compression failed");

        // Truncate after header + CRC32 to cut metadata length field
        let truncate_pos = CrushHeader::SIZE + 4 + 1;
        if compressed.len() > truncate_pos {
            compressed.truncate(truncate_pos);
        }

        let result = decompress(&compressed);
        assert!(result.is_err());
    }

    #[test]
    fn test_decompress_truncated_metadata_payload() {
        use crate::plugin::FileMetadata;
        use crate::{compress_with_options, CompressionOptions};

        init_plugins().expect("Failed to init");
        let original = b"Test";
        let metadata = FileMetadata {
            mtime: Some(1234567890),
            #[cfg(unix)]
            permissions: Some(0o755),
        };
        let options = CompressionOptions::default().with_file_metadata(metadata);
        let mut compressed = compress_with_options(original, &options).expect("Compression failed");

        // Truncate in middle of metadata payload
        let truncate_pos = CrushHeader::SIZE + 4 + 2 + 3;
        if compressed.len() > truncate_pos {
            compressed.truncate(truncate_pos);
        }

        let result = decompress(&compressed);
        assert!(result.is_err());
    }

    #[test]
    fn test_decompress_plugin_not_found() {
        // Create a valid header but with a magic number for a non-existent plugin
        let mut fake_compressed = vec![
            0x43, 0x52, 0x01, 0xFF, // Magic: CR01 but invalid plugin (0xFF)
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Original size: 0
            0x00, // Flags: 0 (no CRC, no metadata)
            0x00, 0x00, 0x00, // Reserved
        ];
        // Add some fake compressed data
        fake_compressed.extend_from_slice(&[0x78, 0x9c, 0x03, 0x00, 0x00, 0x00, 0x00, 0x01]);

        let result = decompress(&fake_compressed);
        assert!(result.is_err());
        // Error could be about plugin not found or invalid magic
        // Just verify it fails, the exact error depends on implementation details
    }

    #[test]
    fn test_decompress_empty_input() {
        let result = decompress(&[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too short"));
    }

    #[test]
    fn test_decompress_default_metadata() {
        init_plugins().expect("Failed to init");
        let original = b"No metadata test";
        let compressed = compress(original).expect("Compression failed");

        let result = decompress(&compressed).expect("Decompression failed");

        // Should have default (empty) metadata when none was provided
        assert!(result.metadata.mtime.is_none());
        #[cfg(unix)]
        assert!(result.metadata.permissions.is_none());
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
