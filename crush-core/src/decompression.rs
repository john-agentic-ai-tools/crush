//! Decompression functionality
//!
//! Provides the public `decompress()` API that reads Crush-compressed data,
//! validates headers and checksums, routes to the correct plugin, and decompresses.

use crate::error::{PluginError, Result, ValidationError};
use crate::plugin::registry::get_plugin_by_magic;
use crate::plugin::{CompressionAlgorithm, CrushHeader, FileMetadata, list_plugins};
use crc32fast::Hasher;
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

/// Read and compute the CRC32 block that follows a Crush header.
///
/// Returns `(stored_crc, computed_crc, new_payload_start)` where:
/// - `stored_crc` is the checksum recorded in the stream
/// - `computed_crc` is the checksum computed over the remaining payload
/// - `new_payload_start` is the byte offset at which the actual payload begins
///
/// # Errors
///
/// Returns an error if the input is truncated (fewer than 4 bytes remain at
/// `payload_start`).
pub(crate) fn read_crc32_block(input: &[u8], payload_start: usize) -> Result<(u32, u32, usize)> {
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
    let new_payload_start = payload_start + 4;

    let payload_for_crc = &input[new_payload_start..];
    let mut hasher = Hasher::new();
    hasher.update(payload_for_crc);
    let computed_crc = hasher.finalize();

    Ok((stored_crc, computed_crc, new_payload_start))
}

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
    let cancel_flag = Arc::new(AtomicBool::new(false));
    decompress_with_cancel(input, cancel_flag)
}

/// Decompress Crush-compressed data with an externally-controlled cancellation flag.
///
/// This variant accepts a caller-provided `cancel_flag` so that signal handlers
/// (e.g. Ctrl+C) can interrupt long-running GPU kernel dispatches.
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
/// - The cancel flag is set during decompression
pub fn decompress_with_cancel(
    input: &[u8],
    cancel_flag: Arc<AtomicBool>,
) -> Result<DecompressionResult> {
    let c = open_container(input)?;

    // Decompress the payload using the caller-provided cancel flag
    let decompressed = c.plugin.decompress(c.payload, cancel_flag)?;
    check_size(c.expected_size, decompressed.len() as u64)?;

    Ok(DecompressionResult {
        data: decompressed,
        metadata: c.metadata,
    })
}

/// Result of [`decompress_to_writer`].
#[derive(Debug, Clone)]
pub struct StreamedDecompression {
    /// Number of bytes written (equal to the header's original size).
    pub bytes: u64,

    /// File metadata stored with the data (mtime, permissions)
    pub metadata: FileMetadata,
}

/// Decompress Crush-compressed data into `out` without materialising the
/// whole result when the plugin can stream (see
/// `CompressionAlgorithm::decompress_to`).
///
/// The header and the CRC32 of the compressed payload are checked before
/// anything is written, and the number of bytes written is checked against
/// the header's original size at the end.
///
/// # Errors
///
/// As for [`decompress_with_cancel`], plus I/O errors from `out`. On error,
/// `out` may already have received part of the data; callers writing to a
/// file should remove it.
pub fn decompress_to_writer(
    input: &[u8],
    cancel_flag: Arc<AtomicBool>,
    out: &mut dyn Write,
) -> Result<StreamedDecompression> {
    let c = open_container(input)?;
    let bytes = c.plugin.decompress_to(c.payload, cancel_flag, out)?;
    check_size(c.expected_size, bytes)?;
    Ok(StreamedDecompression {
        bytes,
        metadata: c.metadata,
    })
}

/// A parsed, CRC-checked Crush container.
struct Container<'a> {
    payload: &'a [u8],
    metadata: FileMetadata,
    plugin: &'static dyn CompressionAlgorithm,
    expected_size: u64,
}

/// Parse the header, verify the CRC32, read the metadata, and find the plugin.
fn open_container(input: &[u8]) -> Result<Container<'_>> {
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
        let (stored_crc, computed_crc, new_start) = read_crc32_block(input, payload_start)?;
        payload_start = new_start;
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

    Ok(Container {
        payload: compressed_payload,
        metadata,
        plugin,
        expected_size: header.original_size,
    })
}

/// The decoded length must match the header's original size.
fn check_size(expected: u64, got: u64) -> Result<()> {
    if got == expected {
        Ok(())
    } else {
        Err(ValidationError::CorruptedData(format!(
            "Size mismatch: header says {expected} bytes, got {got} bytes"
        ))
        .into())
    }
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

    /// Streaming output equals the buffered result, and metadata comes back.
    #[test]
    fn decompress_to_writer_matches_decompress() {
        use crate::plugin::FileMetadata;
        use crate::{CompressionOptions, compress_with_options};

        init_plugins().expect("init");
        let original: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        let meta = FileMetadata {
            mtime: Some(1_700_000_000),
            #[cfg(unix)]
            permissions: Some(0o640),
        };
        let options = CompressionOptions::default()
            .with_plugin("deflate")
            .with_file_metadata(meta.clone());
        let compressed = compress_with_options(&original, &options).expect("compress");

        let mut sink = Vec::new();
        let flag = Arc::new(AtomicBool::new(false));
        let got = decompress_to_writer(&compressed, flag, &mut sink).expect("stream");
        assert_eq!(sink, original);
        assert_eq!(got.bytes, original.len() as u64);
        assert_eq!(got.metadata, meta);
    }

    /// A CRC mismatch is reported before anything reaches the writer.
    #[test]
    fn decompress_to_writer_checks_crc_before_writing() {
        init_plugins().expect("init");
        let mut compressed = compress(&[7u8; 10_000]).expect("compress");
        let last = compressed.len() - 1;
        compressed[last] ^= 0xFF;
        let mut sink = Vec::new();
        let flag = Arc::new(AtomicBool::new(false));
        assert!(decompress_to_writer(&compressed, flag, &mut sink).is_err());
        assert!(
            sink.is_empty(),
            "{} bytes written before the CRC check",
            sink.len()
        );
    }

    /// A payload that decodes to a different size than the header says is
    /// an error (the CRC is recomputed so only the size is wrong).
    #[test]
    fn decompress_to_writer_checks_the_size() {
        init_plugins().expect("init");
        let mut compressed = compress(&[7u8; 10_000]).expect("compress");
        let mut header =
            CrushHeader::from_bytes(&compressed[..CrushHeader::SIZE].try_into().expect("header"))
                .expect("parse");
        header.original_size += 1;
        compressed[..CrushHeader::SIZE].copy_from_slice(&header.to_bytes());
        let mut sink = Vec::new();
        let flag = Arc::new(AtomicBool::new(false));
        assert!(decompress_to_writer(&compressed, flag, &mut sink).is_err());
    }

    #[test]
    fn test_decompress_with_metadata() {
        use crate::plugin::FileMetadata;
        use crate::{CompressionOptions, compress_with_options};

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
        use crate::{CompressionOptions, compress_with_options};

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
        use crate::{CompressionOptions, compress_with_options};

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
