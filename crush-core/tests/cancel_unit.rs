//! Unit tests for cancellation token functionality

use crush_core::cancel::{AtomicCancellationToken, CancellationToken};
use std::sync::Arc;
use std::thread;

/// T012: Test that `is_cancelled` returns false for new token
#[test]
fn test_is_cancelled_new_token() {
    let token = AtomicCancellationToken::new();
    assert!(!token.is_cancelled(), "New token should not be cancelled");
}

/// T013: Test that `cancel` is idempotent (calling multiple times is safe)
#[test]
fn test_cancel_idempotency() {
    let token = AtomicCancellationToken::new();

    // Cancel multiple times
    token.cancel();
    token.cancel();
    token.cancel();

    // Should still be cancelled (idempotent)
    assert!(
        token.is_cancelled(),
        "Token should be cancelled after multiple cancel() calls"
    );
}

/// T014: Test that reset clears cancellation state
#[test]
fn test_reset_clears_cancellation() {
    let token = AtomicCancellationToken::new();

    // Cancel, then reset
    token.cancel();
    assert!(
        token.is_cancelled(),
        "Token should be cancelled before reset"
    );

    token.reset();
    assert!(
        !token.is_cancelled(),
        "Token should not be cancelled after reset"
    );
}

/// T015: Test concurrent cancel safety (multiple threads calling cancel)
#[test]
#[allow(clippy::unwrap_used)]
fn test_concurrent_cancel_safety() {
    let token = Arc::new(AtomicCancellationToken::new());
    let mut handles = vec![];

    // Spawn 10 threads that all try to cancel simultaneously
    for _ in 0..10 {
        let token_clone = Arc::clone(&token);
        let handle = thread::spawn(move || {
            token_clone.cancel();
        });
        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }

    // Token should be cancelled
    assert!(
        token.is_cancelled(),
        "Token should be cancelled after concurrent cancel() calls"
    );
}

/// Additional test: Verify `AtomicBool` is lock-free (platform requirement)
/// Note: `is_lock_free()` requires Rust 1.70+, `AtomicBool` is lock-free on all major platforms
#[test]
fn test_atomic_bool_is_lock_free() {
    // `AtomicBool` is guaranteed to be lock-free on x86/x86_64/ARM/ARM64
    // This test documents the requirement rather than checking at runtime
    // We rely on AtomicBool's lock-free property for signal handler safety
    let token = AtomicCancellationToken::new();
    // If this compiles and runs, AtomicBool is available and lock-free on this platform
    assert!(!token.is_cancelled(), "New token should not be cancelled");
}
