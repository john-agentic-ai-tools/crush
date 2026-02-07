# Contract: CancellationToken Trait

**Feature**: Graceful Cancellation Support
**Module**: `crush-core::cancel`
**Date**: 2026-02-03

## Overview

The `CancellationToken` trait provides a thread-safe, lock-free mechanism for signaling and checking cancellation state across compression/decompression operations.

---

## Trait Definition

```rust
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
/// ```rust
/// use std::sync::Arc;
/// use crush_core::cancel::{CancellationToken, AtomicCancellationToken};
///
/// let token = Arc::new(AtomicCancellationToken::new());
/// let token_worker = Arc::clone(&token);
///
/// // Worker thread checks cancellation
/// rayon::spawn(move || {
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
    ///
    /// # Performance
    ///
    /// This method performs a single atomic load with sequential consistency.
    /// Expected performance: < 10 nanoseconds on modern CPUs.
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
    ///
    /// # Async-Signal-Safety
    ///
    /// This method is async-signal-safe and can be called from SIGINT handlers.
    fn cancel(&self);

    /// Reset the cancellation state to not-cancelled.
    ///
    /// This allows reusing the same token for multiple sequential operations.
    /// NOT async-signal-safe - do not call from signal handlers.
    ///
    /// # Use Cases
    ///
    /// - CLI tools processing multiple files sequentially
    /// - Resetting state after handling a cancellation
    /// - Testing (reset between test cases)
    ///
    /// # Panics
    ///
    /// Implementations MAY panic if called while operations are still active.
    /// Callers must ensure all workers have stopped before calling reset.
    fn reset(&self);
}
```

---

## Implementation Contract

### Standard Implementation: `AtomicCancellationToken`

```rust
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Standard implementation of CancellationToken using AtomicBool.
///
/// This is the recommended implementation for most use cases.
pub struct AtomicCancellationToken {
    cancelled: AtomicBool,
    // Note: signal_received_at would require Mutex, which breaks
    // async-signal-safety. Omitted from standard implementation.
    // Consider separate metrics/debugging structure if needed.
}

