# API Contract: crush-parallel (Rust Library)

**Branch**: `007-parallel-gzip-engine`
**Date**: 2026-02-21
**Crate**: `crush-parallel`

This document defines the public Rust API contract. It is technology-specific (Rust) by nature — the spec defines WHAT, this defines HOW the public surface is shaped.

---

## Public Types

### `EngineConfiguration`

Builder for all compression and decompression operations.

```rust
/// Configuration for the parallel compression/decompression engine.
///
/// Construct via [`EngineConfiguration::builder()`].
pub struct EngineConfiguration {
    pub workers: usize,
    pub block_size: u32,
    pub compression_level: u8,
    pub max_expansion_ratio: f64,
    pub max_decompression_ratio: f64,
    pub checksums: bool,
    pub gpu: bool,
    pub progress: Option<Arc<Mutex<ProgressCallback>>>,
}

pub struct EngineConfigurationBuilder { /* private fields */ }

impl EngineConfiguration {
    pub fn builder() -> EngineConfigurationBuilder;
    pub fn default() -> Self;  // 1 MB blocks, level 6, 8 workers, checksums on, gpu off
}

impl EngineConfigurationBuilder {
    pub fn workers(self, n: usize) -> Self;
    pub fn block_size(self, bytes: u32) -> Self;
    pub fn compression_level(self, level: u8) -> Self;
    pub fn max_expansion_ratio(self, ratio: f64) -> Self;
    pub fn max_decompression_ratio(self, ratio: f64) -> Self;
    pub fn checksums(self, enabled: bool) -> Self;
    pub fn gpu(self, enabled: bool) -> Self;
    pub fn progress(self, cb: Arc<Mutex<ProgressCallback>>) -> Self;
    pub fn build(self) -> Result<EngineConfiguration, CrushError>;
}
```

---

### `ProgressEvent`

```rust
#[derive(Debug, Clone)]
pub struct ProgressEvent {
    /// Cumulative uncompressed bytes processed so far.
    pub bytes_processed: u64,
    /// Number of blocks fully processed.
    pub blocks_completed: u64,
    /// Total blocks in the operation. None for streaming inputs.
    pub total_blocks: Option<u64>,
    /// Whether we are compressing or decompressing.
    pub phase: ProgressPhase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressPhase {
    Compressing,
    Decompressing,
}

/// Callback type. Returns `true` to continue, `false` to cancel.
pub type ProgressCallback = Box<dyn FnMut(ProgressEvent) -> bool + Send>;
```

---

### `CrushError` additions

The following variants are added to the existing `CrushError` enum in `crush-core`:

```rust
pub enum CrushError {
    // ... existing variants ...

    /// The file was produced by a different engine version.
    /// Decompression is refused. User must use the named version.
    #[error("version mismatch: file was produced by engine {file_version}, current engine is {current_version}")]
    VersionMismatch {
        file_version: String,
        current_version: String,
    },

    /// The file header magic bytes do not match `CRSH`.
    #[error("invalid format: magic bytes do not match (expected CRSH)")]
    InvalidFormat(String),

    /// A block's CRC32 checksum does not match.
    #[error("checksum mismatch at block {block_index}: expected {expected:#010x}, got {actual:#010x}")]
    ChecksumMismatch {
        block_index: u64,
        expected: u32,
        actual: u32,
    },

    /// Decompressed output would exceed the configured expansion ratio limit.
    #[error("expansion limit exceeded at block {block_index}: decompressed size would exceed limit")]
    ExpansionLimitExceeded { block_index: u64 },

    /// The block index is missing or truncated.
    #[error("index corrupted or truncated: {0}")]
    IndexCorrupted(String),

    // Cancelled is already present in existing CrushError.
}

impl CrushError {
    /// Returns true if this error represents a user-initiated cancellation.
    pub fn is_cancelled(&self) -> bool {
        matches!(self, CrushError::Cancelled)
    }
}
```

---

## Entry Point Functions

All functions are synchronous (no `async`). All are safe Rust (no `unsafe` in the public API).

### `compress`

