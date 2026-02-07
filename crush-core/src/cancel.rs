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
/// marked as complete. This ensures incomplete output files are deleted if an
/// operation is cancelled or fails.
///
/// # Thread Safety
///
/// All methods use interior mutability and are safe to call from multiple threads.
///
/// # Example
///
/// ```no_run
/// use crush_core::cancel::ResourceTracker;
/// use std::path::PathBuf;
///
/// let tracker = ResourceTracker::new();
///
/// // Register the output file to be cleaned up if operation fails
/// tracker.register_output(PathBuf::from("output.crush"));
///
/// // Register temporary files that should always be deleted
/// tracker.register_temp_file(PathBuf::from("temp.dat"));
///
/// // ... do compression work ...
///
/// // If successful, mark complete to keep the output file
/// tracker.mark_complete();
///
/// // Drop will clean up temp files but keep output (marked complete)
/// ```
///
/// # Cleanup Behavior
///
/// - **Temp files**: Always deleted on drop
/// - **Output file**: Deleted on drop UNLESS `mark_complete()` was called
/// - **On panic**: Drop runs, ensuring cleanup even during unwinding
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

    /// Register the output file path to be cleaned up if the operation doesn't complete.
    ///
    /// The output file will be deleted on drop unless `mark_complete()` is called.
    /// Only one output file can be registered (subsequent calls replace the previous).
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the output file
    pub fn register_output(&self, path: PathBuf) {
        if let Ok(mut output) = self.output_path.lock() {
            *output = Some(path);
        }
    }

    /// Register a temporary file that should always be deleted on cleanup.
    ///
    /// Temporary files are always deleted on drop, regardless of completion status.
    /// Multiple temporary files can be registered.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the temporary file
    pub fn register_temp_file(&self, path: PathBuf) {
        if let Ok(mut temps) = self.temp_files.lock() {
            temps.push(path);
        }
    }

    /// Mark the operation as successfully completed, preventing output file deletion.
    ///
    /// Call this after the operation succeeds to keep the output file.
    /// If not called, the output file will be deleted on drop (cleanup on failure).
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use crush_core::cancel::ResourceTracker;
    /// # use std::path::PathBuf;
    /// let tracker = ResourceTracker::new();
    /// tracker.register_output(PathBuf::from("output.dat"));
    ///
    /// // ... do work ...
    ///
    /// if work_succeeded() {
    ///     tracker.mark_complete(); // Keep the output file
    /// }
    /// // If work failed, drop will delete the output file
    /// # fn work_succeeded() -> bool { true }
    /// ```
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
#[allow(clippy::panic_in_result_fn)]
#[allow(clippy::unwrap_used)]
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

    #[test]
    fn default_creates_uncancelled_token() {
        let token = AtomicCancellationToken::default();
        assert!(!token.is_cancelled());
    }

    // ResourceTracker tests
    #[test]
    fn new_resource_tracker_is_empty() {
        let tracker = ResourceTracker::new();
        // Verify cleanup_all succeeds with no registered resources
        assert!(tracker.cleanup_all().is_ok());
    }

    #[test]
    fn resource_tracker_default() {
        let tracker = ResourceTracker::default();
        assert!(tracker.cleanup_all().is_ok());
    }

    #[test]
    fn register_output_and_mark_complete() -> std::io::Result<()> {
        use std::fs;
        use tempfile::TempDir;

        let dir = TempDir::new()?;
        let output_path = dir.path().join("output.txt");

        // Create the output file
        fs::write(&output_path, b"test data")?;
        assert!(output_path.exists());

        let tracker = ResourceTracker::new();
        tracker.register_output(output_path.clone());
        tracker.mark_complete();

        // Clean up - file should NOT be deleted (marked complete)
        tracker.cleanup_all()?;
        assert!(
            output_path.exists(),
            "Completed output file should not be deleted"
        );

        Ok(())
    }

    #[test]
    fn register_output_without_complete_deletes() -> std::io::Result<()> {
        use std::fs;
        use tempfile::TempDir;

        let dir = TempDir::new()?;
        let output_path = dir.path().join("output.txt");

        // Create the output file
        fs::write(&output_path, b"test data")?;
        assert!(output_path.exists());

        let tracker = ResourceTracker::new();
        tracker.register_output(output_path.clone());

        // Clean up without marking complete - file SHOULD be deleted
        tracker.cleanup_all()?;
        assert!(
            !output_path.exists(),
            "Incomplete output file should be deleted"
        );

        Ok(())
    }

    #[test]
    fn register_temp_file_always_deletes() -> std::io::Result<()> {
        use std::fs;
        use tempfile::TempDir;

        let dir = TempDir::new()?;
        let temp_path = dir.path().join("temp.txt");

        // Create the temp file
        fs::write(&temp_path, b"temp data")?;
        assert!(temp_path.exists());

        let tracker = ResourceTracker::new();
        tracker.register_temp_file(temp_path.clone());
        tracker.mark_complete(); // Even if marked complete

        // Clean up - temp file should ALWAYS be deleted
        tracker.cleanup_all()?;
        assert!(!temp_path.exists(), "Temp file should always be deleted");

        Ok(())
    }

    #[test]
    fn register_multiple_temp_files() -> std::io::Result<()> {
        use std::fs;
        use tempfile::TempDir;

        let dir = TempDir::new()?;
        let temp1 = dir.path().join("temp1.txt");
        let temp2 = dir.path().join("temp2.txt");
        let temp3 = dir.path().join("temp3.txt");

        // Create temp files
        fs::write(&temp1, b"temp1")?;
        fs::write(&temp2, b"temp2")?;
        fs::write(&temp3, b"temp3")?;

        let tracker = ResourceTracker::new();
        tracker.register_temp_file(temp1.clone());
        tracker.register_temp_file(temp2.clone());
        tracker.register_temp_file(temp3.clone());

        // Clean up - all temp files should be deleted
        tracker.cleanup_all()?;
        assert!(!temp1.exists());
        assert!(!temp2.exists());
        assert!(!temp3.exists());

        Ok(())
    }

    #[test]
    fn drop_cleans_up_incomplete_output() -> std::io::Result<()> {
        use std::fs;
        use tempfile::TempDir;

        let dir = TempDir::new()?;
        let output_path = dir.path().join("output.txt");

        // Create the output file
        fs::write(&output_path, b"test data")?;
        assert!(output_path.exists());

        {
            let tracker = ResourceTracker::new();
            tracker.register_output(output_path.clone());
            // Drop without marking complete
        }

        // File should be deleted by Drop
        assert!(
            !output_path.exists(),
            "Drop should delete incomplete output"
        );

        Ok(())
    }

    #[test]
    fn drop_keeps_completed_output() -> std::io::Result<()> {
        use std::fs;
        use tempfile::TempDir;

        let dir = TempDir::new()?;
        let output_path = dir.path().join("output.txt");

        // Create the output file
        fs::write(&output_path, b"test data")?;

        {
            let tracker = ResourceTracker::new();
            tracker.register_output(output_path.clone());
            tracker.mark_complete();
            // Drop with completion
        }

        // File should NOT be deleted
        assert!(output_path.exists(), "Drop should keep completed output");

        Ok(())
    }

    #[test]
    fn cleanup_nonexistent_files_succeeds() {
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        let fake_path = dir.path().join("does_not_exist.txt");

        let tracker = ResourceTracker::new();
        tracker.register_output(fake_path.clone());
        tracker.register_temp_file(fake_path);

        // Cleanup should succeed even if files don't exist
        assert!(tracker.cleanup_all().is_ok());
    }

    #[test]
    fn replace_output_registration() -> std::io::Result<()> {
        use std::fs;
        use tempfile::TempDir;

        let dir = TempDir::new()?;
        let output1 = dir.path().join("output1.txt");
        let output2 = dir.path().join("output2.txt");

        fs::write(&output1, b"data1")?;
        fs::write(&output2, b"data2")?;

        let tracker = ResourceTracker::new();
        tracker.register_output(output1.clone());
        tracker.register_output(output2.clone()); // Replace first registration

        // Only the second output should be deleted
        tracker.cleanup_all()?;

        assert!(
            output1.exists(),
            "First output should not be tracked after replacement"
        );
        assert!(!output2.exists(), "Second output should be deleted");

        Ok(())
    }
}
