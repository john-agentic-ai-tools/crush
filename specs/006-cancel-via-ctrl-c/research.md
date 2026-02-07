# Research Document: Graceful Cancellation in Rust CLI Applications

**Feature**: Graceful Cancellation Support
**Branch**: 006-cancel-via-ctrl-c
**Date**: 2026-02-03

This document contains research findings for implementing graceful cancellation support in the Crush compression CLI application.

---

## 1. Cross-Platform Signal Handling

### Decision: Use `ctrlc` Crate

**Chosen Approach**: Implement signal handling using the `ctrlc` crate with `Arc<AtomicBool>` for the cancellation flag.

**Rationale**:
- Cross-platform compatibility: Works seamlessly on both Windows and Unix systems
- Simplicity: Focused specifically on SIGINT/Ctrl+C handling
- Community support: Well-maintained with wide adoption in CLI tools
- Minimal complexity: Doesn't introduce unnecessary features

**Implementation Pattern**:
```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

fn setup_signal_handler() -> Arc<AtomicBool> {
    let cancelled = Arc::new(AtomicBool::new(false));
    let cancelled_clone = Arc::clone(&cancelled);

    ctrlc::set_handler(move || {
        cancelled_clone.store(true, Ordering::SeqCst);
    }).expect("Error setting Ctrl-C handler");

    cancelled
}
```

**Platform Differences**:
- **Unix**: Maps to SIGINT (signal 2). Handlers are overwritten when registered.
- **Windows**: Maps to CTRL_C_EVENT or CTRL_BREAK_EVENT. Multiple handlers allowed.

**Thread Safety**: The `ctrlc` crate starts a dedicated signal handling thread where the handler executes.

**Alternatives Considered**:
- `signal-hook` crate: Rejected due to unnecessary complexity for single-signal use case
- Manual platform-specific code: Rejected due to increased maintenance burden

---

## 2. Atomic Flag Patterns for Multi-threaded Cancellation

### Decision: `Arc<AtomicBool>` with Sequential Consistency

**Chosen Approach**: Use `Arc<AtomicBool>` with `Ordering::SeqCst` for the cancellation flag.

**Rationale**:
- Thread safety: `AtomicBool` provides lock-free, thread-safe boolean operations
- Shared ownership: `Arc` enables sharing across threads without lifetime issues
- Memory ordering: `SeqCst` provides strongest guarantees, preventing reordering
- Zero-cost in practice: Modern CPUs make `SeqCst` very cheap for simple flags

**Implementation Pattern**:
```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

// Create the cancellation flag
let cancelled = Arc::new(AtomicBool::new(false));

// Clone for signal handler
let cancelled_signal = Arc::clone(&cancelled);
ctrlc::set_handler(move || {
    cancelled_signal.store(true, Ordering::SeqCst);
}).expect("Error setting Ctrl-C handler");

// Clone for worker threads
let cancelled_worker = Arc::clone(&cancelled);
rayon::spawn(move || {
    while !cancelled_worker.load(Ordering::SeqCst) {
        // Do work...
    }
});
```

**Memory Ordering Choice**:
- `Ordering::SeqCst`: Sequential consistency - strongest guarantee
- Alternative `Relaxed`: Rejected - could allow reordering that delays visibility
- Alternative `Acquire`/`Release`: Overkill for simple flag

**Best Practices**:
- Store with `Ordering::SeqCst` in signal handler
- Load with `Ordering::SeqCst` in worker loops
- Keep atomic operations simple - no complex logic in signal context
- Never use `.unwrap()` or `.expect()` in signal handler

**Alternatives Considered**:
- Static `AtomicBool`: Rejected due to testing difficulties and inflexibility
- Channels (`crossbeam::channel`): Rejected due to higher overhead than atomics

---

## 3. Resource Cleanup Strategies

### Decision: RAII with `Drop` Trait

**Chosen Approach**: Rely on Rust's RAII pattern with the `Drop` trait for all resource cleanup.

**Rationale**:
- Automatic cleanup: Resources freed when they go out of scope
- Exception safety: Works correctly even during panics (stack unwinding calls `Drop`)
- Deterministic: Cleanup happens at predictable scope boundaries
- Memory safe: Compiler guarantees `Drop` is called exactly once
- Zero-cost abstraction: No runtime overhead

**Implementation Pattern**:
```rust
use std::fs::File;
use std::io::{Write, Result};

struct CompressionOutput {
    file: File,
    bytes_written: u64,
}

impl CompressionOutput {
    fn new(path: &Path) -> Result<Self> {
        Ok(Self {
            file: File::create(path)?,
            bytes_written: 0,
        })
    }

    fn write_compressed(&mut self, data: &[u8]) -> Result<()> {
        self.file.write_all(data)?;
        self.bytes_written += data.len() as u64;
        Ok(())
    }
}

impl Drop for CompressionOutput {
    fn drop(&mut self) {
        // File::drop is automatically called here
        // Happens even if cancelled or on panic
        log::debug!("Closed output file after writing {} bytes", self.bytes_written);
    }
}
```

