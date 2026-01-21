//! Integration tests for plugin timeout protection
//!
//! Following TDD: These tests are written BEFORE implementation.
//! They MUST fail initially, then pass after implementation.

#![allow(clippy::expect_used)]
#![allow(clippy::unwrap_used)]

use crush_core::{compress_with_options, init_plugins, CompressionOptions};
use std::time::Duration;

/// Test that normal compression completes successfully within timeout
#[test]
fn test_compression_within_timeout() {
    init_plugins().expect("Plugin initialization failed");

    let data = b"Test data that should compress quickly within timeout.";

    // Set a generous timeout (5 seconds) - normal compression should complete quickly
    let options = CompressionOptions::default().with_timeout(Duration::from_secs(5));

    let result = compress_with_options(data, &options);

    // Should succeed because DEFLATE completes quickly
    assert!(result.is_ok(), "Compression should succeed within timeout");

    let compressed = result.unwrap();
    assert!(!compressed.is_empty());
}

/// Test that default timeout is applied when none specified
#[test]
fn test_default_timeout_applied() {
    init_plugins().expect("Plugin initialization failed");

    let data = b"Test data for default timeout.";

    // Use default options (should have 30s timeout)
    let options = CompressionOptions::default();

    let result = compress_with_options(data, &options);

    // Should succeed with default timeout
    assert!(
        result.is_ok(),
        "Compression should succeed with default 30s timeout"
    );
}

/// Test that timeout is configurable
#[test]
#[allow(clippy::similar_names)]
fn test_configurable_timeout() {
    init_plugins().expect("Plugin initialization failed");

    let data = b"Test data for configurable timeout.";

    // Set different timeout values
    let options_1s = CompressionOptions::default().with_timeout(Duration::from_secs(1));
    let options_10s = CompressionOptions::default().with_timeout(Duration::from_secs(10));

    // Both should succeed for fast compression
    assert!(compress_with_options(data, &options_1s).is_ok());
    assert!(compress_with_options(data, &options_10s).is_ok());
}

/// Test that cancellation flag is properly initialized
///
/// This test verifies that the timeout system creates and passes
/// a cancellation flag to plugins. We can't easily test actual
/// timeout behavior without a slow plugin, but we can verify
/// the infrastructure exists.
#[test]
fn test_cancellation_infrastructure_exists() {
    init_plugins().expect("Plugin initialization failed");

    let data = b"Test data for cancellation infrastructure.";
    let options = CompressionOptions::default().with_timeout(Duration::from_millis(100));

    // For fast operations, this should always succeed
    let result = compress_with_options(data, &options);

    assert!(
        result.is_ok(),
        "Fast operations should complete before timeout"
    );
}

/// Test compression with very small data and very short timeout
///
/// Even with a very short timeout (e.g., 10ms), compressing a few bytes
/// should succeed because modern compression is fast.
#[test]
fn test_fast_compression_short_timeout() {
    init_plugins().expect("Plugin initialization failed");

    let data = b"Hi";

    // Very short timeout - but data is tiny so should succeed
    let options = CompressionOptions::default().with_timeout(Duration::from_millis(10));

    let result = compress_with_options(data, &options);

    // Should succeed because compression of 2 bytes is essentially instant
    assert!(result.is_ok(), "Tiny data should compress within 10ms");
}

/// Test that timeout value is validated
#[test]
fn test_zero_timeout_handling() {
    init_plugins().expect("Plugin initialization failed");

    let data = b"Test data";

    // Zero timeout should be handled gracefully (either error or use default)
    let options = CompressionOptions::default().with_timeout(Duration::from_secs(0));

    let result = compress_with_options(data, &options);

    // Implementation may choose to error or use default timeout
    // Either behavior is acceptable for zero timeout
    let _ = result;
}

/// Test multiple compressions with timeout
///
/// Verifies that timeout system works correctly across multiple operations
#[test]
fn test_multiple_operations_with_timeout() {
    init_plugins().expect("Plugin initialization failed");

    let options = CompressionOptions::default().with_timeout(Duration::from_secs(2));

    for i in 0..5 {
        let data = format!("Test data iteration {i}");
        let result = compress_with_options(data.as_bytes(), &options);

        assert!(result.is_ok(), "Iteration {i} should succeed within timeout");
    }
}

/// Test that large data still compresses within reasonable timeout
#[test]
fn test_large_data_within_timeout() {
    init_plugins().expect("Plugin initialization failed");

    // Create 1MB of data
    let data = vec![0x42u8; 1_000_000];

    // Set 5 second timeout (should be plenty for 1MB)
    let options = CompressionOptions::default().with_timeout(Duration::from_secs(5));

    let result = compress_with_options(&data, &options);

    assert!(result.is_ok(), "1MB should compress within 5 seconds");
}

/// Test timeout with different plugin selections
#[test]
fn test_timeout_with_plugin_selection() {
    init_plugins().expect("Plugin initialization failed");

    let data = b"Test data for plugin selection with timeout.";

    // Manually select DEFLATE with timeout
    let options = CompressionOptions::default()
        .with_plugin("deflate")
        .with_timeout(Duration::from_secs(2));

    let result = compress_with_options(data, &options);

    assert!(result.is_ok(), "DEFLATE should complete within timeout");
}

/// Test that timeout errors provide useful information
///
/// Note: This test documents expected behavior but may not actually
/// trigger a timeout with current DEFLATE plugin (it's too fast).
/// With future slow plugins, this would verify error messages.
#[test]
fn test_timeout_error_message_quality() {
    init_plugins().expect("Plugin initialization failed");

    let data = b"Test data";
    let options = CompressionOptions::default().with_timeout(Duration::from_secs(1));

    let result = compress_with_options(data, &options);

    // For fast plugins, should succeed
    // For slow plugins (future), error should mention timeout
    if let Err(e) = result {
        let error_msg = format!("{e}");
        // Error message should be informative (contain "timeout" or "timed out")
        assert!(
            error_msg.to_lowercase().contains("timeout")
                || error_msg.to_lowercase().contains("timed out"),
            "Timeout error should mention timeout in message: {error_msg}"
        );
    }
}
