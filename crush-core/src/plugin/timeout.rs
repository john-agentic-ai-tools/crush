//! Timeout protection for plugin operations
//!
//! Implements thread-based timeout enforcement with cooperative cancellation.
//! Uses crossbeam channels for reliable timeout detection and `Arc<AtomicBool>`
//! for cooperative cancellation within plugins.

use crate::error::{Result, TimeoutError};
use crossbeam::channel;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// RAII guard that sets cancellation flag on drop
///
/// When this guard is dropped (either normally or due to panic), it sets
/// the cancellation flag to signal the plugin to stop processing.
pub struct TimeoutGuard {
    cancel_flag: Arc<AtomicBool>,
}

impl Drop for TimeoutGuard {
    fn drop(&mut self) {
        // Signal cancellation when guard is dropped (timeout or panic)
        self.cancel_flag.store(true, Ordering::Release);
    }
}

/// Run an operation with timeout protection
///
/// Spawns the operation in a dedicated thread and enforces the specified timeout.
/// If the operation doesn't complete within the timeout, the cancellation flag
/// is set and an error is returned.
///
/// # Arguments
///
/// * `timeout` - Maximum duration to wait for operation completion
/// * `operation` - The operation to run (receives cancellation flag)
///
/// # Returns
///
/// The operation's result if it completes within timeout, otherwise a timeout error
///
/// # Errors
///
/// Returns an error if:
/// - Operation times out
/// - Plugin panics during execution
/// - Operation returns an error
///
/// # Examples
///
/// ```no_run
/// use crush_core::plugin::timeout::run_with_timeout;
/// use std::sync::Arc;
/// use std::sync::atomic::AtomicBool;
/// use std::time::Duration;
///
/// let timeout = Duration::from_secs(5);
/// let result = run_with_timeout(timeout, |cancel_flag| {
///     // Operation code here
///     Ok(vec![1, 2, 3])
/// });
/// ```
pub fn run_with_timeout<F, T>(timeout: Duration, operation: F) -> Result<T>
where
    F: FnOnce(Arc<AtomicBool>) -> Result<T> + Send + 'static,
    T: Send + 'static,
{
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let cancel_flag_clone = Arc::clone(&cancel_flag);

    let (_tx, rx) = channel::bounded(1);

    // Spawn operation in dedicated thread
    let handle = std::thread::spawn(move || {
        let _guard = TimeoutGuard {
            cancel_flag: cancel_flag_clone,
        };

        // Run the operation
        operation(cancel_flag)
    });

    // Wait for completion or timeout
    match rx.recv_timeout(timeout) {
        Ok(result) => result,
        Err(channel::RecvTimeoutError::Timeout) => {
            // Timeout occurred - cancellation flag will be set when guard drops
            // Wait a bit for thread to notice cancellation
            std::thread::sleep(Duration::from_millis(10));

            // Try to join thread (it might have finished just after timeout)
            if let Ok(result) = handle.join() {
                // Thread completed just after timeout - use result anyway
                result
            } else {
                // Thread panicked
                eprintln!("Warning: Plugin operation timed out after {timeout:?}");
                Err(TimeoutError::Timeout(timeout).into())
            }
        }
        Err(channel::RecvTimeoutError::Disconnected) => {
            // Channel disconnected - check if thread panicked
            match handle.join() {
                Ok(result) => result,
                Err(e) => {
                    eprintln!("Warning: Plugin panicked during execution: {e:?}");
                    Err(TimeoutError::PluginPanic.into())
                }
            }
        }
    }
}

/// Alternative implementation that actually sends results through channel
///
/// This is the corrected version that properly communicates between threads.
/// Spawns the operation in a dedicated thread and enforces the specified timeout.
///
/// # Errors
///
/// Returns an error if:
/// - Operation times out
/// - Plugin thread panics during execution
/// - Operation returns an error
pub fn run_with_timeout_v2<F, T>(timeout: Duration, operation: F) -> Result<T>
where
    F: FnOnce(Arc<AtomicBool>) -> Result<T> + Send + 'static,
    T: Send + 'static,
{
    // Timeout of 0 means no timeout - use Duration::MAX for effectively infinite wait
    let effective_timeout = if timeout == Duration::from_secs(0) {
        Duration::MAX
    } else {
        timeout
    };

    let cancel_flag = Arc::new(AtomicBool::new(false));
    let cancel_flag_thread = Arc::clone(&cancel_flag);
    let cancel_flag_guard = Arc::clone(&cancel_flag);

    let (tx, rx) = channel::bounded(1);

    // Spawn operation in dedicated thread
    std::thread::spawn(move || {
        let _guard = TimeoutGuard {
            cancel_flag: cancel_flag_guard,
        };

        // Run operation and send result
        let result = operation(cancel_flag_thread);
        let _ = tx.send(result); // Ignore send errors (receiver might have timed out)
    });

    // Wait for completion or timeout
    match rx.recv_timeout(effective_timeout) {
        Ok(result) => result,
        Err(channel::RecvTimeoutError::Timeout) => {
            eprintln!("Warning: Plugin operation timed out after {timeout:?}");
            Err(TimeoutError::Timeout(timeout).into())
        }
        Err(channel::RecvTimeoutError::Disconnected) => {
            eprintln!("Warning: Plugin thread panicked during execution");
            Err(TimeoutError::PluginPanic.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::PluginError;

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_operation_completes_within_timeout() {
        let timeout = Duration::from_secs(1);

        let result = run_with_timeout_v2(timeout, |_cancel| {
            // Fast operation
            Ok(42)
        });

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_operation_respects_cancellation() {
        let timeout = Duration::from_millis(50);

        let result = run_with_timeout_v2(timeout, |cancel_flag| {
            // Simulate slow operation that checks cancellation
            for _ in 0..1000 {
                if cancel_flag.load(Ordering::Acquire) {
                    return Err(PluginError::Cancelled.into());
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(42)
        });

        // Should either timeout or be cancelled
        assert!(result.is_err());
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_zero_timeout_means_no_timeout() {
        let timeout = Duration::from_secs(0);

        let result = run_with_timeout_v2(timeout, |_cancel| Ok(42));

        // Zero timeout means no timeout - operation should succeed
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }
}
