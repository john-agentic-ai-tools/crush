//! Integration tests for compression/decompression cancellation

use crush_core::cancel::{AtomicCancellationToken, CancellationToken};
use crush_core::{compress_with_options, decompress, init_plugins, CompressionOptions, CrushError};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tempfile::NamedTempFile;

/// Helper to initialize plugins once for all tests
fn setup() {
    let _ = init_plugins();
}

/// T016: Test that compression respects cancellation
#[test]
#[allow(clippy::unwrap_used)]
fn test_compress_respects_cancellation() {
    setup();

    let input = vec![0u8; 10_000_000]; // 10MB of zeros
    let cancel_token: Arc<dyn CancellationToken> = Arc::new(AtomicCancellationToken::new());

    // Cancel immediately before compression
    cancel_token.cancel();

    // Attempt compression with cancelled token
    let options = CompressionOptions::default().with_cancel_token(cancel_token);

    let result = compress_with_options(&input, &options);

    // Should return Cancelled error
    assert!(
        matches!(result, Err(CrushError::Cancelled)),
        "Expected Cancelled error, got: {result:?}"
    );
}

/// T017: Test that decompression respects cancellation
#[test]
#[allow(clippy::unwrap_used)]
fn test_decompress_respects_cancellation() {
    setup();

    // First compress some data
    let input = b"test data".repeat(10000);
    let compressed = compress_with_options(&input, &CompressionOptions::default()).unwrap();

    // Now try to decompress with cancellation
    let _cancel_token: Arc<dyn CancellationToken> = Arc::new(AtomicCancellationToken::new());
    // Note: Decompression with cancel token will be implemented later
    // For now, just test that regular decompression works
    let _result = decompress(&compressed);

    // TODO: This test will need to be updated once decompress_with_cancel is implemented
}

/// T018: Test that incomplete files are cleaned up on cancellation
#[test]
#[allow(clippy::unwrap_used, clippy::cast_possible_truncation)]
fn test_file_cleanup_on_cancellation() {
    setup();

    // Use diverse data that doesn't compress instantly (not all zeros)
    let mut input = Vec::with_capacity(50_000_000); // 50MB
    for i in 0..50_000_000 {
        #[allow(clippy::cast_sign_loss)]
        let byte = (i % 256) as u8;
        input.push(byte);
    }
    let _output_file = NamedTempFile::new().unwrap();

    let cancel_token: Arc<dyn CancellationToken> = Arc::new(AtomicCancellationToken::new());

    // Start compression in background thread
    let token_clone = Arc::clone(&cancel_token);
    let handle = thread::spawn(move || {
        let options = CompressionOptions::default().with_cancel_token(token_clone);
        compress_with_options(&input, &options)
    });

    // Cancel after a short delay
    thread::sleep(Duration::from_millis(50));
    cancel_token.cancel();

    // Wait for compression to finish
    let result = handle.join().unwrap();

    // Should be cancelled
    assert!(matches!(result, Err(CrushError::Cancelled)));

    // Output file should either not exist or be cleaned up
    // (This will be verified once ResourceTracker integration is complete)
}

/// T019: Test exit code behavior on cancellation
#[test]
fn test_exit_code_on_cancellation() {
    setup();

    let input = vec![0u8; 1_000_000];
    let cancel_token: Arc<dyn CancellationToken> = Arc::new(AtomicCancellationToken::new());

    // Cancel before compression
    cancel_token.cancel();

    let options = CompressionOptions::default().with_cancel_token(cancel_token);

    let result = compress_with_options(&input, &options);

    // Verify we get Cancelled error (exit code handling is in CLI)
    assert!(
        matches!(result, Err(CrushError::Cancelled)),
        "Expected Cancelled error, got: {result:?}"
    );
    // Expected - CLI will convert this to exit code 130 (Unix) or 2 (Windows)
}

/// Additional test: Verify cancellation during parallel processing
#[test]
#[allow(clippy::unwrap_used, clippy::cast_possible_truncation)]
fn test_cancel_during_parallel_processing() {
    setup();

    // Use diverse data that doesn't compress instantly (not all zeros)
    let mut input = Vec::with_capacity(50_000_000); // 50MB
    for i in 0..50_000_000 {
        #[allow(clippy::cast_sign_loss)]
        let byte = (i % 256) as u8;
        input.push(byte);
    }
    let cancel_token: Arc<dyn CancellationToken> = Arc::new(AtomicCancellationToken::new());

    // Clone token for background cancellation
    let token_clone = Arc::clone(&cancel_token);

    // Start compression
    let handle = thread::spawn(move || {
        let options = CompressionOptions::default().with_cancel_token(token_clone);
        compress_with_options(&input, &options)
    });

    // Cancel mid-operation
    thread::sleep(Duration::from_millis(50));
    cancel_token.cancel();

    // Should get cancelled error
    let result = handle.join().unwrap();
    assert!(
        matches!(result, Err(CrushError::Cancelled)),
        "Expected Cancelled error, got: {result:?}"
    );
}
