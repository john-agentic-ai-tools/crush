//! Timeout protection for plugin operations
//!
//! Implements thread-based timeout enforcement with cooperative cancellation.
//! Uses crossbeam channels for reliable timeout detection and `Arc<AtomicBool>`
//! for cooperative cancellation within plugins.

use crate::cancel::CancellationToken;
use crate::error::{Result, TimeoutError};
use crossbeam::channel;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
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
/// * `timeout` - Maximum duration to wait for operation completion (0 = no timeout)
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
/// - Plugin thread panics during execution
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

    // Wait for completion or timeout.
    //
    // Bound to a local rather than matched inline: as the function's tail
    // expression, a `match` scrutinee temporary would drop after locals in
    // Rust 2021 but before them in Rust 2024. Naming it pins the drop order
    // across editions instead of leaving it to `tail-expr-drop-order`.
    let received = rx.recv_timeout(effective_timeout);
    match received {
        Ok(result) => result,
        Err(channel::RecvTimeoutError::Timeout) => {
            // Signal the worker so a cooperative plugin stops promptly. The
            // thread is detached, so without this a timed-out operation runs to
            // completion burning CPU for a result nobody reads. The three sibling
            // variants already did this; this one was the odd one out.
            cancel_flag.store(true, Ordering::Release);
            eprintln!("Warning: Plugin operation timed out after {timeout:?}");
            Err(TimeoutError::Timeout(timeout).into())
        }
        Err(channel::RecvTimeoutError::Disconnected) => {
            eprintln!("Warning: Plugin thread panicked during execution");
            Err(TimeoutError::PluginPanic.into())
        }
    }
}

/// Run an operation with timeout protection and external cancellation support
///
/// This version supports both timeout-based cancellation and external cancellation
/// via a `CancellationToken` (e.g., for Ctrl+C handling).
///
/// # Arguments
///
/// * `timeout` - Maximum duration to wait for operation completion (0 = no timeout)
/// * `cancel_token` - Optional external cancellation token
/// * `operation` - The operation to run (receives cancellation flag)
///
/// # Returns
///
/// The operation's result if it completes, otherwise a timeout or cancellation error
///
/// # Errors
///
/// Returns an error if:
/// - Operation times out
/// - External cancellation is triggered
/// - Plugin thread panics during execution
/// - Operation returns an error
pub fn run_with_timeout_and_cancel<F, T>(
    timeout: Duration,
    cancel_token: Option<Arc<dyn CancellationToken>>,
    operation: F,
) -> Result<T>
where
    F: FnOnce(Arc<AtomicBool>) -> Result<T> + Send + 'static,
    T: Send + 'static,
{
    // Check if already cancelled before starting
    if let Some(ref token) = cancel_token
        && token.is_cancelled()
    {
        return Err(crate::error::CrushError::Cancelled);
    }

    // Timeout of 0 means no timeout - use Duration::MAX for effectively infinite wait
    let effective_timeout = if timeout == Duration::from_secs(0) {
        Duration::MAX
    } else {
        timeout
    };

    let cancel_flag = Arc::new(AtomicBool::new(false));
    let cancel_flag_thread = Arc::clone(&cancel_flag);
    let cancel_flag_guard = Arc::clone(&cancel_flag);
    let cancel_flag_monitor = Arc::clone(&cancel_flag);

    // Spawn a monitor thread for external cancellation token
    let monitor_handle = if let Some(token) = cancel_token {
        let handle = std::thread::spawn(move || {
            // Poll the external token very frequently for responsive cancellation
            while !cancel_flag_monitor.load(Ordering::Acquire) {
                if token.is_cancelled() {
                    // External cancellation requested - signal the plugin
                    cancel_flag_monitor.store(true, Ordering::Release);
                    break;
                }
                // Use a very short sleep for fast response
                std::thread::sleep(Duration::from_micros(100));
            }
        });
        Some(handle)
    } else {
        None
    };

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
    let result = match rx.recv_timeout(effective_timeout) {
        Ok(result) => {
            // Convert PluginError::Cancelled to CrushError::Cancelled
            match result {
                Err(crate::error::CrushError::Plugin(crate::error::PluginError::Cancelled)) => {
                    Err(crate::error::CrushError::Cancelled)
                }
                other => other,
            }
        }
        Err(channel::RecvTimeoutError::Timeout) => {
            // Signal cancellation to the operation
            cancel_flag.store(true, Ordering::Release);
            eprintln!("Warning: Plugin operation timed out after {timeout:?}");
            Err(TimeoutError::Timeout(timeout).into())
        }
        Err(channel::RecvTimeoutError::Disconnected) => {
            eprintln!("Warning: Plugin thread panicked during execution");
            Err(TimeoutError::PluginPanic.into())
        }
    };

    // Stop the monitor thread if it's still running
    cancel_flag.store(true, Ordering::Release);
    if let Some(handle) = monitor_handle {
        let _ = handle.join(); // Wait for monitor to finish
    }

    result
}

