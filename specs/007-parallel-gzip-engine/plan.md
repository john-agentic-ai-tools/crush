# Implementation Plan: Parallel Compression Engine

**Branch**: `007-parallel-gzip-engine` | **Date**: 2026-02-21 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/007-parallel-gzip-engine/spec.md`

---

## Summary

Implement `crush-parallel`, a new workspace crate providing a pigz-inspired multi-threaded DEFLATE compression engine using a custom binary format (CRSH) optimised for parallel decompression and random block access. The engine uses `rayon` for CPU parallelism and optionally `wgpu` for GPU acceleration (feature-gated). It integrates into the existing `crush-core` plugin architecture and provides a progress callback/cancellation API. `crush-cli` gains a reference progress bar implementation.

---

## Technical Context

**Language/Version**: Rust stable (latest, pinned in `rust-toolchain.toml`)
**Primary Dependencies**:
- `rayon` — parallel block compression/decompression (workspace dep)
- `flate2` — raw DEFLATE encoding/decoding per block (workspace dep)
- `crc32fast` — per-block CRC32 checksums (workspace dep)
- `memmap2` — memory-mapped file I/O; used inside `compress_file()` for zero-copy large file reads (workspace dep, used in `crush-parallel`)
- `thiserror` — error types (workspace dep)
- `wgpu` + `pollster` — GPU compute, optional feature `gpu` (new, feature-gated)
- `linkme` — plugin registration (workspace dep)

**CLI additions**:
- `indicatif` — progress bar in `crush-cli` (already in crush-cli deps)

**Dev Dependencies** (workspace root):

- `cargo-husky` — pre-commit hooks enforcing `cargo fmt --check` and `cargo clippy --quiet` (MANDATORY per constitution)

**Storage**: Binary file format (`.crsh`). No database.
**Testing**: `cargo test`, `cargo-fuzz` (100k iterations), `criterion` benchmarks, `proptest` for round-trip properties.
**Target Platform**: Linux, macOS, Windows (library; no platform-specific code in default path).
**Performance Goals**: >500 MB/s @ 8 cores; linear scaling 1→4 cores; <100 ms random access on 10 GB files.
**Constraints**: <32 MB per thread; no async runtimes in `crush-core`/`crush-parallel`; no `.unwrap()` in production.

---

## Constitution Check

### I. Performance First ✅

- Zero-copy reads via `memmap2` for file inputs
- SIMD-friendly: `crc32fast` uses SSE 4.2 hardware CRC; `flate2`/`miniz_oxide` uses SIMD DEFLATE where available
- `rayon` work-stealing minimises thread idle time
- Memory pooling opportunity: pre-allocated output buffers per block (see Polish phase)
- All performance claims backed by `criterion` benchmarks

### II. Correctness & Safety ✅

- CRC32 per block mandatory by default (FR-010)
- Input validation at all boundaries (`EngineConfiguration::build()` validates all fields)
- No `.unwrap()` in production paths — all fallible operations use `?`
- Fuzz targets required: `fuzz_decompress` (arbitrary input → must not panic), `fuzz_roundtrip`
- Property tests: compress random data → decompress → assert identical

### III. Modularity & Extensibility ✅

- `crush-parallel` is a separate crate — `crush-core` has no compile-time dependency on it
- Plugin registered via `linkme` distributed slice under name `parallel-deflate` (FR-015, same pattern as existing plugins)
- Algorithm selection policy (`crush-cli/src/algorithm.rs`) is decoupled from the plugin library — policy changes do not require recompiling `crush-parallel` (FR-016)
- GPU path is entirely feature-gated — zero GPU symbols in default binary
- `EngineConfiguration` uses builder pattern per constitution requirement

### IV. Test-First Development ✅

- TDD enforced: tests written before implementation in each story phase
- Red-Green-Refactor cycle
- Roundtrip tests for all code paths (CPU, GPU if available, raw/stored blocks)

### Dependency Review

| New Dependency | Justification | Constitution Status |
|---|---|---|
| `wgpu` (optional, `gpu` feature) | Cross-platform GPU compute; synchronous dispatch; no async runtime | Allowed with justification: GPU acceleration (FR-008, US3) is a spec requirement. Feature-gated — default binary is unaffected. |
| `pollster` (optional, `gpu` feature) | Zero-dependency future executor for wgpu adapter init; replaces async runtime | Allowed: no async runtime in core path; `pollster` is used only for single-shot wgpu init. |

All other dependencies are existing workspace dependencies. No new additions to the non-GPU path.

---

## Project Structure

### Documentation (this feature)

```text
specs/007-parallel-gzip-engine/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/
│   └── rust-api.md      # Phase 1 output
└── tasks.md             # Phase 2 output (/speckit.tasks)
```

### Source Code

```text
crush-parallel/
├── Cargo.toml
├── src/
│   ├── lib.rs              # Public API surface, re-exports, plugin registration
│   ├── engine.rs           # compress(), decompress(), compress_stream() entry points
│   ├── block.rs            # Block splitting, per-block compress/decompress, checksum
│   ├── format.rs           # FileHeader, BlockHeader, BlockIndex, IndexHeader, FileFooter
│   │                       #   serialization/deserialization (little-endian binary)
│   ├── index.rs            # BlockIndex: load_index(), decompress_block(), random access
│   ├── config.rs           # EngineConfiguration, EngineConfigurationBuilder
│   │                       #   ProgressEvent, ProgressPhase, ProgressCallback type alias
│   └── gpu/
│       ├── mod.rs          # GpuWorker, feature-gated (#[cfg(feature = "gpu")])
│       ├── worker.rs       # wgpu adapter init (pollster::block_on), compute dispatch,
│       │                   #   device.poll(PollType::Wait) sync readback
│       └── shaders/
│           └── deflate.wgsl  # GDeflate-derived WGSL compute shader (block compression)
├── benches/
│   ├── throughput.rs       # Criterion: throughput vs thread count (1,2,4,8), block sizes
│   └── random_access.rs    # Criterion: load_index + decompress_block latency
└── fuzz/
    ├── Cargo.toml
    └── fuzz_targets/
        ├── fuzz_decompress.rs   # cargo-fuzz: arbitrary bytes → decompress → no panic
        └── fuzz_roundtrip.rs    # cargo-fuzz: random data → compress → decompress → identical

