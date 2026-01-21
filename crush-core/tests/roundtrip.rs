//! Integration tests for compress/decompress roundtrip
//!
//! Following TDD: These tests are written BEFORE implementation.
//! They MUST fail initially, then pass after implementation.

#![allow(clippy::expect_used)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]

use crush_core::{compress, decompress, init_plugins};

/// Test basic roundtrip: compress data and decompress it back
///
/// Verifies that compress(data) followed by decompress(compressed) produces
/// the original data byte-for-byte.
#[test]
fn test_roundtrip_basic() {
    init_plugins().expect("Plugin initialization failed");
    let original = b"Hello, Crush! This is a test of the compression system.";

    // Compress the data
    let compressed = compress(original).expect("Compression should succeed");

    // Compressed data should be smaller or similar size for small inputs
    // (DEFLATE may expand very small inputs due to header overhead)
    assert!(!compressed.is_empty(), "Compressed data should not be empty");

    // Decompress the data
    let decompressed = decompress(&compressed).expect("Decompression should succeed");

    // Verify roundtrip produces identical data
    assert_eq!(
        original.as_slice(),
        decompressed.as_slice(),
        "Decompressed data should match original"
    );
}

/// Test roundtrip with empty data
#[test]
fn test_roundtrip_empty() {
    init_plugins().expect("Plugin initialization failed");
    let original = b"";

    let compressed = compress(original).expect("Compression should succeed on empty data");
    let decompressed = decompress(&compressed).expect("Decompression should succeed");

    assert_eq!(original.as_slice(), decompressed.as_slice());
}

/// Test roundtrip with large data (>1MB)
#[test]
fn test_roundtrip_large() {
    init_plugins().expect("Plugin initialization failed");
    // Create 1MB of repeating pattern (compresses well)
    let original: Vec<u8> = (0..1_000_000).map(|i| (i % 256) as u8).collect();

    let compressed = compress(&original).expect("Compression should succeed");

    // Verify compression actually reduces size for repetitive data
    assert!(
        compressed.len() < original.len(),
        "Compressed size {} should be less than original {}",
        compressed.len(),
        original.len()
    );

    let decompressed = decompress(&compressed).expect("Decompression should succeed");

    assert_eq!(original, decompressed);
}

/// Test roundtrip with random data (incompressible)
#[test]
fn test_roundtrip_random() {
    init_plugins().expect("Plugin initialization failed");
    // Simulate random data (won't compress well)
    let original: Vec<u8> = (0..10_000)
        .map(|i| ((i * 7919) % 256) as u8) // Pseudo-random sequence
        .collect();

    let compressed = compress(&original).expect("Compression should succeed");
    let decompressed = decompress(&compressed).expect("Decompression should succeed");

    assert_eq!(original, decompressed);
}

/// Test that corrupted compressed data produces an error
#[test]
fn test_corrupted_data_invalid_magic() {
    let mut corrupted = vec![0xFF, 0xFF, 0xFF, 0xFF]; // Invalid magic number
    corrupted.extend_from_slice(&[0u8; 12]); // Rest of header
    corrupted.extend_from_slice(b"garbage data");

    let result = decompress(&corrupted);

    assert!(
        result.is_err(),
        "Decompression should fail on invalid magic number"
    );
}

/// Test that truncated compressed data produces an error
#[test]
fn test_corrupted_data_truncated() {
    init_plugins().expect("Plugin initialization failed");
    let original = b"Test data for corruption";
    let compressed = compress(original).expect("Compression should succeed");

    // Truncate to just the header
    let truncated = &compressed[..16];

    let result = decompress(truncated);

    assert!(
        result.is_err(),
        "Decompression should fail on truncated data"
    );
}

/// Test that modified compressed payload produces an error
#[test]
fn test_corrupted_data_modified_payload() {
    init_plugins().expect("Plugin initialization failed");
    let original = b"Test data that will be corrupted";
    let mut compressed = compress(original).expect("Compression should succeed");

    // Corrupt a byte in the payload (skip header)
    if compressed.len() > 20 {
        compressed[20] ^= 0xFF; // Flip all bits in one byte
    }

    let result = decompress(&compressed);

    // Should either fail decompression or fail CRC check
    assert!(
        result.is_err(),
        "Decompression should fail on corrupted payload"
    );
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// Property-based test: ANY data should roundtrip correctly
        ///
        /// This test generates random byte sequences and verifies that
        /// compress→decompress always produces the original data.
        #[test]
        fn proptest_roundtrip(data: Vec<u8>) {
            init_plugins().expect("Plugin initialization failed");
            let compressed = compress(&data).expect("Compression should always succeed");
            let decompressed = decompress(&compressed).expect("Decompression should always succeed");

            prop_assert_eq!(data, decompressed, "Roundtrip failed for data");
        }

        /// Property-based test: Compression should be deterministic
        ///
        /// Compressing the same data twice should produce identical output.
        #[test]
        fn proptest_deterministic(data: Vec<u8>) {
            init_plugins().expect("Plugin initialization failed");
            let compressed1 = compress(&data).expect("Compression should succeed");
            let compressed2 = compress(&data).expect("Compression should succeed");

            prop_assert_eq!(
                compressed1,
                compressed2,
                "Compression should be deterministic"
            );
        }
    }
}
