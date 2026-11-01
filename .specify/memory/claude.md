# Crush Development Guidelines

Auto-generated from all feature plans, maintained by hand since spec-kit 1.x. Last updated: 2026-10-30

## Active Technologies

**Language**: Rust (stable, pinned via `rust-toolchain.toml`)
**Build**: Cargo workspace
**Parallelism**: `rayon` (CPU), `wgpu` + `pollster` (GPU, optional feature)
**Compression**: `flate2` (raw DEFLATE, Phase 1), `crc32fast` (CRC32 checksums)
**File I/O**: `memmap2` (zero-copy large file reads)
**Error handling**: `thiserror`
**CLI**: `clap` (crush-cli only), `indicatif` (progress bars, crush-cli only), `ctrlc`
**Plugin system**: `linkme` (distributed slice for auto-registration)
**Testing**: `cargo test`, `cargo-fuzz` (100k iterations minimum), `criterion` (benchmarks), `proptest`
**Linting**: `clippy` pedantic, `rustfmt`

## Project Structure

```text
crush/
├── crush-core/          # Core library: traits, error types, plugin registry, cancel token
│   ├── src/
│   │   ├── lib.rs
│   │   ├── engine.rs
│   │   ├── stream.rs
│   │   ├── block.rs
│   │   ├── pool.rs
│   │   ├── cancel.rs        # AtomicCancellationToken
│   │   ├── error.rs         # CrushError enum (all variants)
│   │   └── plugins/
│   └── Cargo.toml
├── crush-parallel/      # Parallel DEFLATE engine (new — feature 007)
│   ├── src/
│   │   ├── lib.rs
│   │   ├── engine.rs        # compress(), decompress() entry points
│   │   ├── block.rs         # Per-block compress/decompress, CRC32
│   │   ├── format.rs        # FileHeader, BlockHeader, BlockIndex, FileFooter (CRSH format)
│   │   ├── index.rs         # load_index(), decompress_block(), random access
│   │   ├── config.rs        # EngineConfiguration builder, ProgressEvent, ProgressCallback
│   │   └── gpu/             # Feature-gated: #[cfg(feature = "gpu")]
│   │       ├── mod.rs
│   │       ├── worker.rs    # wgpu init (pollster::block_on), compute dispatch, sync readback
│   │       └── shaders/deflate.wgsl
│   ├── benches/
│   └── fuzz/
├── crush-lz/            # fast-lz plugin: LZ4-class codec, parallel compress (features 023, 027)
│   ├── src/
│   │   ├── lib.rs           # FastLzPlugin (linkme-registered), compress/decompress API
│   │   ├── format.rs        # frame/block headers, bounds
│   │   ├── compress.rs      # greedy LZ77, 2^12 table, 6-byte hash
│   │   ├── decompress.rs    # validating decoder, fixed-size copies, streaming
│   │   └── estimate.rs      # sampled ratio estimate for selection
│   ├── benches/codec.rs     # bench.py suite `fast-lz`
│   ├── examples/codec_bench.rs  # in-memory timing (perf/lz-vs-lz4.sh)
│   └── fuzz/                # own [workspace]; fuzz_decompress, fuzz_roundtrip
├── crush-cli/           # Thin CLI wrapper
│   ├── src/
│   │   ├── main.rs
│   │   ├── args.rs
│   │   ├── signal.rs
│   │   └── commands/
│   │       ├── compress.rs    # indicatif progress bar reference implementation
│   │       └── decompress.rs  # --block N for random access
│   └── Cargo.toml
├── benches/
├── fuzz/
├── specs/               # SpecKit feature specs
│   ├── 001-project-structure/
│   ├── ...
│   └── 007-parallel-gzip-engine/  # Active feature
├── Cargo.toml           # Workspace manifest
└── CLAUDE.md
```

## CRSH File Format (Feature 007)

Binary format for parallel-decompress-friendly compressed files:

```
[FileHeader: 64B] [Block0: 16B header + payload] ... [BlockN] [IndexHeader: 8B] [N×IndexEntry: 20B each] [FileFooter: 24B]
```