/// Scoped variant of [`run_with_timeout`] — operation may borrow non-`'static` data.
///
/// Uses `std::thread::scope` so the closure runs on a thread whose lifetime is bounded
/// by `'scope`. This lets hot-path callers pass `&[u8]` directly instead of allocating
/// an owned clone just to satisfy the `'static` bound of [`run_with_timeout`].
///
/// Semantics (timeout, panic, 0 = infinite) are identical to [`run_with_timeout`].
///
/// Note: kept `pub(crate)` to preserve the strict zero-diff public API contract
/// (SC-007). See specs/011-perf-optimizations/contracts/public-api.md.
///
/// # Errors
///
/// Same as [`run_with_timeout`].
pub(crate) fn run_with_timeout_scoped<'scope, 'env, F, T>(
    scope: &'scope std::thread::Scope<'scope, 'env>,
    timeout: Duration,
    operation: F,
) -> Result<T>
where
    F: FnOnce(Arc<AtomicBool>) -> Result<T> + Send + 'scope,
    T: Send + 'scope,
{
    let effective_timeout = if timeout == Duration::from_secs(0) {
        Duration::MAX
    } else {
        timeout
    };

    let cancel_flag = Arc::new(AtomicBool::new(false));
    let cancel_flag_thread = Arc::clone(&cancel_flag);
    let cancel_flag_guard = Arc::clone(&cancel_flag);

    let (tx, rx) = channel::bounded(1);

    scope.spawn(move || {
        let _guard = TimeoutGuard {
            cancel_flag: cancel_flag_guard,
        };
        let result = operation(cancel_flag_thread);
        let _ = tx.send(result);
    });

    // Bound to a local for the same cross-edition drop-order reason as in
    // `run_with_timeout` above.
    let received = rx.recv_timeout(effective_timeout);
    match received {
        Ok(result) => result,
        Err(channel::RecvTimeoutError::Timeout) => {
            cancel_flag.store(true, Ordering::Release);
            eprintln!("Warning: Plugin operation timed out after {timeout:?}");
            Err(TimeoutError::Timeout(timeout).into())
        }
        Err(channel::RecvTimeoutError::Disconnected) => {
            eprintln!("Warning: Plugin thread panicked during execution");
            Err(TimeoutError::PluginPanic.into())
        }
    }
}