crush-core/src/error.rs          # Add: VersionMismatch, ChecksumMismatch,
                                 #   ExpansionLimitExceeded, IndexCorrupted variants
                                 #   Add: CrushError::is_cancelled() helper

crush-cli/src/commands/
├── compress.rs                  # Wire crush-parallel plugin; indicatif progress bar;
│                                #   invoke parallel-deflate automatically for inputs ≥ 25 MB
└── decompress.rs                # Wire crush-parallel plugin; indicatif progress bar;
                                 #   --block N flag for random access

crush-cli/src/algorithm.rs       # Algorithm selection logic: choose plugin by name or by
                                 #   input size (≥ 25 MB → parallel-deflate, else default);
                                 #   --algorithm flag; --parallel-threshold flag; verbose output

Cargo.toml (workspace root)      # Add crush-parallel to members[];
                                 # Add wgpu, pollster to [workspace.dependencies]
                                 #   (optional, behind feature flag)
```

**Structure Decision**: Separate `crush-parallel` crate per Principle III (Modularity). The `crush-core` crate is the stable interface layer; `crush-parallel` is a plugin implementation registered under the name `parallel-deflate` via the `linkme` distributed slice (FR-015). Algorithm selection (FR-016) lives in `crush-cli/src/algorithm.rs` — no coupling between selection policy and the plugin library. GPU code is entirely within `crush-parallel/src/gpu/`, gated by `#[cfg(feature = "gpu")]`.

---

## Implementation Phases

### Phase 1: Setup & Workspace (Foundational)

1. Add `crush-parallel` to workspace `Cargo.toml`
2. Create `crush-parallel/Cargo.toml` with correct dependencies
3. Add new `CrushError` variants to `crush-core/src/error.rs`
4. Add `CrushError::is_cancelled()` helper
5. Stub `crush-parallel/src/lib.rs` with public module declarations and `linkme` plugin registration (FR-015): `static PARALLEL_DEFLATE_PLUGIN: CompressionPlugin` under `#[crush_core::plugin::register]`
6. Stub `crush-cli/src/algorithm.rs` with `select_algorithm(input_size: u64, explicit: Option<&str>, threshold: u64) -> &'static str` signature (returns plugin name string)
7. Configure `cargo-husky` pre-commit hooks (`.cargo-husky/hooks/pre-commit`): `cargo fmt --check` + `cargo clippy --quiet` (constitution MANDATORY)
8. Verify `cargo build` compiles clean

### Phase 2: File Format (Foundational — blocks US1–US4)

1. Implement `format.rs`: all binary structs with `to_bytes()` / `from_bytes()` methods
2. Property tests: serialise → deserialise → assert equal for all structs
3. Edge cases: magic byte rejection, format version mismatch, truncated footer

