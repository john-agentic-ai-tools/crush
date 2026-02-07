# Contract: Compression Engine API Changes

**Feature**: Graceful Cancellation Support
**Module**: `crush-core::engine`
**Date**: 2026-02-03

## Overview

This document defines the API changes required in the compression engine to support graceful cancellation. The changes are designed to be backward-compatible where possible, with new methods added alongside existing ones.

---

## New API: Cancellable Compression

### Method Signature

```rust
use std::io::{Read, Write};
use crate::cancel::CancellationToken;

impl CompressionEngine {
    /// Compress data with cancellation support.
    ///
    /// This method performs compression with periodic cancellation checks.
    /// If cancellation is requested, compression stops cleanly after completing
    /// the current block, and returns `Err(CrushError::Cancelled)`.
    ///
    /// # Arguments
    ///
    /// * `input` - Source data to compress
    /// * `output` - Destination for compressed data
    /// * `cancel_token` - Cancellation token to check between blocks
    ///
    /// # Returns
    ///
    /// * `Ok(CompressionStats)` - On successful compression
    /// * `Err(CrushError::Cancelled)` - If cancelled via cancel_token
    /// * `Err(CrushError::Io(e))` - On I/O errors
    /// * `Err(CrushError::Compression(e))` - On compression errors
    ///
    /// # Cancellation Guarantee
    ///
    /// When cancelled:
    /// - Current block completes compression (for data integrity)
    /// - No additional blocks are started
    /// - Output file may be incomplete (caller must delete)
    /// - No memory leaks (all resources freed via RAII)
    ///
    /// # Performance
    ///
    /// Cancellation checks occur once per block (default 128KB).
    /// Overhead: < 0.01% of total compression time.
    ///
    /// # Example
    ///
    /// ```rust
    /// use std::fs::File;
    /// use std::sync::Arc;
    /// use crush_core::engine::CompressionEngine;
    /// use crush_core::cancel::AtomicCancellationToken;
    ///
    /// let engine = CompressionEngine::new();
    /// let cancel_token = Arc::new(AtomicCancellationToken::new());
    ///
    /// let input = File::open("input.txt")?;
    /// let output = File::create("output.gz")?;
    ///
    /// match engine.compress_with_cancel(input, output, &cancel_token) {
    ///     Ok(stats) => println!("Compressed {} bytes", stats.compressed_size),
    ///     Err(CrushError::Cancelled) => {
    ///         eprintln!("Compression cancelled");
    ///         std::fs::remove_file("output.gz")?; // Clean up incomplete file
    ///     }
    ///     Err(e) => eprintln!("Error: {}", e),
    /// }
    /// ```
    pub fn compress_with_cancel<R: Read, W: Write>(
        &self,
        input: R,
        output: W,
        cancel_token: &dyn CancellationToken,
    ) -> Result<CompressionStats>;

    /// Decompress data with cancellation support.
    ///
    /// Similar to `compress_with_cancel`, but for decompression.
    /// Checks cancellation between blocks and returns cleanly.
    ///
    /// # Arguments
    ///
    /// * `input` - Compressed data source
    /// * `output` - Destination for decompressed data
    /// * `cancel_token` - Cancellation token to check between blocks
    ///
    /// # Returns
    ///
    /// Same return types as `compress_with_cancel`.
    ///
    /// # Cancellation Guarantee
    ///
    /// When cancelled:
    /// - Current block completes decompression
    /// - No additional blocks are decompressed
    /// - Output file may be incomplete (caller must delete)
    ///
    /// # Example
    ///
    /// ```rust
    /// let engine = CompressionEngine::new();
    /// let cancel_token = Arc::new(AtomicCancellationToken::new());
    ///
    /// let input = File::open("input.gz")?;
    /// let output = File::create("output.txt")?;
    ///
    /// engine.decompress_with_cancel(input, output, &cancel_token)?;
    /// ```
    pub fn decompress_with_cancel<R: Read, W: Write>(
        &self,
        input: R,
        output: W,
        cancel_token: &dyn CancellationToken,
    ) -> Result<CompressionStats>;
}
```

---

## Backward Compatibility

### Existing API (Unchanged)

```rust
impl CompressionEngine {
    /// Compress data without cancellation support (legacy API).
    ///
    /// This method exists for backward compatibility and simple use cases.
    /// For cancellable compression, use `compress_with_cancel`.
    ///
    /// # Implementation Note
    ///
    /// Internally calls `compress_with_cancel` with a never-cancelled token.
    pub fn compress<R: Read, W: Write>(
        &self,
        input: R,
        output: W,
    ) -> Result<CompressionStats> {
        let never_cancel = Arc::new(AtomicCancellationToken::new());
        self.compress_with_cancel(input, output, &*never_cancel)
    }

    /// Decompress data without cancellation support (legacy API).
    pub fn decompress<R: Read, W: Write>(
        &self,
        input: R,
        output: W,
    ) -> Result<CompressionStats> {
        let never_cancel = Arc::new(AtomicCancellationToken::new());
        self.decompress_with_cancel(input, output, &*never_cancel)
    }
}
```

**Compatibility Guarantees**:
- Existing code using `compress()` and `decompress()` continues to work
- No breaking changes to public API
- Signature and behavior unchanged
- Performance impact: negligible (extra atomic check that's never true)

---

## Internal Implementation Contract

### Block Processing Loop

```rust
// Pseudocode for internal block processing
fn compress_blocks<R: Read, W: Write>(
    input: R,
    output: W,
    cancel_token: &dyn CancellationToken,
) -> Result<()> {
    let mut block_reader = BlockReader::new(input);

    while let Some(block) = block_reader.next_block()? {
        // CHECK CANCELLATION BEFORE STARTING NEW BLOCK
        if cancel_token.is_cancelled() {
            return Err(CrushError::Cancelled);
        }

        // Compress the block (cannot be interrupted mid-block)
        let compressed = compress_block(&block)?;

        // Write compressed block (complete this before checking again)
        output.write_all(&compressed)?;
    }

    Ok(())
}
```

### Cancellation Check Placement

**Required Check Points**:
1. **Before starting new block**: MUST check cancellation
2. **After completing block**: MAY check for early detection (optional optimization)

**Prohibited Check Points**:
- Inside block compression algorithm (breaks atomicity)
- During file header/trailer writing (data integrity)
- Inside DEFLATE dictionary operations

### Parallel Processing with Rayon

```rust
use rayon::prelude::*;