/// Scoped variant of [`run_with_timeout_and_cancel`] — operation may borrow non-`'static` data.
///
/// See [`run_with_timeout_scoped`] for the motivation. Adds the same external-cancellation
/// monitor thread as [`run_with_timeout_and_cancel`].
///
/// # Errors
///
/// Same as [`run_with_timeout_and_cancel`].
pub(crate) fn run_with_timeout_and_cancel_scoped<'scope, 'env, F, T>(
    scope: &'scope std::thread::Scope<'scope, 'env>,
    timeout: Duration,
    cancel_token: Option<Arc<dyn CancellationToken>>,
    operation: F,
) -> Result<T>
where
    F: FnOnce(Arc<AtomicBool>) -> Result<T> + Send + 'scope,
    T: Send + 'scope,
{
    if let Some(ref token) = cancel_token
        && token.is_cancelled()
    {
        return Err(crate::error::CrushError::Cancelled);
    }

    let effective_timeout = if timeout == Duration::from_secs(0) {
        Duration::MAX
    } else {
        timeout
    };

    let cancel_flag = Arc::new(AtomicBool::new(false));
    let cancel_flag_thread = Arc::clone(&cancel_flag);
    let cancel_flag_guard = Arc::clone(&cancel_flag);
    let cancel_flag_monitor = Arc::clone(&cancel_flag);

    let monitor_handle = cancel_token.map(|token| {
        scope.spawn(move || {
            while !cancel_flag_monitor.load(Ordering::Acquire) {
                if token.is_cancelled() {
                    cancel_flag_monitor.store(true, Ordering::Release);
                    break;
                }
                std::thread::sleep(Duration::from_micros(100));
            }
        })
    });

    let (tx, rx) = channel::bounded(1);

    scope.spawn(move || {
        let _guard = TimeoutGuard {
            cancel_flag: cancel_flag_guard,
        };
        let result = operation(cancel_flag_thread);
        let _ = tx.send(result);
    });

    let result = match rx.recv_timeout(effective_timeout) {
        Ok(result) => match result {
            Err(crate::error::CrushError::Plugin(crate::error::PluginError::Cancelled)) => {
                Err(crate::error::CrushError::Cancelled)
            }
            other => other,
        },
        Err(channel::RecvTimeoutError::Timeout) => {
            cancel_flag.store(true, Ordering::Release);
            eprintln!("Warning: Plugin operation timed out after {timeout:?}");
            Err(TimeoutError::Timeout(timeout).into())
        }
        Err(channel::RecvTimeoutError::Disconnected) => {
            eprintln!("Warning: Plugin thread panicked during execution");
            Err(TimeoutError::PluginPanic.into())
        }
    };

    cancel_flag.store(true, Ordering::Release);
    if let Some(handle) = monitor_handle {
        let _ = handle.join();
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::PluginError;

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_operation_completes_within_timeout() {
        let timeout = Duration::from_secs(1);

        let result = run_with_timeout(timeout, |_cancel| {
            // Fast operation
            Ok(42)
        });

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_operation_respects_cancellation() {
        let timeout = Duration::from_millis(50);

        let result = run_with_timeout(timeout, |cancel_flag| {
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

        let result = run_with_timeout(timeout, |_cancel| Ok(42));

        // Zero timeout means no timeout - operation should succeed
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_timeout_guard_sets_flag_on_drop() {
        let cancel_flag = Arc::new(AtomicBool::new(false));
        {
            let _guard = TimeoutGuard {
                cancel_flag: Arc::clone(&cancel_flag),
            };
            assert!(!cancel_flag.load(Ordering::Acquire));
        }
        // Flag should be set after guard is dropped
        assert!(cancel_flag.load(Ordering::Acquire));
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_run_with_timeout_basic_success() {
        let timeout = Duration::from_secs(1);

        let result = run_with_timeout(timeout, |_cancel| Ok(100));

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 100);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_run_with_timeout_operation_error() {
        let timeout = Duration::from_secs(1);

        let result: Result<i32> = run_with_timeout(timeout, |_cancel| {
            Err(PluginError::OperationFailed("test error".to_string()).into())
        });

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("test error"));
    }

    #[test]
    fn test_timeout_error_display() {
        let timeout_err = TimeoutError::Timeout(Duration::from_secs(30));
        assert!(timeout_err.to_string().contains("30"));

        let panic_err = TimeoutError::PluginPanic;
        assert!(panic_err.to_string().contains("panicked"));
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_run_with_timeout_error_propagation() {
        let timeout = Duration::from_secs(1);

        let result: Result<i32> = run_with_timeout(timeout, |_cancel| {
            Err(PluginError::OperationFailed("custom error".to_string()).into())
        });

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("custom error"));
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_effective_timeout_conversion() {
        // Test that 0 timeout becomes Duration::MAX internally
        let timeout = Duration::from_secs(0);

        // This should complete successfully even with "infinite" effective timeout
        let result = run_with_timeout(timeout, |_cancel| {
            std::thread::sleep(Duration::from_millis(10));
            Ok("done".to_string())
        });

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "done");
    }

    // -----------------------------------------------------------------------
    // Timeout expiry, panic propagation, external cancellation, scoped variants
    // -----------------------------------------------------------------------

    use crate::cancel::AtomicCancellationToken;
    use crate::error::CrushError;

    /// An operation that ignores its cancel flag and outlives any short timeout.
    fn sleep_ignoring_cancellation(d: Duration) -> impl FnOnce(Arc<AtomicBool>) -> Result<u32> {
        move |_cancel| {
            std::thread::sleep(d);
            Ok(1)
        }
    }

    #[test]
    fn run_with_timeout_reports_timeout_when_the_deadline_passes() {
        let result = run_with_timeout(
            Duration::from_millis(50),
            sleep_ignoring_cancellation(Duration::from_secs(5)),
        );

        assert!(
            matches!(
                result,
                Err(CrushError::Timeout(TimeoutError::Timeout(d))) if d == Duration::from_millis(50)
            ),
            "expected Timeout(50ms), got {result:?}"
        );
    }

    #[test]
    #[allow(clippy::panic)] // the panic is the behaviour under test
    fn a_panicking_operation_is_reported_as_plugin_panic() {
        // The worker drops `tx` while unwinding, so the receiver observes
        // Disconnected rather than hanging until the timeout elapses.
        let result = run_with_timeout(Duration::from_secs(30), |_cancel| -> Result<u32> {
            panic!("plugin exploded");
        });

        assert!(
            matches!(result, Err(CrushError::Timeout(TimeoutError::PluginPanic))),
            "expected PluginPanic, got {result:?}"
        );
    }

    #[test]
    fn timeout_sets_the_cancel_flag_so_the_worker_can_stop() {
        let observed = Arc::new(AtomicBool::new(false));
        let probe = Arc::clone(&observed);

        let result = run_with_timeout(Duration::from_millis(50), move |cancel| -> Result<u32> {
            // Wait until the timeout path signals us, then report that we saw it.
            for _ in 0..2_000 {
                if cancel.load(Ordering::Acquire) {
                    probe.store(true, Ordering::Release);
                    return Ok(0);
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            Ok(0)
        });

        assert!(result.is_err(), "expected the call to time out");
        // Give the worker a moment to notice the flag it was handed.
        std::thread::sleep(Duration::from_millis(200));
        assert!(
            observed.load(Ordering::Acquire),
            "worker should have observed the cancel flag set by the timeout path"
        );
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn cancel_variant_succeeds_with_no_token() {
        let result = run_with_timeout_and_cancel(Duration::from_secs(5), None, |_c| Ok(7));
        assert_eq!(result.unwrap(), 7);
    }

    #[test]
    fn cancel_variant_short_circuits_on_an_already_cancelled_token() {
        let token = Arc::new(AtomicCancellationToken::new());
        token.cancel();

        let ran = Arc::new(AtomicBool::new(false));
        let probe = Arc::clone(&ran);

        let result = run_with_timeout_and_cancel(Duration::from_secs(5), Some(token), move |_c| {
            probe.store(true, Ordering::Release);
            Ok(0u32)
        });

        assert!(matches!(result, Err(CrushError::Cancelled)));
        assert!(
            !ran.load(Ordering::Acquire),
            "operation must not start when the token is already cancelled"
        );
    }

    #[test]
    fn cancel_variant_propagates_external_cancellation_mid_flight() {
        let token = Arc::new(AtomicCancellationToken::new());
        let trigger = Arc::clone(&token);

        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            trigger.cancel();
        });

        // The monitor thread flips the plugin's cancel flag; the plugin reports
        // PluginError::Cancelled, which must surface as CrushError::Cancelled.
        let result = run_with_timeout_and_cancel(Duration::from_secs(30), Some(token), |cancel| {
            for _ in 0..3_000 {
                if cancel.load(Ordering::Acquire) {
                    return Err(PluginError::Cancelled.into());
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            Ok(0u32)
        });

        assert!(
            matches!(result, Err(CrushError::Cancelled)),
            "expected Cancelled, got {result:?}"
        );
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn scoped_variant_can_borrow_non_static_data() {
        // The whole reason the scoped variants exist: no 'static bound, so the
        // closure can borrow a local instead of cloning it onto the heap.
        let owned = [1u8, 2, 3, 4];

        let total = std::thread::scope(|s| {
            run_with_timeout_scoped(s, Duration::from_secs(5), |_cancel| {
                Ok(owned.iter().map(|b| u32::from(*b)).sum::<u32>())
            })
        })
        .unwrap();

        assert_eq!(total, 10);
        // `owned` is still usable — it was borrowed, not moved.
        assert_eq!(owned.len(), 4);
    }

    #[test]
    fn scoped_variant_times_out() {
        let result = std::thread::scope(|s| {
            run_with_timeout_scoped(
                s,
                Duration::from_millis(50),
                sleep_ignoring_cancellation(Duration::from_millis(600)),
            )
        });

        assert!(
            matches!(result, Err(CrushError::Timeout(TimeoutError::Timeout(_)))),
            "expected Timeout, got {result:?}"
        );
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn scoped_cancel_variant_succeeds_with_no_token() {
        let data = [10u32, 20, 30];
        let sum = std::thread::scope(|s| {
            run_with_timeout_and_cancel_scoped(s, Duration::from_secs(5), None, |_c| {
                Ok(data.iter().sum::<u32>())
            })
        })
        .unwrap();
        assert_eq!(sum, 60);
    }

    #[test]
    fn scoped_cancel_variant_short_circuits_on_a_cancelled_token() {
        let token = Arc::new(AtomicCancellationToken::new());
        token.cancel();

        let result = std::thread::scope(|s| {
            run_with_timeout_and_cancel_scoped(s, Duration::from_secs(5), Some(token), |_c| {
                Ok(0u32)
            })
        });

        assert!(matches!(result, Err(CrushError::Cancelled)));
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn zero_timeout_is_treated_as_no_deadline_in_every_variant() {
        // Duration::ZERO maps to Duration::MAX internally rather than expiring
        // immediately; verify that for each entry point.
        assert_eq!(run_with_timeout(Duration::ZERO, |_c| Ok(1u32)).unwrap(), 1);
        assert_eq!(
            run_with_timeout_and_cancel(Duration::ZERO, None, |_c| Ok(2u32)).unwrap(),
            2
        );
        assert_eq!(
            std::thread::scope(|s| run_with_timeout_scoped(s, Duration::ZERO, |_c| Ok(3u32)))
                .unwrap(),
            3
        );
        assert_eq!(
            std::thread::scope(|s| run_with_timeout_and_cancel_scoped(
                s,
                Duration::ZERO,
                None,
                |_c| Ok(4u32)
            ))
            .unwrap(),
            4
        );
    }
}