impl AtomicCancellationToken {
    /// Create a new cancellation token in the not-cancelled state.
    pub fn new() -> Self {
        Self {
            cancelled: AtomicBool::new(false),
        }
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

impl Default for AtomicCancellationToken {
    fn default() -> Self {
        Self::new()
    }
}
```

---

## Memory Ordering Guarantees

### Ordering Choice: `SeqCst` (Sequential Consistency)

**Rationale**:
- Provides strongest memory ordering guarantees
- Ensures all threads see operations in the same total order
- Prevents subtle bugs from memory reordering
- Performance overhead is negligible for flag checking (~1-2ns on modern CPUs)

**Alternative Orderings Considered**:
- `Relaxed`: REJECTED - could delay cancellation visibility across threads
- `Acquire`/`Release`: ACCEPTABLE but unnecessary - `SeqCst` is clearer and safer

**Performance Note**: On x86/x86_64, `SeqCst` compiles to the same instructions as `Acquire`/`Release` for loads/stores.

---

## Usage Patterns

### Pattern 1: Single Operation

```rust
use std::sync::Arc;
use crush_core::cancel::AtomicCancellationToken;

fn compress_file(input: &Path, output: &Path) -> Result<()> {
    let cancel_token = Arc::new(AtomicCancellationToken::new());

    // Register signal handler
    let cancel_signal = Arc::clone(&cancel_token);
    ctrlc::set_handler(move || {
        cancel_signal.cancel();
    })?;

    // Perform compression
    compress_with_cancel(input, output, &cancel_token)?;

    Ok(())
}
```

### Pattern 2: Multiple Sequential Operations (with reset)

```rust
fn compress_multiple_files(files: &[PathBuf]) -> Result<()> {
    let cancel_token = Arc::new(AtomicCancellationToken::new());

    // Register signal handler once
    let cancel_signal = Arc::clone(&cancel_token);
    ctrlc::set_handler(move || {
        cancel_signal.cancel();
    })?;

    for file in files {
        if cancel_token.is_cancelled() {
            return Err(CrushError::Cancelled);
        }

        compress_with_cancel(file, &output_path(file), &cancel_token)?;

        // Reset for next file
        cancel_token.reset();
    }

    Ok(())
}
```

### Pattern 3: Rayon Parallel Workers

```rust
use rayon::prelude::*;

fn compress_blocks(
    blocks: Vec<Block>,
    cancel: Arc<dyn CancellationToken>,
) -> Result<Vec<CompressedBlock>> {
    blocks
        .into_par_iter()
        .map(|block| {
            // Check at start of each block
            if cancel.is_cancelled() {
                return Err(CrushError::Cancelled);
            }

            compress_block(&block)
        })
        .collect() // Short-circuits on first Err
}
```

---

## Error Handling

### Cancellation Error Type

```rust
#[derive(Debug, thiserror::Error)]
pub enum CrushError {
    // ... other error types ...

    /// Operation was cancelled by user (Ctrl+C)
    #[error("Operation cancelled")]
    Cancelled,
}
```

**Error Propagation**:
- Workers return `Err(CrushError::Cancelled)` when cancellation detected
- Main thread catches cancellation error and performs cleanup
- Exit with code 130 (Unix) or 2 (Windows)

---

## Testing Contract

### Required Tests for Implementations

All implementations of `CancellationToken` MUST pass these tests:

```rust
#[test]
fn new_token_not_cancelled() -> Result<()> {
    let token = AtomicCancellationToken::new();
    assert!(!token.is_cancelled());
    Ok(())
}

#[test]
fn cancel_sets_flag() -> Result<()> {
    let token = AtomicCancellationToken::new();
    token.cancel();
    assert!(token.is_cancelled());
    Ok(())
}

#[test]
fn cancel_is_idempotent() -> Result<()> {
    let token = AtomicCancellationToken::new();
    token.cancel();
    token.cancel();
    token.cancel();
    assert!(token.is_cancelled());
    Ok(())
}

#[test]
fn reset_clears_cancellation() -> Result<()> {
    let token = AtomicCancellationToken::new();
    token.cancel();
    assert!(token.is_cancelled());
    token.reset();
    assert!(!token.is_cancelled());
    Ok(())
}

#[test]
fn concurrent_cancel_safe() -> Result<()> {
    use std::sync::Arc;
    use std::thread;

    let token = Arc::new(AtomicCancellationToken::new());
    let handles: Vec<_> = (0..10)
        .map(|_| {
            let t = Arc::clone(&token);
            thread::spawn(move || {
                t.cancel();
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }

    assert!(token.is_cancelled());
    Ok(())
}

#[test]
fn check_is_lock_free() {
    // Platform-specific test - AtomicBool is lock-free on all major platforms
    assert!(AtomicBool::is_lock_free());
}
```

---

## Performance Requirements

### Benchmarks

Implementations MUST meet these performance targets:

- `is_cancelled()`: < 10 nanoseconds per call
- `cancel()`: < 50 nanoseconds per call
- `reset()`: < 50 nanoseconds per call
- Zero allocations in hot path (`is_cancelled`, `cancel`)

### Benchmark Implementation

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_is_cancelled(c: &mut Criterion) {
    let token = AtomicCancellationToken::new();

    c.bench_function("is_cancelled", |b| {
        b.iter(|| {
            black_box(token.is_cancelled());
        });
    });
}

criterion_group!(benches, bench_is_cancelled);
criterion_main!(benches);
```

---

## Compatibility

### Minimum Rust Version

Requires Rust 1.70+ for:
- `std::sync::atomic::AtomicBool::is_lock_free()` (stabilized in 1.70)
- Trait object safety improvements

### Platform Support

- **Linux**: Full support (SIGINT handling)
- **macOS**: Full support (SIGINT handling)
- **Windows**: Full support (Ctrl+C via `ctrlc` crate)
- **BSD**: Expected to work (SIGINT similar to Linux)

---

**Contract Status**: ✅ Defined
**Implementation**: Required in `crush-core/src/cancel.rs`