### Phase 3: US1 — CPU Parallel Compression

TDD order:
1. Tests: `test_compress_roundtrip_small`, `test_compress_incompressible_stored`, `test_compression_scales_with_threads`
2. Implement `block.rs`: `compress_block()`, incompressible detection, CRC32
3. Implement `engine.rs`: `compress()` using `rayon::par_iter`, block ordering, output assembly
4. Implement `config.rs`: `EngineConfiguration` builder with validation
5. Benchmark: `throughput.rs` — verify >500 MB/s @ 8 cores, linear scaling 1→4

### Phase 4: US2 — Parallel Decompression

TDD order:
1. Tests: `test_decompress_roundtrip`, `test_decompress_corrupt_block_detected`, `test_decompress_scales_with_threads`
2. Implement `index.rs`: `load_index()`, index validation
3. Implement `engine.rs`: `decompress()` + `decompress_from_reader()` with parallel index-driven decompression
4. Verify checksum validation halts cleanly at corrupt block

### Phase 5: Cross-Cutting — Progress Callback, Cancellation & Algorithm Selection (FR-012, FR-013, FR-016)

TDD order:
1. Tests: `test_progress_events_emitted`, `test_cancel_halts_at_block_boundary`, `test_cancelled_discards_output`
2. Implement `config.rs`: `ProgressEvent`, `ProgressCallback` type alias, `EngineConfiguration::progress` field
3. Integrate callback invocation + `AtomicCancellationToken` into `engine.rs` compress/decompress loops
4. `crush-cli`: implement `indicatif` progress bar reference implementation (FR-013)
5. Wire `ctrlc` → `AtomicCancellationToken` → callback
6. `crush-cli/src/algorithm.rs`: implement full `select_algorithm()` logic — default threshold 25 MB, `--algorithm` override, `--parallel-threshold` flag; log selected plugin name when `--verbose` is set (FR-016)

### Phase 6: US4 — Random Access

TDD order:
1. Tests: `test_decompress_block_n`, `test_block_for_offset`, `test_random_access_does_not_read_other_blocks`
2. Implement `index.rs`: `decompress_block()`, `BlockIndex::block_for_offset()`, `uncompressed_offset()`
3. Benchmark: `random_access.rs` — verify <100 ms for last block on 10 GB file
4. `crush-cli`: add `--block N` flag to `decompress` command

### Phase 7: US3 (P3) — GPU Acceleration

TDD order:
1. Tests: `test_gpu_produces_identical_output_to_cpu` (conditional on `gpu` feature + adapter availability)
2. Implement `gpu/worker.rs`: `GpuWorker::new()` with `pollster::block_on(wgpu::Instance::request_adapter(...))`, fallback to `None` when no adapter
3. Implement WGSL shader `deflate.wgsl` (port GDeflate HLSL → WGSL)
4. Wire `GpuWorker` into `engine.rs`: when `config.gpu = true` and `GpuWorker::new()` returns `Some`, dispatch blocks to GPU; otherwise CPU
5. GPU failure mid-compression → fallback to CPU for remaining blocks (FR-008)

### Phase 8: Polish & Cross-Cutting

1. Fuzz targets: `fuzz_decompress` + `fuzz_roundtrip` — run 100k iterations (CI gate)
2. `proptest` round-trip property: arbitrary input → compress → decompress → identical
3. Documentation: all public API items with `///` doc comments and examples
4. `cargo doc --no-deps` — verify no warnings
5. Memory profiling: verify <32 MB per thread at default block size
6. Duplicate detection: run `detect-duplicates.ps1`, extract any patterns > 20 lines
7. `cargo clippy --all-targets -- -D warnings` clean
8. `cargo test` clean
9. `cargo bench` — baseline captured, no regressions

---

## Constitution Check (Post-Design)

Re-evaluated against all design decisions:

| Principle | Status | Notes |
|---|---|---|
| Performance First | ✅ | memmap2 zero-copy, crc32fast SIMD, rayon work-stealing, benchmarks required |
| Correctness & Safety | ✅ | No unwrap in production, fuzz required, CRC32 on by default |
| Modularity | ✅ | Separate crate, feature-gated GPU, plugin registration |
| Test-First | ✅ | TDD in each phase, property tests, fuzz |
| Dependencies | ✅ | wgpu/pollster only in optional `gpu` feature; zero new deps on default path |
| No async in core | ✅ | pollster used only for single-shot wgpu init, entirely within gpu feature |
| Clippy pedantic | ✅ | Enforced by existing workspace Cargo.toml lints |

No constitution violations. No complexity justification table required.