- Magic: `CRSH` (4 bytes)
- Format version: `u32` — rejected if mismatch (no backward compat)
- Engine version: packed semver in header — shown in VersionMismatch errors
- Index at **end of file** (trailing, like zstd seekable format / XZ)
- Per-block CRC32 via `crc32fast`
- Incompressible blocks stored raw with flag bit
- Default block size: 1 MB

## fast-lz Format (Feature 023)

Payload inside the Crush outer header (magic `43 52 01 04`):

```
[frame: "LZ" | version 1 | flags 0 | block_size_log2 16..=20 | 3 reserved]
[block: u32 compressed_len (bit 31 = stored) | u32 uncompressed_len | data] ...
[end marker: u32 0]
```

- LZ4-style sequences: token (literal/match nibbles), 255-continued
  lengths, u16 offset, 64 KiB window, minimum match 4; matches stay
  within their block.
- Contract: `specs/023-lz4-fast-plugin/contracts/fast-lz-format.md`.

## Plugin Contract Hooks (Feature 023)

Provided methods on `CompressionAlgorithm` (defaults keep old behaviour):

- `content_aware()` / `estimate_ratio(input)`: with speed preferred
  (`throughput >= SPEED_PREFERRED_THROUGHPUT`, 0.8, i.e. `--level fast`)
  and no plugin named, `PluginSelector::select_for_input` uses the best
  content-aware plugin estimating `>= MIN_ESTIMATED_RATIO` (1.10);
  otherwise scoring is unchanged and content-aware plugins are excluded.
- `compress_into(input, cancel, &mut Vec)`: append the payload after
  crush-core's header prefix (no assembly copy).
- `streams_compressed_output()` + `compress_to(input, cancel, &mut dyn
  Write)`: `crush_core::compress_to_writer` (Write + Seek) streams and
  patches the CRC by seeking back.
- `decompress_to(input, cancel, &mut dyn Write)`:
  `crush_core::decompress_to_writer` checks the CRC first, then streams.

## Key API Patterns

```rust
// Configuration (builder pattern — constitution requirement)
let config = EngineConfiguration::builder()
    .workers(8)
    .block_size(1024 * 1024)
    .compression_level(6)
    .progress(Arc::new(Mutex::new(callback)))
    .build()?;

// Progress + cancellation — single callback, returns bool
pub type ProgressCallback = Box<dyn FnMut(ProgressEvent) -> bool + Send>;
// Returning false = cancel. Engine halts at next block boundary.

// Cancellation result — CrushError::Cancelled variant (existing)
// Use: error.is_cancelled() for ergonomic check

// Random access
let index = load_index(&mut reader)?;            // reads last 24B + index
let block = decompress_block(&mut reader, &index, block_n, &config)?;  // O(1) seek
```

## Commands

```bash
cargo build                          # build workspace
cargo test                           # run all tests
cargo clippy --all-targets -- -D warnings   # lint (pedantic, deny warnings)
cargo fmt --all -- --check           # format check
cargo bench                          # run criterion benchmarks
cargo fuzz run fuzz_decompress       # run fuzz target
cargo doc --no-deps                  # build docs
```

## Code Style

- No `.unwrap()` or `.expect()` in production code — use `?`
- Tests return `Result<()>` and propagate with `?`
- Builder pattern for configuration structs
- `CrushError` variants via `thiserror`, never raw `String` errors at library boundaries
- GPU code entirely behind `#[cfg(feature = "gpu")]`
- Rayon parallelism: `try_for_each` + `ControlFlow::Break` for cancellable pipelines

## Recent Changes

- **006-cancel-via-ctrl-c**: Added `AtomicCancellationToken` to `crush-core/src/cancel.rs`; wired `ctrlc` in crush-cli
- **007-parallel-gzip-engine**: New `crush-parallel` crate; CRSH binary format; rayon parallel DEFLATE; wgpu GPU feature; progress callback API; random access via block index
- **023-lz4-fast-plugin**: New `crush-lz` crate (`fast-lz` plugin); content-aware selection at `--level fast`; plugin hooks for in-place/streamed compression and streamed decompression; CLI memory-maps input files >= 1 MB (the one `unsafe`, in `crush-cli/src/commands/utils.rs`) and streams file output; `CRUSH_TIMINGS=1` phase timings; `perf/compare.py` subsets and `--abba`

<!-- MANUAL ADDITIONS START -->
<!-- MANUAL ADDITIONS END -->
