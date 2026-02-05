# Data Model: Graceful Cancellation Support

**Feature**: Graceful Cancellation Support
**Branch**: 006-cancel-via-ctrl-c
**Date**: 2026-02-03

This document defines the data structures and state machines for implementing graceful cancellation in Crush.

---

## Entities

### 1. CancellationToken

**Purpose**: Thread-safe cancellation signal that can be checked by compression/decompression workers.

**Fields**:
- `cancelled: AtomicBool` - Atomic flag indicating cancellation state
- `signal_received_at: Option<Instant>` - Timestamp when cancellation was first requested (for metrics/debugging)

**Methods**:
```rust
pub trait CancellationToken: Send + Sync {
    /// Check if cancellation has been requested (non-blocking)
    fn is_cancelled(&self) -> bool;

    /// Request cancellation (idempotent - safe to call multiple times)
    fn cancel(&self);

    /// Reset cancellation state for next operation
    fn reset(&self);

    /// Get time elapsed since cancellation was requested
    fn time_since_cancel(&self) -> Option<Duration>;
}
```

**Lifecycle**:
1. Created at operation start (or reused from previous operation after reset)
2. Checked between blocks during compression/decompression
3. Set to cancelled when signal received or explicit cancel() called
4. Reset after operation completes (success or cancellation)

**Invariants**:
- Once `cancelled` is set to true, it remains true until reset
- `is_cancelled()` must be lock-free and very fast (<10 nanoseconds)
- Safe to call from signal handler context

**Implementation Note**: Use `Arc<AtomicCancellationToken>` to share across threads.

---

### 2. OperationState

**Purpose**: Track the current state of a compression/decompression operation for coordinated shutdown.

**States**:
```rust
pub enum OperationState {
    /// Operation is actively running
    Running,

    /// Cancellation requested, cleanup in progress
    Cancelling,

    /// Operation fully cancelled, cleanup complete
    Cancelled,

    /// Operation completed successfully
    Completed,
}
```

**State Transitions**:
```
Running → Cancelling  (on SIGINT or explicit cancel)
Running → Completed   (on successful completion)
Cancelling → Cancelled (after cleanup finishes)
```

**Invalid Transitions** (compile-time prevented via type system):
- `Cancelled → Running` (must create new operation or reset)
- `Completed → Cancelling` (operation already finished)
- `Cancelled → Completed` (cannot complete after cancellation)

**Validation Rules**:
- Cannot transition from `Completed` or `Cancelled` to `Running` without creating new state
- `Cancelling` state must eventually reach `Cancelled` (no indefinite hang)
- State transitions must be atomic (use `AtomicU8` or similar for lock-free updates)

**Usage Pattern**:
```rust
let state = Arc::new(AtomicOperationState::new(OperationState::Running));

// Worker thread checks state
match state.load() {
    OperationState::Running => { /* continue work */ },
    OperationState::Cancelling => { /* finish current block, then stop */ },
    OperationState::Cancelled | OperationState::Completed => { /* exit */ },
}

// Signal handler transitions to cancelling
state.transition(OperationState::Cancelling);
```

---

### 3. ResourceTracker

**Purpose**: Track all resources (files, handles) created during an operation for guaranteed cleanup.

**Fields**:
- `output_path: Option<PathBuf>` - Final output file path (if created)
- `temp_files: Vec<PathBuf>` - Temporary files to delete on cancellation
- `file_handles: Vec<File>` - Open file handles to close
- `is_complete: AtomicBool` - Whether operation completed successfully

**Methods**:
```rust
pub struct ResourceTracker {
    output_path: Mutex<Option<PathBuf>>,
    temp_files: Mutex<Vec<PathBuf>>,
    is_complete: AtomicBool,
}

impl ResourceTracker {
    pub fn new() -> Self { /* ... */ }

    /// Register output file path (to delete on cancellation)
    pub fn register_output(&self, path: PathBuf);

    /// Register temporary file (to delete always)
    pub fn register_temp_file(&self, path: PathBuf);

    /// Mark operation as successfully completed (keep output file)
    pub fn mark_complete(&self);

    /// Clean up all tracked resources
    pub fn cleanup_all(&self) -> Result<()>;
}

impl Drop for ResourceTracker {
    fn drop(&mut self) {
        if !self.is_complete.load(Ordering::SeqCst) {
            // Operation did not complete - clean up everything
            let _ = self.cleanup_all();
        }
    }
}
```

**Lifecycle**:
1. Created at operation start
2. Resources registered as they're created (output files, temp files)
3. On successful completion: `mark_complete()` called, output kept
4. On cancellation or error: `Drop` called, all resources cleaned up
5. Files closed before deletion (Windows requirement)

**Cleanup Order** (critical for correctness):
1. Close all file handles (if still open)
2. Delete temporary files
3. Delete output file (if operation not complete)
4. Log cleanup summary

**Invariants**:
- All registered temp files are deleted on drop (regardless of `is_complete`)
- Output file is deleted only if `is_complete` is false
- Cleanup is best-effort - logs errors but doesn't panic
- Thread-safe: multiple threads can register resources concurrently

---

## State Machines

### Operation Lifecycle State Machine