```rust
/// Compress `input` bytes using the parallel block engine.
///
/// # Returns
/// - `Ok(Vec<u8>)` — compressed output in CRSH format
/// - `Err(CrushError::Cancelled)` — operation was cancelled via the progress callback
/// - `Err(CrushError::InvalidConfig)` — configuration validation failed
///
/// # Example
/// ```rust
/// use crush_parallel::{compress, EngineConfiguration};
///
/// let config = EngineConfiguration::default();
/// let compressed = compress(b"hello world", &config)?;
/// # Ok::<(), crush_core::CrushError>(())
/// ```
pub fn compress(input: &[u8], config: &EngineConfiguration) -> Result<Vec<u8>, CrushError>;
```

### `compress_to_writer`

```rust
/// Compress `input` bytes, writing the CRSH output to `writer`.
///
/// Suitable for streaming to a file or network socket.
pub fn compress_to_writer<W: Write>(
    input: &[u8],
    writer: W,
    config: &EngineConfiguration,
) -> Result<u64, CrushError>;  // returns bytes written
```

### `compress_stream`

```rust
/// Compress a [`Read`] stream, writing CRSH output to `writer`.
///
/// Total input size is unknown at start. `FileHeader::block_count` and
/// `FileHeader::uncompressed_size` will be `u64::MAX` in the output header
/// and patched to real values in the footer — or left as `u64::MAX` if
/// `writer` is not seekable.
pub fn compress_stream<R: Read, W: Write>(
    reader: R,
    writer: W,
    config: &EngineConfiguration,
) -> Result<u64, CrushError>;
```

### `decompress`

```rust
/// Decompress a CRSH-format byte slice.
///
/// Reads the file footer first, loads the index, then decompresses all
/// blocks in parallel.
///
/// # Errors
/// - `VersionMismatch` — file was produced by a different engine version
/// - `InvalidFormat` — magic bytes invalid or file truncated
/// - `ChecksumMismatch` — a block's CRC32 does not match
/// - `ExpansionLimitExceeded` — output would exceed `config.max_decompression_ratio`
/// - `IndexCorrupted` — block index is missing or truncated
/// - `Cancelled` — cancelled via progress callback
pub fn decompress(input: &[u8], config: &EngineConfiguration) -> Result<Vec<u8>, CrushError>;
```

### `decompress_from_reader`

```rust
/// Decompress a CRSH file from a seekable reader.
///
/// The reader must implement `Read + Seek` to enable footer-first index loading
/// and parallel random-access reads.
pub fn decompress_from_reader<R: Read + Seek>(
    reader: R,
    config: &EngineConfiguration,
) -> Result<Vec<u8>, CrushError>;
```

### `decompress_block`

```rust
/// Decompress a single block by block index.
///
/// The `index` parameter is a loaded [`BlockIndex`] (obtained via
/// [`load_index`]). The `reader` must implement `Read + Seek`.
///
/// This is the random-access entry point. Satisfies FR-005 and US4 (P4).
///
/// # Performance
/// Time-to-first-byte is O(1) in the number of blocks — requires only a
/// single seek to `block_index.entries[block_n].block_offset`.
pub fn decompress_block<R: Read + Seek>(
    reader: &mut R,
    block_index: &BlockIndex,
    block_n: u64,
    config: &EngineConfiguration,
) -> Result<Vec<u8>, CrushError>;
```

### `load_index`

```rust
/// Load the [`BlockIndex`] from a seekable reader.
///
/// Reads the last 24 bytes (FileFooter), validates magic and checksum,
/// then reads the index region.
///
/// This function is called once per file open; subsequent block reads
/// use the loaded index for O(1) seeking.
pub fn load_index<R: Read + Seek>(reader: &mut R) -> Result<BlockIndex, CrushError>;
```

---

## `BlockIndex` Type

```rust
/// In-memory representation of the trailing block index.
#[derive(Debug, Clone)]
pub struct BlockIndex {
    pub entries: Vec<BlockIndexEntry>,
}

#[derive(Debug, Clone, Copy)]
pub struct BlockIndexEntry {
    pub block_offset: u64,
    pub compressed_size: u32,
    pub uncompressed_size: u32,
    pub checksum: u32,
}

impl BlockIndex {
    /// Returns the absolute byte offset within the original uncompressed stream
    /// at which block `n` begins. O(N) — sums preceding uncompressed sizes.
    pub fn uncompressed_offset(&self, block_n: u64) -> u64;

    /// Returns the block index containing the given uncompressed byte offset.
    /// O(N) binary search over cumulative uncompressed sizes.
    pub fn block_for_offset(&self, uncompressed_offset: u64) -> Option<u64>;

    /// Total uncompressed size across all blocks.
    pub fn total_uncompressed_size(&self) -> u64;

    /// Number of blocks.
    pub fn len(&self) -> u64;
}
```

---

## `CompressionAlgorithm` Plugin Registration

`crush-parallel` registers itself as a plugin into `crush-core`'s plugin registry via the `linkme`-based distributed slice pattern already used by the existing plugin system:

```rust
// In crush-parallel/src/lib.rs
use crush_core::plugin::CompressionPlugin;

#[crush_core::plugin::register]
static PARALLEL_DEFLATE_PLUGIN: CompressionPlugin = CompressionPlugin {
    name: "parallel-deflate",
    description: "Multi-threaded DEFLATE compression with CRSH block format",
    compress: parallel_compress_fn,
    decompress: parallel_decompress_fn,
};
```

This means `crush-cli` automatically discovers `crush-parallel` when it is a workspace dependency — no manual registration required.

---

## Error Handling Contract

| Scenario | Error Returned |
|---|---|
| Config validation fails (e.g., block_size < 64 KB) | `CrushError::InvalidConfig(String)` |
| File magic bytes invalid | `CrushError::InvalidFormat(String)` |
| Format version mismatch | `CrushError::VersionMismatch { file_version, current_version }` |
| Block index missing/truncated | `CrushError::IndexCorrupted(String)` |
| CRC32 checksum mismatch | `CrushError::ChecksumMismatch { block_index, expected, actual }` |
| Expansion limit exceeded | `CrushError::ExpansionLimitExceeded { block_index }` |
| I/O error | `CrushError::Io(std::io::Error)` |
| Progress callback returned false | `CrushError::Cancelled` |
| GPU not available (fallback to CPU) | No error — silent fallback, log at debug level |

**Contract for `Cancelled`**:
- The engine halts at the next block boundary.
- Partial output is discarded (not returned to the caller).
- All worker threads complete their current block before the function returns.
- The caller can distinguish cancellation via `error.is_cancelled()`.
