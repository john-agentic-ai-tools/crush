//! Tests for file inspection functionality

#![allow(clippy::expect_used)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::unreadable_literal)]
#![allow(clippy::uninlined_format_args)]

use crush_core::plugin::FileMetadata;
use crush_core::{compress_with_options, init_plugins, inspect, CompressionOptions};

#[test]
fn test_inspect_valid_file_with_crc() {
    init_plugins().expect("Failed to initialize plugins");

    let data = b"Hello, inspection test!";
    let compressed =
        compress_with_options(data, &CompressionOptions::default()).expect("Compression failed");

    let result = inspect(&compressed).expect("Inspection failed");

    assert_eq!(result.original_size, data.len() as u64);
    assert_eq!(result.compressed_size, compressed.len() as u64);
    assert_eq!(result.plugin_name, "deflate");
    assert!(result.crc_valid, "CRC should be valid");
}

#[test]
fn test_inspect_with_metadata() {
    init_plugins().expect("Failed to initialize plugins");

    let data = b"Test data with metadata";
    let metadata = FileMetadata {
        mtime: Some(1234567890),
        #[cfg(unix)]
        permissions: Some(0o644),
    };

    let options = CompressionOptions::default().with_file_metadata(metadata.clone());
    let compressed = compress_with_options(data, &options).expect("Compression failed");

    let result = inspect(&compressed).expect("Inspection failed");

    assert_eq!(result.original_size, data.len() as u64);
    assert_eq!(result.plugin_name, "deflate");
    assert!(result.crc_valid);
    assert_eq!(result.metadata.mtime, metadata.mtime);
    #[cfg(unix)]
    assert_eq!(result.metadata.permissions, metadata.permissions);
}

#[test]
fn test_inspect_input_too_short() {
    init_plugins().expect("Failed to initialize plugins");

    let short_input = vec![0u8; 10]; // Less than CrushHeader::SIZE (16)
    let result = inspect(&short_input);

    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Input too short") || err_msg.contains("expected at least"),
        "Error message: {}",
        err_msg
    );
}

#[test]
fn test_inspect_invalid_header() {
    init_plugins().expect("Failed to initialize plugins");

    // Create 16 bytes of invalid data (wrong magic number)
    let invalid_header = vec![0xFFu8; 16];
    let result = inspect(&invalid_header);

    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Invalid magic") || err_msg.contains("magic number"),
        "Error message: {}",
        err_msg
    );
}

#[test]
fn test_inspect_truncated_crc() {
    init_plugins().expect("Failed to initialize plugins");

    let data = b"Test";
    let mut compressed =
        compress_with_options(data, &CompressionOptions::default()).expect("Compression failed");

    // Truncate to remove CRC32 data (keep header but remove CRC32 bytes)
    compressed.truncate(16); // Just the header, no CRC32

    let result = inspect(&compressed);

    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Truncated") || err_msg.contains("CRC32"),
        "Error message: {}",
        err_msg
    );
}

#[test]
fn test_inspect_invalid_crc() {
    init_plugins().expect("Failed to initialize plugins");

    let data = b"Test data for CRC validation";
    let mut compressed =
        compress_with_options(data, &CompressionOptions::default()).expect("Compression failed");

    // Corrupt the CRC32 value (bytes 16-19 after header)
    if compressed.len() > 19 {
        compressed[16] ^= 0xFF; // Flip bits in CRC32
    }

    let result = inspect(&compressed).expect("Inspection should succeed");

    // The inspect function should detect invalid CRC
    assert!(
        !result.crc_valid,
        "CRC should be marked as invalid after corruption"
    );
}

#[test]
fn test_inspect_corrupted_payload() {
    init_plugins().expect("Failed to initialize plugins");

    let data = b"Test data";
    let mut compressed =
        compress_with_options(data, &CompressionOptions::default()).expect("Compression failed");

    // Corrupt the payload (not the CRC)
    if compressed.len() > 25 {
        compressed[24] ^= 0xFF;
    }

    let result = inspect(&compressed).expect("Inspection should succeed");

    // CRC should be invalid due to payload corruption
    assert!(
        !result.crc_valid,
        "CRC should be invalid after payload corruption"
    );
}