### Temporary File Handling

**Decision**: Use the `tempfile` crate for temporary file management.

**Rationale**:
- Industry-standard solution for temporary files
- Automatic cleanup via `Drop` implementation
- Persistent option: Can call `.persist()` to keep file after success
- Well-tested: Handles edge cases (permissions, disk full, etc.)

**Implementation Pattern**:
```rust
use tempfile::NamedTempFile;
use std::fs;
use std::path::Path;

fn compress_to_temp(input: &Path, output: &Path) -> Result<()> {
    // Create temp file in same directory as output (for atomic rename)
    let temp_file = NamedTempFile::new_in(output.parent().unwrap())?;

    // Perform compression (cancellation checks inside)
    compress_data(input, &temp_file, &cancelled_flag)?;

    // On success, atomically rename temp to final output
    temp_file.persist(output)?;

    Ok(())
    // On error or cancellation, temp_file::Drop deletes the temp file
}
```

**Critical Limitation**: `NamedTempFile` cleanup relies on `Drop` being called. Cleanup may fail in:
1. `std::process::exit()` - terminates immediately without destructors
2. SIGKILL - cannot be caught, process killed instantly
3. Segfault - process terminates abnormally

**Mitigation**: Our Ctrl+C handler sets atomic flag, allowing graceful shutdown where `Drop` is called.

**Alternatives Considered**:
- Manual cleanup with `std::fs::remove_file`: Rejected - error-prone, doesn't work during panics
- Registration-based cleanup (`atexit`): Rejected - doesn't work with `exit()`, adds global state

---

## 4. Thread Coordination for Cancellation

### Decision: Rayon with Shared Atomic Flag

**Chosen Approach**: Pass `Arc<AtomicBool>` into Rayon parallel iterators and check at block boundaries.

**Rationale**:
- Native Rayon integration: Works naturally with work-stealing scheduler
- Low overhead: Atomic load is very cheap (single CPU instruction)
- Cooperative: Each task checks flag and returns early if set
- Fair: All worker threads see cancellation within milliseconds

**Implementation Pattern**:
```rust
use rayon::prelude::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

fn compress_blocks_parallel(
    blocks: Vec<Block>,
    cancelled: Arc<AtomicBool>,
) -> Result<Vec<CompressedBlock>> {
    blocks
        .into_par_iter()
        .map(|block| {
            // Check cancellation at start of each block
            if cancelled.load(Ordering::SeqCst) {
                return Err(CrushError::Cancelled);
            }

            // Compress the block
            let compressed = compress_block(&block)?;

            Ok(compressed)
        })
        .collect() // Stops early on first Err
}
```

**Cancellation Granularity**:

**Block-level (Recommended for Crush)**:
- Check frequency: Once per block (every 128KB)
- Latency: Worst case ~10-50ms (time to compress one block)
- Overhead: Negligible (<0.01% CPU time)
- Best for: Most compression operations

**Byte-level (Not Recommended)**:
- Check frequency: Every N bytes (e.g., every 4KB)
- Latency: Lower (<10ms worst case)
- Overhead: Higher (~0.1-1% CPU time)
- Best for: Very long-running operations (multi-minute compressions)

**Decision for Crush**: Use block-level cancellation because:
- Most file compressions complete in seconds, not minutes
- Block size (128KB) means max 50ms latency - acceptable for UX
- Minimal performance impact aligns with Performance First principle

**Alternatives Considered**:
- Rayon ThreadPool termination: Not possible - Rayon doesn't support killing thread pools
- Crossbeam scoped threads with channels: Rejected - more complex than Rayon + atomic flag

---

## 5. Exit Code Conventions

### Decision: Platform-Specific Exit Codes

**Chosen Approach**: Use exit code 130 on Unix/Linux, exit code 2 on Windows for SIGINT termination.

**Rationale**:
- Unix standard: Exit code 130 (128 + SIGINT signal number 2) is established convention
- Windows behavior: Windows doesn't follow Unix conventions; exit code 2 is typical for Ctrl+C
- Shell compatibility: Unix shells recognize 130 as signal termination
- Tool consistency: Matches behavior of standard tools (gzip, bash, zsh, etc.)