fn compress_blocks_parallel(
    blocks: Vec<Block>,
    cancel_token: Arc<dyn CancellationToken>,
) -> Result<Vec<CompressedBlock>> {
    blocks
        .into_par_iter()
        .map(|block| {
            // Each worker checks cancellation
            if cancel_token.is_cancelled() {
                return Err(CrushError::Cancelled);
            }

            compress_block(&block)
        })
        .collect() // Stops early on first Err
}
```

---

## Return Types

### CompressionStats

```rust
/// Statistics from compression/decompression operation.
#[derive(Debug, Clone)]
pub struct CompressionStats {
    /// Number of bytes read from input
    pub input_size: u64,

    /// Number of bytes written to output
    pub output_size: u64,

    /// Compression ratio (output_size / input_size)
    pub compression_ratio: f64,

    /// Time taken for operation
    pub duration: Duration,

    /// Number of blocks processed
    pub blocks_processed: usize,
}
```

**Note**: On cancellation, `blocks_processed` reflects partial progress.

---

## Error Handling Contract

### Cancellation Error

```rust
#[derive(Debug, thiserror::Error)]
pub enum CrushError {
    /// Operation was cancelled via CancellationToken
    #[error("Operation cancelled")]
    Cancelled,

    /// I/O error during compression/decompression
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Compression algorithm error
    #[error("Compression failed: {0}")]
    Compression(String),

    /// Invalid compressed data format
    #[error("Invalid format: {0}")]
    InvalidFormat(String),

    /// Checksum mismatch
    #[error("Checksum mismatch: expected {expected:x}, got {actual:x}")]
    ChecksumMismatch {
        expected: u32,
        actual: u32,
    },
}

pub type Result<T> = std::result::Result<T, CrushError>;
```

### Error Context

When returning `Err(CrushError::Cancelled)`:
- No additional context needed (cancellation is self-explanatory)
- Caller is responsible for cleanup (deleting incomplete files)
- All allocated memory already freed via RAII
- No need to log error (CLI will display message)

---

## Testing Contract

### Required Tests

```rust
#[test]
fn compress_respects_cancellation() -> Result<()> {
    let input = vec![0u8; 1_000_000]; // 1MB of zeros
    let mut output = Vec::new();
    let cancel_token = Arc::new(AtomicCancellationToken::new());

    // Cancel immediately
    cancel_token.cancel();

    let engine = CompressionEngine::new();
    let result = engine.compress_with_cancel(
        &input[..],
        &mut output,
        &*cancel_token,
    );

    assert!(matches!(result, Err(CrushError::Cancelled)));
    Ok(())
}

#[test]
fn compress_completes_without_cancellation() -> Result<()> {
    let input = b"test data".repeat(1000);
    let mut output = Vec::new();
    let cancel_token = Arc::new(AtomicCancellationToken::new());

    let engine = CompressionEngine::new();
    let stats = engine.compress_with_cancel(
        &input[..],
        &mut output,
        &*cancel_token,
    )?;

    assert!(stats.output_size > 0);
    assert!(!cancel_token.is_cancelled());
    Ok(())
}

#[test]
fn cancel_during_compression() -> Result<()> {
    use std::sync::Arc;
    use std::thread;

    let input = vec![0u8; 10_000_000]; // 10MB
    let mut output = Vec::new();
    let cancel_token = Arc::new(AtomicCancellationToken::new());

    // Cancel after 10ms
    let cancel_clone = Arc::clone(&cancel_token);
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(10));
        cancel_clone.cancel();
    });

    let engine = CompressionEngine::new();
    let result = engine.compress_with_cancel(
        &input[..],
        &mut output,
        &*cancel_token,
    );

    // Should be cancelled (10MB takes >10ms to compress)
    assert!(matches!(result, Err(CrushError::Cancelled)));
    Ok(())
}
```

---

## Migration Guide

### For Library Users

**Old Code**:
```rust
let engine = CompressionEngine::new();
engine.compress(input, output)?;
```

**New Code (with cancellation)**:
```rust
let engine = CompressionEngine::new();
let cancel_token = Arc::new(AtomicCancellationToken::new());

// Register signal handler
let cancel_signal = Arc::clone(&cancel_token);
ctrlc::set_handler(move || cancel_signal.cancel())?;

// Use cancellable API
match engine.compress_with_cancel(input, output, &cancel_token) {
    Ok(_) => println!("Success"),
    Err(CrushError::Cancelled) => {
        println!("Cancelled");
        // Clean up incomplete output
    }
    Err(e) => eprintln!("Error: {}", e),
}
```

### For Plugin Developers

Plugins implementing `CompressionAlgorithm` trait must:
1. Accept `cancel_token: &dyn CancellationToken` parameter
2. Check `cancel_token.is_cancelled()` between blocks
3. Return `Err(CrushError::Cancelled)` when detected

---

**Contract Status**: ✅ Defined
**Implementation**: Required in `crush-core/src/engine.rs`
