//! Cancellation support for compression operations
//!
//! This module provides thread-safe cancellation mechanisms for gracefully
//! stopping compression/decompression operations in response to user signals
//! (e.g., Ctrl+C) or programmatic cancellation requests.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// Thread-safe cancellation signal for compression operations.
///
/// This trait provides a lock-free way to signal cancellation to worker threads
/// and check cancellation state without blocking. Implementations must be
/// async-signal-safe (can be called from signal handlers).
///
/// # Thread Safety
///
/// All methods are safe to call concurrently from multiple threads.
/// Implementations must use atomic operations to ensure lock-freedom.
///
/// # Example
///
/// ```
/// use std::sync::Arc;
/// use crush_core::cancel::{CancellationToken, AtomicCancellationToken};
///
/// let token = Arc::new(AtomicCancellationToken::new());
/// let token_worker = Arc::clone(&token);
///
/// // Worker thread checks cancellation
/// std::thread::spawn(move || {
///     while !token_worker.is_cancelled() {
///         // Do work...
///     }
/// });
///
/// // Signal handler or main thread cancels
/// token.cancel();
/// ```
pub trait CancellationToken: Send + Sync {
    /// Check if cancellation has been requested.
    ///
    /// This method is lock-free, async-signal-safe, and very fast (<10ns).
    /// It can be called from signal handlers and hot loops without concern.
    ///
    /// # Returns
    ///
    /// `true` if cancellation has been requested, `false` otherwise.
    fn is_cancelled(&self) -> bool;

    /// Request cancellation.
    ///
    /// This method is idempotent - calling it multiple times has the same
    /// effect as calling it once. Safe to call from signal handlers.
    ///
    /// # Thread Safety
    ///
    /// Multiple threads can call this concurrently. The first call to set
    /// the flag "wins", but all calls succeed.
    fn cancel(&self);

    /// Reset the cancellation state to not-cancelled.
    ///
    /// This allows reusing the same token for multiple sequential operations.
    /// NOT async-signal-safe - do not call from signal handlers.
    fn reset(&self);
}

/// Standard implementation of `CancellationToken` using `AtomicBool`.
///
/// This is the recommended implementation for most use cases.
pub struct AtomicCancellationToken {
    cancelled: AtomicBool,
}

impl AtomicCancellationToken {
    /// Create a new cancellation token in the not-cancelled state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            cancelled: AtomicBool::new(false),
        }
    }
}

impl Default for AtomicCancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

impl CancellationToken for AtomicCancellationToken {
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    fn reset(&self) {
        self.cancelled.store(false, Ordering::SeqCst);
    }
}

/// Tracks resources created during compression/decompression for guaranteed cleanup.
///
/// Uses RAII pattern - resources are automatically cleaned up when dropped unless
/// marked as complete.
pub struct ResourceTracker {
    output_path: Mutex<Option<PathBuf>>,
    temp_files: Mutex<Vec<PathBuf>>,
    is_complete: AtomicBool,
}

impl ResourceTracker {
    /// Create a new resource tracker.
    #[must_use]
    pub fn new() -> Self {
        Self {
            output_path: Mutex::new(None),
            temp_files: Mutex::new(Vec::new()),
            is_complete: AtomicBool::new(false),
        }
    }

    /// Register output file path (to delete on cancellation).
    pub fn register_output(&self, path: PathBuf) {
        if let Ok(mut output) = self.output_path.lock() {
            *output = Some(path);
        }
    }

    /// Register temporary file (to delete always).
    pub fn register_temp_file(&self, path: PathBuf) {
        if let Ok(mut temps) = self.temp_files.lock() {
            temps.push(path);
        }
    }

    /// Mark operation as successfully completed (keep output file).
    pub fn mark_complete(&self) {
        self.is_complete.store(true, Ordering::SeqCst);
    }

    /// Clean up all tracked resources.
    ///
    /// # Errors
    ///
    /// Returns an error if file deletion fails (permissions, file in use, etc.).
    /// Returns the first error encountered if multiple deletions fail.
    pub fn cleanup_all(&self) -> std::io::Result<()> {
        // Always delete temp files
        if let Ok(temps) = self.temp_files.lock() {
            for temp in temps.iter() {
                if temp.exists() {
                    std::fs::remove_file(temp)?;
                }
            }
        }

        // Delete output file only if operation not complete
        if !self.is_complete.load(Ordering::SeqCst) {
            if let Ok(output) = self.output_path.lock() {
                if let Some(path) = output.as_ref() {
                    if path.exists() {
                        std::fs::remove_file(path)?;
                    }
                }
            }
        }

        Ok(())
    }
}

impl Default for ResourceTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ResourceTracker {
    fn drop(&mut self) {
        if !self.is_complete.load(Ordering::SeqCst) {
            // Operation did not complete - clean up everything
            // Ignore errors in Drop - best effort cleanup
            let _ = self.cleanup_all();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_token_not_cancelled() {
        let token = AtomicCancellationToken::new();
        assert!(!token.is_cancelled());
    }

    #[test]
    fn cancel_sets_flag() {
        let token = AtomicCancellationToken::new();
        token.cancel();
        assert!(token.is_cancelled());
    }

    #[test]
    fn cancel_is_idempotent() {
        let token = AtomicCancellationToken::new();
        token.cancel();
        token.cancel();
        token.cancel();
        assert!(token.is_cancelled());
    }

    #[test]
    fn reset_clears_cancellation() {
        let token = AtomicCancellationToken::new();
        token.cancel();
        assert!(token.is_cancelled());
        token.reset();
        assert!(!token.is_cancelled());
    }
}