**Implementation Pattern**:
```rust
use std::process::ExitCode;

fn main() -> ExitCode {
    let cancelled = setup_signal_handler();

    match run_compression(cancelled) {
        Ok(_) => ExitCode::SUCCESS,
        Err(CrushError::Cancelled) => {
            eprintln!("Compression cancelled");

            // Platform-specific exit code
            #[cfg(unix)]
            return ExitCode::from(130);

            #[cfg(windows)]
            return ExitCode::from(2);
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            ExitCode::FAILURE
        }
    }
}
```

**Exit Code Semantics**:
- `0` - Success
- `1` - General error (I/O error, invalid input, compression failure)
- `2` - Command-line argument error OR Windows Ctrl+C (ambiguous on Windows)
- `130` - Unix/Linux SIGINT termination (128 + 2)

**Platform Differences**:

**Unix/Linux**:
- Formula: `128 + signal_number`
- SIGINT (signal 2) → exit code 130
- SIGTERM (signal 15) → exit code 143
- Shells recognize codes >128 as abnormal termination

**Windows**:
- No standardized signal exit codes
- Ctrl+C often results in exit code 2 or 3221225786 (OS-dependent)
- Cannot distinguish Ctrl+C from explicit `exit(2)` call

**Best Practice**: Document exit codes in CLI help text and man pages.

**Alternatives Considered**:
- Always use 130 regardless of platform: Rejected - confusing on Windows
- Always use 1 for all errors: Rejected - prevents distinguishing cancellation from errors

---

## 6. Time Estimation for Operations

### Decision: File Size + Throughput Estimation

**Chosen Approach**: Estimate compression time using `file_size / estimated_throughput_mbps`.

**Rationale**:
- Simple calculation: Requires only input file size (available immediately)
- Good enough: 5-second threshold is forgiving - doesn't need precision
- No overhead: No need to pre-scan file or perform sampling
- Adaptive: Can update estimates based on actual throughput after starting

**Implementation Pattern**:
```rust
use std::fs;
use std::path::Path;

const ESTIMATED_THROUGHPUT_MBS: f64 = 100.0; // MB/s conservative estimate
const PROGRESS_THRESHOLD_SECS: u64 = 5;

fn should_show_progress(input_path: &Path) -> Result<bool> {
    let metadata = fs::metadata(input_path)?;
    let file_size_mb = metadata.len() as f64 / (1024.0 * 1024.0);
    let estimated_time_secs = file_size_mb / ESTIMATED_THROUGHPUT_MBS;

    Ok(estimated_time_secs >= PROGRESS_THRESHOLD_SECS as f64)
}
```

**Throughput Estimation Guidelines**:

Based on constitution's Phase 1 performance targets:
- **Minimum target**: 500 MB/s on 8-core CPU (from constitution)
- **Conservative estimate for planning**: 100 MB/s
  - Accounts for slow disks, compression level, data incompressibility
  - Better to overestimate than underestimate
- **Maximum typical**: 1000+ MB/s on modern NVMe with 16+ cores

**File Size Thresholds**:
```
Throughput: 100 MB/s
5-second threshold = 500 MB

If file < 500 MB: No progress hint needed
If file >= 500 MB: Show "Press Ctrl+C to cancel" hint
```

**Alternatives Considered**:
- Pre-scanning file content: Rejected - doubles I/O time, violates Performance First
- Sampling first N blocks: Rejected - adds complexity, first blocks may not be representative
- Machine learning-based estimation: Rejected - massive overkill

---

## Implementation Recommendations Summary

Based on this research, here are the recommended implementations for Crush:

1. **Signal Handling**: Use `ctrlc` crate with `Arc<AtomicBool>` flag
2. **Cancellation Pattern**: Check atomic flag at block boundaries (every 128KB)
3. **Resource Cleanup**: Rely on RAII with `Drop` trait, use `tempfile` for temporary files
4. **Thread Coordination**: Pass atomic flag to Rayon parallel iterators
5. **Exit Codes**: 130 on Unix, 2 on Windows for cancellation
6. **Progress Decision**: Show progress if file size suggests >5 seconds compression time

**Constitution Compliance**:
- ✅ Performance First: Minimal overhead (<0.01% for atomic checks)
- ✅ Correctness & Safety: RAII guarantees cleanup, no `.unwrap()` in signal handlers
- ✅ Modularity: Cancellation flag is dependency-injected, testable
- ✅ Test-First: All patterns are unit-testable with mock atomic flags

---

## Dependencies

**New Dependency**:
- `ctrlc = "3.4"` - Cross-platform Ctrl+C signal handling

**Existing Dependencies** (no changes):
- `rayon` - Parallel processing (already in use)
- `std::sync::atomic` - Atomic types (standard library)
- `tempfile` - Temporary file management (may already be in use, verify)

---

**Research Status**: ✅ Complete
**Next Phase**: Phase 1 - Design & Contracts (data-model.md, contracts/, quickstart.md)
