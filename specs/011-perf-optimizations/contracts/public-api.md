# Frozen Public-API Contract

**Feature**: 011-perf-optimizations | **Snapshot date**: 2026-04-17 | **Against**: `develop` HEAD

This document is the contract for FR-001 and SC-007: every `pub` item below MUST appear byte-identical in the post-011 public surface. CI gate: `cargo public-api diff --deny=all` on both `crush-core` and `crush-parallel`.

## crush-core

From [crush-core/src/lib.rs](../../crush-core/src/lib.rs):

### Modules (public)

- `pub mod cancel;`
- `pub mod compression;`
- `pub mod decompression;`
- `pub mod error;`
- `pub mod inspection;`
- `pub mod plugin;`

### Re-exports

- `pub use cancel::{AtomicCancellationToken, CancellationToken, ResourceTracker};`
- `pub use compression::{compress, compress_with_options, CompressionOptions};`
- `pub use decompression::{decompress, decompress_with_cancel};`
- `pub use error::{CrushError, PluginError, Result, TimeoutError, ValidationError};`

### Free functions

```rust
pub fn compress(input: &[u8]) -> Result<Vec<u8>>;

pub fn compress_with_options(input: &[u8], options: &CompressionOptions) -> Result<Vec<u8>>;

// from decompression.rs — exact signatures derived at T006 time via `cargo rustdoc`
// or `cargo public-api list --package crush-core`. The names above are the set that must
// not grow or shrink; nested signatures are frozen identically.
```

### `CompressionOptions`

```rust
pub struct CompressionOptions { /* private fields */ }
impl CompressionOptions {
    pub fn new() -> Self;
    pub fn with_plugin(self, name: &str) -> Self;
    pub fn with_weights(self, weights: ScoringWeights) -> Self;
    pub fn with_timeout(self, timeout: Duration) -> Self;
    pub fn with_file_metadata(self, metadata: FileMetadata) -> Self;
    pub fn with_cancel_token(self, token: Arc<dyn CancellationToken>) -> Self;
}
impl Debug for CompressionOptions;
impl Default for CompressionOptions;
impl Clone for CompressionOptions;
```

### Constants

- `pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(0);`

### Error types

Every variant of `CrushError`, `PluginError`, `TimeoutError`, `ValidationError` is frozen by variant name, variant shape, and `Display`/`Debug` output. The spec's FR-012 says error *messages* do not change; `cargo public-api diff` confirms the types do not change.

### Cancellation

- `pub trait CancellationToken: Send + Sync` — every method signature frozen.
- `pub struct AtomicCancellationToken` — API frozen.
- `pub struct ResourceTracker` — API frozen.

### Plugin surface

`pub mod plugin` — the full surface (contract traits, registry functions, timeout helpers, default plugin) is frozen by `cargo public-api list --package crush-core`. **A new function added in non-`pub` submodules is allowed; a new `pub` item in `crush-core::plugin` is NOT** — review gate.

> **Added in Slice D** (allowed because it's an additive `pub` item, documented explicitly here to keep the diff expected): `pub fn run_with_timeout_scoped<'scope, 'env, F, T>(scope: &'scope std::thread::Scope<'scope, 'env>, timeout: Duration, f: F) -> Result<T>` and `run_with_timeout_and_cancel_scoped` in `crush-core::plugin::timeout`. These are **new public items**, not modifications to existing ones — `cargo public-api diff` will flag them as additions, which is permitted by SC-007's "no breaking public-API change" phrasing. Document this expected diff in the PR description at T050.

Correction for SC-007 strict reading: SC-007 says "no change to the public API". Two readings exist:
1. **Strict**: zero additions or removals.
2. **Compatibility-only**: no removals or breaking changes; additions OK.

**Decision**: Go with reading 1 in this feature. Keep the scoped variants **`pub(crate)`** rather than `pub`, used only by `crush-core::compression` internally. This preserves a strict zero-diff public surface. Updated in data-model.md and research.md.

## crush-parallel

From [crush-parallel/src/lib.rs](../../crush-parallel/src/lib.rs):

### Modules (public)

- `pub mod block;`
- `pub mod config;`
- `pub mod engine;`
- `pub mod format;`
- `pub mod index;`

### Re-exports

- `pub use config::{EngineConfiguration, EngineConfigurationBuilder, ProgressCallback, ProgressEvent, ProgressPhase};`
- `pub use engine::{compress, compress_file, compress_stream, compress_to_writer, decompress, decompress_from_reader};`
- `pub use format::BlockIndexEntry;`
- `pub use index::{decompress_block, load_index, BlockIndex};`

### Constants

- `pub const PLUGIN_MAGIC: [u8; 4] = [0x43, 0x52, 0x01, 0x02];`

### Function signatures (frozen)

```rust
pub fn compress(input: &[u8], config: &EngineConfiguration) -> Result<Vec<u8>>;
pub fn compress_file(path: &Path, config: &EngineConfiguration) -> Result<Vec<u8>>;
pub fn compress_to_writer<W: Write>(input: &[u8], writer: W, config: &EngineConfiguration) -> Result<u64>;
pub fn compress_stream<R: Read, W: Write>(reader: R, writer: W, config: &EngineConfiguration) -> Result<u64>;

pub fn decompress(input: &[u8], config: &EngineConfiguration) -> Result<Vec<u8>>;
pub fn decompress_from_reader<R: Read + Seek>(reader: &mut R, config: &EngineConfiguration) -> Result<Vec<u8>>;

pub fn load_index<R: Read + Seek>(reader: &mut R) -> Result<BlockIndex>;
pub fn decompress_block<R: Read + Seek>(reader: &mut R, block_index: &BlockIndex, block_n: u64, config: &EngineConfiguration) -> Result<Vec<u8>>;
```

### `BlockIndex` public surface (frozen)

```rust
pub struct BlockIndex {
    pub entries: Vec<BlockIndexEntry>,
    pub checksums_enabled: bool,
    // NEW private field added in Slice E — NOT part of the public surface; does NOT show up in `cargo public-api`.
}
impl BlockIndex {
    pub fn uncompressed_offset(&self, block_n: u64) -> u64;
    pub fn block_for_offset(&self, uncompressed_offset: u64) -> Option<u64>;
    pub fn total_uncompressed_size(&self) -> u64;
    pub fn len(&self) -> u64;
    pub fn is_empty(&self) -> bool;
}
```

### `CompressedBlock` (from `block`)

```rust
pub struct CompressedBlock {
    pub header: BlockHeader,
    pub payload: Vec<u8>,
}
```

— unchanged (see [data-model.md](./data-model.md) § 2 for why the Cow-variant was rejected).

### Plugin integration

`struct ParallelDeflatePlugin` and its `impl CompressionAlgorithm` — not `pub`, no contract.

## Verification command

At any point, reproduce this snapshot with:

```bash
cargo public-api list --package crush-core     > /tmp/crush-core.surface
cargo public-api list --package crush-parallel > /tmp/crush-parallel.surface
diff -u /tmp/crush-core.surface /tmp/crush-core.surface.011-baseline
diff -u /tmp/crush-parallel.surface /tmp/crush-parallel.surface.011-baseline
```

The baseline files are captured once at T006 time against clean `develop`:

```bash
git checkout develop
cargo public-api list --package crush-core     > specs/011-perf-optimizations/contracts/crush-core.surface
cargo public-api list --package crush-parallel > specs/011-perf-optimizations/contracts/crush-parallel.surface
git checkout 011-perf-optimizations
```

These `.surface` files are the authoritative byte-level contract; this Markdown is the human-readable explanation.