```
┌─────────────┐
│   Initial   │
└──────┬──────┘
       │
       │ create()
       ▼
┌─────────────┐
│   Running   │◄─────────────┐
└──────┬──────┘              │
       │                     │
       ├─── SIGINT ──────────┤ reset()
       │                     │
       ▼                     │
┌─────────────┐              │
│ Cancelling  │              │
└──────┬──────┘              │
       │                     │
       │ cleanup_complete()  │
       ▼                     │
┌─────────────┐              │
│  Cancelled  │──────────────┘
└─────────────┘

Alternative path:
Running ──success()──> Completed ──reset()──> Running
```

**State Descriptions**:
- **Initial**: No operation in progress
- **Running**: Active compression/decompression, checking cancellation flag
- **Cancelling**: Cancellation requested, workers finishing current blocks
- **Cancelled**: All workers stopped, cleanup complete, ready to exit
- **Completed**: Operation finished successfully

**Triggering Events**:
- `create()`: User starts compression/decompression
- `SIGINT`: User presses Ctrl+C
- `cleanup_complete()`: Last worker finishes cleanup
- `success()`: Operation completes successfully
- `reset()`: Prepare for next operation (CLI tool may process multiple files)

---

### Resource Cleanup State Machine

```
┌──────────────┐
│   Created    │
└──────┬───────┘
       │
       │ register_resources()
       ▼
┌──────────────┐
│   Tracking   │
└──────┬───────┘
       │
       ├─── mark_complete() ───┐
       │                       │
       │                       ▼
       │              ┌──────────────┐
       │              │  Complete    │
       │              │ (keep output)│
       │              └──────┬───────┘
       │                     │
       │                     │ drop()
       │                     ▼
       │              ┌──────────────┐
       │              │ Cleanup Temps│
       │              └──────────────┘
       │
       │
       ├─── drop() (incomplete) ───┐
       │                            │
       │                            ▼
       │                   ┌──────────────┐
       │                   │ Cleanup All  │
       │                   │ (temps +     │
       │                   │  output)     │
       │                   └──────────────┘
       │
       └─── panic/exit ────> Cleanup All
```

**State Descriptions**:
- **Created**: ResourceTracker initialized, no resources yet
- **Tracking**: Actively registering created resources
- **Complete**: Operation successful, output should be kept
- **Cleanup Temps**: Delete temp files only (output successful)
- **Cleanup All**: Delete everything (operation failed/cancelled)

---

## Relationships

```
┌─────────────────────┐
│  CLI Main Thread    │
└──────────┬──────────┘
           │
           │ creates
           ▼
┌─────────────────────┐          ┌─────────────────────┐
│ CancellationToken   │◄─────────│  Signal Handler     │
│ (Arc<AtomicBool>)   │  checks  │  (ctrlc thread)     │
└──────────┬──────────┘          └─────────────────────┘
           │
           │ shared with
           ▼
┌─────────────────────┐          ┌─────────────────────┐
│  Rayon ThreadPool   │◄─────────│  ResourceTracker    │
│  (worker threads)   │  uses    │  (cleanup on drop)  │
└──────────┬──────────┘          └─────────────────────┘
           │
           │ checks between blocks
           ▼
┌─────────────────────┐
│  Compression        │
│  Engine Core        │
└─────────────────────┘
```

**Ownership & Lifetimes**:
- `CancellationToken`: `Arc<>` shared between main thread, signal handler, and all workers
- `ResourceTracker`: Owned by operation coordinator, dropped when scope exits
- `OperationState`: `Arc<>` shared for coordinated state transitions

---

## Data Validation

### CancellationToken Validation

**Invariants**:
- `is_cancelled()` is always lock-free and async-signal-safe
- `cancel()` is idempotent - calling multiple times has same effect as once
- Atomic operations use `SeqCst` ordering for correctness
- No allocations or blocking operations in hot path

**Validation Tests**:
```rust
#[test]
fn cancellation_token_is_lock_free() -> Result<()> {
    let token = Arc::new(AtomicCancellationToken::new());
    assert!(AtomicBool::is_lock_free()); // Platform check
    Ok(())
}

#[test]
fn cancel_is_idempotent() -> Result<()> {
    let token = Arc::new(AtomicCancellationToken::new());
    token.cancel();
    token.cancel();
    token.cancel();
    assert!(token.is_cancelled());
    Ok(())
}

#[test]
fn reset_clears_cancellation() -> Result<()> {
    let token = Arc::new(AtomicCancellationToken::new());
    token.cancel();
    assert!(token.is_cancelled());
    token.reset();
    assert!(!token.is_cancelled());
    Ok(())
}
```

### ResourceTracker Validation

**Invariants**:
- All registered temp files must be deleted on drop
- Output file deleted only if `is_complete == false`
- File handles closed before attempting deletion (Windows compatibility)
- Cleanup errors are logged but don't panic

**Validation Tests**:
```rust
#[test]
fn temp_files_deleted_on_incomplete() -> Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let temp_file = temp_dir.path().join("temp.gz");

    {
        let tracker = ResourceTracker::new();
        File::create(&temp_file)?;
        tracker.register_temp_file(temp_file.clone());
        // Drop without mark_complete()
    }

    assert!(!temp_file.exists());
    Ok(())
}

#[test]
fn output_kept_when_complete() -> Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let output_file = temp_dir.path().join("output.gz");

    {
        let tracker = ResourceTracker::new();
        File::create(&output_file)?;
        tracker.register_output(output_file.clone());
        tracker.mark_complete(); // Mark as successful
        // Drop
    }

    assert!(output_file.exists()); // Output kept
    Ok(())
}
```

---

**Data Model Status**: ✅ Complete
**Next**: Create API contracts in `contracts/` directory
