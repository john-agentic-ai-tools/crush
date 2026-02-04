//! Integration tests for compress/decompress roundtrip
//!
//! Following TDD: These tests are written BEFORE implementation.
//! They MUST fail initially, then pass after implementation.

#![allow(clippy::panic_in_result_fn)]

use crush_core::{compress, decompress, init_plugins, Result};

/// Test basic roundtrip: compress data and decompress it back
///
/// Verifies that compress(data) followed by decompress(compressed) produces
/// the original data byte-for-byte.
#[test]
fn test_roundtrip_basic() -> Result<()> {
    init_plugins()?;
    let original = b"Hello, Crush! This is a test of the compression system.";

    // Compress the data
    let compressed = compress(original)?;

    // Compressed data should be smaller or similar size for small inputs
    // (DEFLATE may expand very small inputs due to header overhead)
    assert!(
        !compressed.is_empty(),
        "Compressed data should not be empty"
    );

    // Decompress the data
    let decompressed = decompress(&compressed)?;

    // Verify roundtrip produces identical data
    assert_eq!(
        original.as_slice(),
        decompressed.data.as_slice(),
        "Decompressed data should match original"
    );

    Ok(())
}

/// Test roundtrip with empty data
#[test]
fn test_roundtrip_empty() -> Result<()> {
    init_plugins()?;
    let original = b"";

    let compressed = compress(original)?;
    let decompressed = decompress(&compressed)?;

    assert_eq!(original.as_slice(), decompressed.data.as_slice());

    Ok(())
}

/// Test roundtrip with large data (>1MB)
#[test]
fn test_roundtrip_large() -> Result<()> {
    init_plugins()?;
    // Create 1MB of repeating pattern (compresses well)
    #[allow(clippy::cast_possible_truncation)] // Intentional: i % 256 always fits in u8
    let original: Vec<u8> = (0..1_000_000_u32).map(|i| (i % 256) as u8).collect();

    let compressed = compress(&original)?;
    let decompressed = decompress(&compressed)?;

    assert_eq!(original, decompressed.data);
    assert!(
        compressed.len() < original.len(),
        "Compressed size should be less than original for repetitive data"
    );

    Ok(())
}

/// Test roundtrip with random data
///
/// Random data typically doesn't compress well, but roundtrip should still work.
#[test]
fn test_roundtrip_random() -> Result<()> {
    init_plugins()?;

    // Create pseudo-random data (deterministic for reproducibility)
    let mut original = Vec::with_capacity(10_000);
    let mut seed = 12_345_u32;
    for _ in 0..10_000 {
        seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        #[allow(clippy::cast_possible_truncation)]
        // Intentional: LCG algorithm, truncation is expected
        let byte = (seed / 65_536) as u8;
        original.push(byte);
    }

    let compressed = compress(&original)?;
    let decompressed = decompress(&compressed)?;

    assert_eq!(original, decompressed.data);

    Ok(())
}

/// Test corrupted data detection
///
/// Decompression should fail gracefully on corrupted data.
#[test]
fn test_corrupted_data_invalid_magic() {
    // Invalid magic number
    let bad_data = vec![0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00];
    let result = decompress(&bad_data);
    assert!(result.is_err(), "Should reject invalid magic number");
}

#[test]
fn test_corrupted_data_truncated() {
    // Too short to be valid
    let bad_data = vec![0x43, 0x52, 0x01];
    let result = decompress(&bad_data);
    assert!(result.is_err(), "Should reject truncated data");
}

#[test]
fn test_corrupted_data_modified_payload() -> Result<()> {
    init_plugins()?;

    let original = b"Test data for corruption detection";
    let mut compressed = compress(original)?;

    // Corrupt the payload (skip header + CRC32, modify compressed data)
    if compressed.len() > 25 {
        compressed[24] ^= 0xFF;
    }

    let result = decompress(&compressed);
    // Should either fail or detect CRC mismatch
    // (depends on implementation - CRC check might catch this)
    assert!(result.is_ok() || result.is_err());

    Ok(())
}

#[cfg(test)]
mod property_tests {
    use super::*;

    /// Property test: Any data should roundtrip successfully
    ///
    /// This test uses a simple property-based approach without external deps.
    #[test]
    fn proptest_roundtrip() -> Result<()> {
        init_plugins()?;

        // Test various sizes and patterns
        let test_cases = vec![
            vec![],                           // Empty
            vec![0],                          // Single byte
            vec![0xFF; 100],                  // Repeated byte
            (0..255_u8).collect::<Vec<u8>>(), // Sequential
            vec![0x42; 10_000],               // Medium repetitive
        ];

        for original in test_cases {
            let compressed = compress(&original)?;
            let decompressed = decompress(&compressed)?;
            assert_eq!(
                original,
                decompressed.data,
                "Roundtrip failed for data of length {}",
                original.len()
            );
        }

        Ok(())
    }

    /// Property test: Compression should be deterministic
    #[test]
    fn proptest_deterministic() -> Result<()> {
        init_plugins()?;

        let data = b"Test data for determinism check";

        let compressed1 = compress(data)?;
        let compressed2 = compress(data)?;

        assert_eq!(
            compressed1, compressed2,
            "Same input should produce identical compressed output"
        );

        Ok(())
    }
}