#[test]
fn test_inspect_empty_data() {
    init_plugins().expect("Failed to initialize plugins");

    let data = b"";
    let compressed =
        compress_with_options(data, &CompressionOptions::default()).expect("Compression failed");

    let result = inspect(&compressed).expect("Inspection failed");

    assert_eq!(result.original_size, 0);
    assert_eq!(result.plugin_name, "deflate");
    assert!(result.crc_valid);
}

#[test]
fn test_inspect_large_data() {
    init_plugins().expect("Failed to initialize plugins");

    let data = vec![0x42u8; 10_000]; // 10KB
    let compressed =
        compress_with_options(&data, &CompressionOptions::default()).expect("Compression failed");

    let result = inspect(&compressed).expect("Inspection failed");

    assert_eq!(result.original_size, data.len() as u64);
    assert_eq!(result.plugin_name, "deflate");
    assert!(result.crc_valid);
    assert!(result.compressed_size < result.original_size);
}

#[test]
fn test_inspect_metadata_truncated_length() {
    init_plugins().expect("Failed to initialize plugins");

    let data = b"Test";
    let metadata = FileMetadata {
        mtime: Some(1234567890),
        #[cfg(unix)]
        permissions: None,
    };

    let options = CompressionOptions::default().with_file_metadata(metadata);
    let mut compressed = compress_with_options(data, &options).expect("Compression failed");

    // Find where metadata length starts (after header + CRC32)
    let metadata_len_pos = 16 + 4; // CrushHeader::SIZE + CRC32 size

    // Truncate right after metadata length field starts
    if compressed.len() > metadata_len_pos + 1 {
        compressed.truncate(metadata_len_pos + 1); // Keep only 1 byte of 2-byte length
    }

    let result = inspect(&compressed);

    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Truncated") || err_msg.contains("metadata"),
        "Error message: {}",
        err_msg
    );
}

#[test]
fn test_inspect_metadata_truncated_payload() {
    init_plugins().expect("Failed to initialize plugins");

    let data = b"Test";
    let metadata = FileMetadata {
        mtime: Some(1234567890),
        #[cfg(unix)]
        permissions: Some(0o755),
    };

    let options = CompressionOptions::default().with_file_metadata(metadata);
    let mut compressed = compress_with_options(data, &options).expect("Compression failed");

    // Truncate in the middle of metadata payload
    let metadata_start = 16 + 4 + 2; // Header + CRC32 + metadata length field
    if compressed.len() > metadata_start + 5 {
        compressed.truncate(metadata_start + 5); // Keep only partial metadata
    }

    let result = inspect(&compressed);

    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Truncated")
            || err_msg.contains("metadata")
            || err_msg.contains("exceeds"),
        "Error message: {}",
        err_msg
    );
}

#[test]
fn test_inspect_default_metadata() {
    init_plugins().expect("Failed to initialize plugins");

    let data = b"Test without metadata";
    // Use default options (no metadata)
    let compressed =
        compress_with_options(data, &CompressionOptions::default()).expect("Compression failed");

    let result = inspect(&compressed).expect("Inspection failed");

    // Should have default (empty) metadata
    assert!(result.metadata.mtime.is_none());
    #[cfg(unix)]
    assert!(result.metadata.permissions.is_none());
}

#[test]
fn test_inspect_result_serialization() {
    init_plugins().expect("Failed to initialize plugins");

    let data = b"Serialization test";
    let compressed =
        compress_with_options(data, &CompressionOptions::default()).expect("Compression failed");

    let result = inspect(&compressed).expect("Inspection failed");

    // Test that InspectResult can be serialized to JSON
    let json = serde_json::to_string(&result).expect("Failed to serialize to JSON");

    assert!(json.contains("original_size"));
    assert!(json.contains("compressed_size"));
    assert!(json.contains("plugin_name"));
    assert!(json.contains("crc_valid"));
    assert!(json.contains("metadata"));
}
