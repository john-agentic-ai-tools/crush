# Tasks: Parallel Compression Engine (crush-parallel)

**Input**: Design documents from `/specs/007-parallel-gzip-engine/`
**Prerequisites**: plan.md ✅, spec.md ✅, research.md ✅, data-model.md ✅, contracts/rust-api.md ✅, quickstart.md ✅

**TDD**: Test-first development is **mandatory** per the project constitution. Within each phase, test tasks appear before implementation tasks.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story?] Description`

- **[P]**: Can run in parallel (different files, no shared state dependencies)
- **[Story]**: Which user story this task belongs to ([US1], [US2], [US3], [US4])
- All file paths are relative to the workspace root

---

## Phase 1: Setup (Workspace & Crate Scaffolding)

**Purpose**: Initialize the `crush-parallel` crate within the existing Cargo workspace and create the new `CrushError` variants required by all subsequent stories.

- [ ] T001 Add `crush-parallel` to `members[]` array in workspace `Cargo.toml`
- [ ] T002 Create `crush-parallel/Cargo.toml` with all dependencies: `rayon`, `flate2`, `crc32fast`, `memmap2`, `thiserror`, `linkme` (workspace deps); optional feature `gpu`: `wgpu`, `pollster`
- [ ] T003 [P] Add `VersionMismatch`, `InvalidFormat`, `ChecksumMismatch`, `ExpansionLimitExceeded`, `IndexCorrupted` variants and `is_cancelled()` helper method to `crush-core/src/error.rs`
- [ ] T004 [P] Create stub `crush-parallel/src/lib.rs` declaring all public module paths (`engine`, `block`, `format`, `index`, `config`, `gpu`) and public re-exports
- [ ] T005 Run `cargo build` to verify the workspace compiles cleanly with the new crate skeleton

---

## Phase 2: Foundational — CRSH File Format & Configuration

**Purpose**: Implement the CRSH binary format serialization layer and the `EngineConfiguration` builder. These are hard prerequisites for **all** user stories — no compression or decompression logic can be implemented until this phase is complete.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

### Tests (write first — must fail before implementation)

- [ ] T006 Write `test_file_header_roundtrip` property test (serialize → deserialize → assert equal, including magic rejection and version mismatch detection) in `crush-parallel/src/format.rs`
- [ ] T007 [P] Write `test_block_header_roundtrip`, `test_block_index_entry_roundtrip`, `test_index_header_roundtrip`, and `test_file_footer_roundtrip` property tests in `crush-parallel/src/format.rs`
- [ ] T008 [P] Write `test_engine_configuration_builder_validates_fields` (invalid block_size, invalid level, zero ratios) in `crush-parallel/src/config.rs`

### Implementation

- [ ] T009 Implement `FileHeader` struct (magic `[u8;4]`, format_version `u32`, engine_version `EngineVersion` 8B, block_size `u32`, compression_level `u8`, flags `FileFlags` 1B, reserved 2B, uncompressed_size `u64`, block_count `u64`, _reserved 24B = 64B total) with `to_bytes()` / `from_bytes()` in `crush-parallel/src/format.rs`
- [ ] T010 [P] Implement `BlockHeader` struct (compressed_size `u32`, uncompressed_size `u32`, checksum `u32`, flags `BlockFlags` 1B, _reserved 3B = 16B total) with `to_bytes()` / `from_bytes()` in `crush-parallel/src/format.rs`
- [ ] T011 [P] Implement `BlockIndexEntry` struct (block_offset `u64`, compressed_size `u32`, uncompressed_size `u32`, checksum `u32` = 20B total) with `to_bytes()` / `from_bytes()` in `crush-parallel/src/format.rs`
- [ ] T012 [P] Implement `IndexHeader` struct (entry_count `u32`, index_flags `u32` = 8B) and `FileFooter` struct (index_offset `u64`, index_size `u32`, footer_checksum `u32`, format_version `u32`, magic `[u8;4]` = 24B total) with `to_bytes()` / `from_bytes()` in `crush-parallel/src/format.rs`
- [ ] T013 Implement `EngineConfiguration` with builder pattern (`EngineConfigurationBuilder`), field validation in `build()` (block_size in [65536, 268435456], level in [0, 9], ratios > 0.0), and `Default` impl (1 MB blocks, level 6, workers=0, checksums=true, gpu=false) in `crush-parallel/src/config.rs`
- [ ] T014 [P] Implement `ProgressEvent` struct, `ProgressPhase` enum (`Compressing`/`Decompressing`), and `ProgressCallback` type alias (`Box<dyn FnMut(ProgressEvent) -> bool + Send>`) in `crush-parallel/src/config.rs`
- [ ] T015 Run `cargo test` — verify all format roundtrip tests and config validation tests pass

**Checkpoint**: CRSH format layer complete — user story implementation can now begin.

---

## Phase 3: User Story 1 — Multi-Core CPU Compression (Priority: P1) 🎯 MVP

**Goal**: Compress input data (byte slice, writer, or stream) in parallel across all available CPU cores using rayon, producing CRSH-format output with per-block CRC32 checksums and a trailing block index. Includes progress callback and cancellation support.

**Independent Test**: Compress a multi-megabyte buffer with varying thread counts (1, 2, 4, 8) and verify: (1) decompressed output is byte-for-byte identical to input, (2) throughput scales with thread count, (3) incompressible data is stored raw.

### Tests for US1 (write first — must fail before implementation)

- [ ] T016 [US1] Write `test_compress_roundtrip_small` (compress `b"hello world".repeat(N)` → verify decompressible and identical) in `crush-parallel/src/engine.rs`
- [ ] T017 [P] [US1] Write `test_compress_incompressible_stored` (compress random/encrypted bytes → verify blocks have `stored` flag set and output size ≤ input size + overhead) in `crush-parallel/src/engine.rs`
- [ ] T018 [P] [US1] Write `test_compress_output_valid_crsh_format` (verify FileHeader magic, format_version, footer magic, IndexHeader entry_count matches actual block count) in `crush-parallel/src/engine.rs`
- [ ] T019 [P] [US1] Write `test_progress_callback_invoked_per_block` (count callback invocations, verify bytes_processed increases monotonically) in `crush-parallel/src/engine.rs`
- [ ] T020 [P] [US1] Write `test_cancel_halts_at_block_boundary` (callback returns false after N blocks → verify `CrushError::Cancelled` returned and no partial output) in `crush-parallel/src/engine.rs`

### Implementation for US1

- [ ] T021 [US1] Implement `compress_block()` in `crush-parallel/src/block.rs`: raw DEFLATE via `flate2::Compress::new(level, false)`, CRC32 via `crc32fast::hash()` on uncompressed data, store raw if `compressed_size / uncompressed_size > max_expansion_ratio`
- [ ] T022 [US1] Implement `compress(input: &[u8], config: &EngineConfiguration) -> Result<Vec<u8>, CrushError>` in `crush-parallel/src/engine.rs`: split input into blocks, `rayon::par_iter` dispatch, ordered block assembly, write FileHeader → BlockHeaders+payloads → IndexHeader+BlockIndexEntries → FileFooter
- [ ] T023 [P] [US1] Implement `compress_to_writer<W: Write>(input: &[u8], writer: W, config: &EngineConfiguration) -> Result<u64, CrushError>` in `crush-parallel/src/engine.rs`
- [ ] T024 [P] [US1] Implement `compress_stream<R: Read, W: Write>(reader: R, writer: W, config: &EngineConfiguration) -> Result<u64, CrushError>` in `crush-parallel/src/engine.rs` (sets uncompressed_size/block_count to `u64::MAX` in header, patches in footer)
- [ ] T025 [US1] Wire `AtomicCancellationToken` (from `crush-core::cancel`) and `ProgressCallback` into `compress()` via `rayon::try_for_each` + `ControlFlow::Break`; callback returning `false` sets the token and returns `CrushError::Cancelled` after all in-flight blocks complete in `crush-parallel/src/engine.rs`
- [ ] T026 [US1] Implement `indicatif` progress bar in `crush-cli/src/commands/compress.rs` wired to `crush_parallel::compress` (ProgressBar::new(total_bytes), set_position(event.bytes_processed) on each callback)
- [ ] T027 [US1] Create throughput criterion benchmark (thread counts 1, 2, 4, 8; block sizes 64KB, 512KB, 1MB) in `crush-parallel/benches/throughput.rs`
- [ ] T028 [US1] Run `cargo test` — verify all US1 tests pass; run `cargo bench` to capture throughput baseline

**Checkpoint**: US1 complete — multi-core CPU compression is fully functional and independently testable. Target: >500 MB/s @ 8 cores.

---

## Phase 4: User Story 2 — Parallel Decompression (Priority: P2)

**Goal**: Decompress CRSH files in parallel using the trailing block index. Each block is decompressed independently in parallel, enabling symmetric throughput to compression. Checksum validation halts cleanly at the first corrupt block.

**Independent Test**: Compress a file with US1 engine → decompress with varying thread counts → verify output is byte-for-byte identical to original. Corrupt one block → verify exactly that block is reported and decompression halts.

### Tests for US2 (write first — must fail before implementation)

- [ ] T029 [US2] Write `test_decompress_roundtrip` (compress then decompress, assert identical to input) in `crush-parallel/src/engine.rs`
- [ ] T030 [P] [US2] Write `test_decompress_corrupt_block_detected` (flip bits in one compressed block, verify `CrushError::ChecksumMismatch { block_index: N }` returned) in `crush-parallel/src/engine.rs`
- [ ] T031 [P] [US2] Write `test_version_mismatch_rejected` (craft FileFooter with wrong format_version, verify `CrushError::VersionMismatch` returned) in `crush-parallel/src/engine.rs`
- [ ] T032 [P] [US2] Write `test_expansion_limit_exceeded` (set max_decompression_ratio=0.001, verify `CrushError::ExpansionLimitExceeded` returned) in `crush-parallel/src/engine.rs`
- [ ] T033 [P] [US2] Write `test_truncated_footer_rejected` (truncate file to remove last 24 bytes, verify `CrushError::InvalidFormat` or `CrushError::IndexCorrupted`) in `crush-parallel/src/engine.rs`

### Implementation for US2

- [ ] T034 [US2] Implement `BlockIndex` struct (wrapping `Vec<BlockIndexEntry>`) with `len()`, `total_uncompressed_size()` in `crush-parallel/src/index.rs`
- [ ] T035 [US2] Implement `load_index<R: Read + Seek>(reader: &mut R) -> Result<BlockIndex, CrushError>` in `crush-parallel/src/index.rs`: seek to `file_size - 24`, read FileFooter, validate magic + format_version + footer_checksum, seek to `index_offset`, read IndexHeader + N BlockIndexEntry records
- [ ] T036 [US2] Implement `decompress(input: &[u8], config: &EngineConfiguration) -> Result<Vec<u8>, CrushError>` in `crush-parallel/src/engine.rs`: load index via `load_index`, parallel block decompression via `rayon::par_iter` over index entries, each block: seek to `entry.block_offset`, read BlockHeader + payload, DEFLATE decompress, verify CRC32 checksum
- [ ] T037 [P] [US2] Implement `decompress_from_reader<R: Read + Seek>(reader: R, config: &EngineConfiguration) -> Result<Vec<u8>, CrushError>` in `crush-parallel/src/engine.rs`
- [ ] T038 [US2] Implement per-block checksum validation (`ChecksumMismatch { block_index, expected, actual }`) and `ExpansionLimitExceeded { block_index }` check in decompression path in `crush-parallel/src/engine.rs`
- [ ] T039 [US2] Wire `AtomicCancellationToken` and `ProgressCallback` into `decompress()` loop (same pattern as compress — callback false → `CrushError::Cancelled`) in `crush-parallel/src/engine.rs`
- [ ] T040 [US2] Implement `indicatif` progress bar in `crush-cli/src/commands/decompress.rs` wired to `crush_parallel::decompress`
- [ ] T041 [US2] Run `cargo test` — verify all US2 tests pass

**Checkpoint**: US1 and US2 complete — symmetric parallel compression and decompression independently functional.

---

## Phase 5: User Story 3 — GPU-Accelerated Compression (Priority: P3)

**Goal**: Offload block compression to GPU via `wgpu` when `config.gpu = true` and a compatible adapter is present. Falls back silently to CPU if no adapter is found. GPU output is byte-for-byte identical to CPU output.

**Independent Test**: Compress the same input with CPU-only and GPU-enabled modes → verify outputs are identical (byte-for-byte) → verify GPU mode shows ≥20% throughput improvement on hardware with a supported GPU.

> **Note**: All GPU code is behind `#[cfg(feature = "gpu")]`. Default builds are unaffected. Tests run conditionally based on adapter availability.

### Tests for US3 (write first — must fail before implementation)

- [ ] T042 [US3] Write `test_gpu_produces_identical_output_to_cpu` (conditional: `#[cfg(feature = "gpu")]`, skip if `GpuWorker::new()` returns `None`) in `crush-parallel/src/gpu/mod.rs`
- [ ] T043 [P] [US3] Write `test_gpu_fallback_when_no_adapter` (mock no-adapter scenario, verify compress completes successfully via CPU) in `crush-parallel/src/gpu/mod.rs`

### Implementation for US3

- [ ] T044 [US3] Implement `GpuWorker::new() -> Option<GpuWorker>` in `crush-parallel/src/gpu/worker.rs`: `pollster::block_on(wgpu::Instance::request_adapter(...))`, returns `None` when no compatible adapter; device, queue, and pipeline initialization on success
- [ ] T045 [US3] Implement WGSL compute shader for parallel block compression (GDeflate-derived algorithm, one workgroup per block) in `crush-parallel/src/gpu/shaders/deflate.wgsl`
- [ ] T046 [US3] Implement `GpuWorker::compress_block(&self, input: &[u8]) -> Result<Vec<u8>, CrushError>` in `crush-parallel/src/gpu/worker.rs`: write input buffer, dispatch compute, `device.poll(PollType::Wait)` for synchronous readback, return compressed bytes
- [ ] T047 [US3] Wire `GpuWorker` into `compress()` in `crush-parallel/src/engine.rs`: when `config.gpu = true` and `GpuWorker::new()` returns `Some(worker)`, dispatch blocks to GPU; on GPU error mid-compression, fall back to CPU for remaining blocks (log at debug level)
- [ ] T048 [US3] Run `cargo test --features gpu` — verify GPU tests pass (auto-skip if no adapter on CI)

**Checkpoint**: US3 complete — GPU acceleration available as opt-in feature with automatic CPU fallback.

---

## Phase 6: User Story 4 — Seekable Random Access (Priority: P4)

**Goal**: Decompress a single block by index in O(1) time (one seek + one read), without reading or decompressing any other block. Enables analytics workloads that need specific byte ranges from large compressed files.

**Independent Test**: Compress a known multi-block dataset → request block N via `decompress_block()` → verify output matches the original N-th block slice → verify no other block offsets were read.

### Tests for US4 (write first — must fail before implementation)

- [ ] T049 [US4] Write `test_decompress_block_n` (compress multi-block data, decompress block 0, middle, last — each independently, verify correct slice) in `crush-parallel/src/index.rs`
- [ ] T050 [P] [US4] Write `test_block_for_offset` (verify `BlockIndex::block_for_offset(offset)` returns correct block index for known offsets) in `crush-parallel/src/index.rs`
- [ ] T051 [P] [US4] Write `test_random_access_does_not_read_other_blocks` (instrument reader with a read counter, call `decompress_block()`, verify ≤2 seeks/reads beyond index load) in `crush-parallel/src/index.rs`

### Implementation for US4

- [ ] T052 [US4] Implement `BlockIndex::uncompressed_offset(block_n: u64) -> u64` (cumulative sum of preceding `uncompressed_size` values) in `crush-parallel/src/index.rs`
- [ ] T053 [P] [US4] Implement `BlockIndex::block_for_offset(uncompressed_offset: u64) -> Option<u64>` (binary search over cumulative uncompressed sizes) in `crush-parallel/src/index.rs`
- [ ] T054 [US4] Implement `decompress_block<R: Read + Seek>(reader: &mut R, block_index: &BlockIndex, block_n: u64, config: &EngineConfiguration) -> Result<Vec<u8>, CrushError>` in `crush-parallel/src/index.rs`: seek to `entry[block_n].block_offset`, read BlockHeader, validate sizes, read payload, DEFLATE decompress, verify checksum
- [ ] T055 [US4] Add `--block N` flag to `crush-cli/src/commands/decompress.rs` that calls `load_index()` + `decompress_block()` for single-block random access
- [ ] T056 [US4] Create random access criterion benchmark (load_index + decompress_block latency for first/middle/last block on a large synthetic file) in `crush-parallel/benches/random_access.rs`
- [ ] T057 [US4] Run `cargo test` — verify all US4 tests pass; run `cargo bench` to verify random access latency target (<100 ms for last block on large file)

**Checkpoint**: All four user stories complete. Full API functional: compress, decompress, GPU opt-in, random access.

---

## Phase 7: Plugin Registration, Fuzz Testing & Polish

**Purpose**: Wire the linkme plugin registration, add fuzz targets, proptest round-trip, documentation, and enforce all quality gates.

- [ ] T058 Register `crush-parallel` as a `CompressionPlugin` via `linkme` distributed slice (`#[crush_core::plugin::register]` static) in `crush-parallel/src/lib.rs`
- [ ] T059 [P] Create `crush-parallel/fuzz/Cargo.toml` for `cargo-fuzz` setup with `fuzz_decompress` and `fuzz_roundtrip` targets
- [ ] T060 Create `fuzz_decompress` target: arbitrary bytes → `decompress()` → must not panic (verify only `Err(...)` returned, never `panic`) in `crush-parallel/fuzz/fuzz_targets/fuzz_decompress.rs`
- [ ] T061 [P] Create `fuzz_roundtrip` target: random data → `compress()` → `decompress()` → assert byte-for-byte identical to input in `crush-parallel/fuzz/fuzz_targets/fuzz_roundtrip.rs`
- [ ] T062 Add `proptest` round-trip property test (arbitrary `Vec<u8>` input, all block sizes 64KB–4MB, levels 0/6/9 → compress → decompress → identical) in `crush-parallel/src/engine.rs`
- [ ] T063 [P] Add `///` doc comments and `# Example` sections to all public API items in `crush-parallel/src/lib.rs`, `engine.rs`, `index.rs`, `config.rs`, `format.rs`
- [ ] T064 Run `cargo doc --no-deps` — verify zero documentation warnings
- [ ] T065 Run `cargo clippy --all-targets -- -D warnings` — fix all warnings across `crush-parallel`, `crush-core`, `crush-cli`
- [ ] T066 Run `cargo fmt --all -- --check` — fix any formatting issues
- [ ] T067 Run `cargo test` — verify complete test suite (all phases) passes with zero failures
- [ ] T068 Run `cargo fuzz run fuzz_decompress -- -runs=100000` in `crush-parallel/fuzz`
- [ ] T069 [P] Run `cargo fuzz run fuzz_roundtrip -- -runs=100000` in `crush-parallel/fuzz`

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: No dependencies — start immediately
- **Phase 2 (Foundational)**: Depends on Phase 1 completion — **BLOCKS all user stories**
- **Phase 3 (US1)**: Depends on Phase 2 completion — no dependency on US2/US3/US4
- **Phase 4 (US2)**: Depends on Phase 2 completion — independently testable, does not require US1 output (can compress a fixture in the test)
- **Phase 5 (US3)**: Depends on Phase 3 (US1) — GPU compression builds on CPU block model
- **Phase 6 (US4)**: Depends on Phase 4 (US2) — random access uses `load_index()` implemented in US2
- **Phase 7 (Polish)**: Depends on all story phases

### User Story Dependencies

| Story | Depends On | Can Start After |
|-------|-----------|----------------|
| US1 (P1) | Phase 2 complete | Foundational phase |
| US2 (P2) | Phase 2 complete | Foundational phase (independently) |
| US3 (P3) | US1 complete | Phase 3 complete |
| US4 (P4) | US2 complete | Phase 4 complete |

### Within Each User Story

1. Tests MUST be written and verified to FAIL before implementation begins
2. Format/struct definitions before logic that uses them
3. Core compress/decompress logic before CLI wiring
4. Benchmarks after core implementation (capture baseline before optimization)

### Parallel Opportunities

- T010, T011, T012 (format structs) can run in parallel within Phase 2
- T017, T018, T019, T020 (US1 tests) can be written in parallel
- T023, T024 (`compress_to_writer`, `compress_stream`) can run in parallel after T022
- T030, T031, T032, T033 (US2 tests) can be written in parallel
- T037 (`decompress_from_reader`) can run in parallel after T036
- T050, T051 (US4 tests) can be written in parallel
- T060, T061 (fuzz targets) can be created in parallel
- T068, T069 (fuzz runs) can run in parallel

---

## Parallel Execution Examples

### Phase 2 — Format Structs (run together after T009)

```
T010: Implement BlockHeader in crush-parallel/src/format.rs
T011: Implement BlockIndexEntry in crush-parallel/src/format.rs
T012: Implement IndexHeader + FileFooter in crush-parallel/src/format.rs
```

### Phase 3 — US1 Test Writing (run together)

```
T016: test_compress_roundtrip_small
T017: test_compress_incompressible_stored
T018: test_compress_output_valid_crsh_format
T019: test_progress_callback_invoked_per_block
T020: test_cancel_halts_at_block_boundary
```

### Phase 4 — US2 Test Writing (run together)

```
T029: test_decompress_roundtrip
T030: test_decompress_corrupt_block_detected
T031: test_version_mismatch_rejected
T032: test_expansion_limit_exceeded
T033: test_truncated_footer_rejected
```

---

## Implementation Strategy

### MVP First (US1 Only — Phases 1–3)

1. Complete Phase 1: Workspace setup
2. Complete Phase 2: CRSH format layer
3. Complete Phase 3: US1 — CPU parallel compression with progress
4. **STOP and VALIDATE**: `cargo test`, `cargo bench` — confirm >500 MB/s @ 8 cores
5. Demo: compress a real file via `crush-cli` and verify output

### Incremental Delivery

1. Setup + Foundational → CRSH format layer ready
2. US1 → Parallel compression works → benchmark baseline captured
3. US2 → Parallel decompression works → full roundtrip benchmarked
4. US3 → GPU acceleration (optional hardware dependency — skip if no GPU available)
5. US4 → Random access → analytics use cases unlocked
6. Polish → Quality gates pass → ready for merge

### Quality Gates (must all pass before merge)

- [ ] `cargo test` — zero failures
- [ ] `cargo clippy --all-targets -- -D warnings` — zero warnings
- [ ] `cargo fmt --all -- --check` — clean
- [ ] `cargo doc --no-deps` — zero warnings
- [ ] `cargo bench` — no regression vs baseline (< 5% slowdown)
- [ ] Fuzz: `fuzz_decompress` + `fuzz_roundtrip` — 100k iterations each, no panics
- [ ] SC-001: >500 MB/s @ 8 cores (1 MB blocks, default level)
- [ ] SC-004: <100 ms random access on last block of ≥1 GB file
- [ ] SC-007: 100% byte-for-byte roundtrip fidelity across all paths

---

## Notes

- `[P]` tasks operate on different files or independent data — safe to parallelize
- TDD: red → green → refactor strictly enforced. Do not write implementation before tests fail.
- GPU tests auto-skip when no compatible adapter is present (do not fail CI)
- All production code: no `.unwrap()`, no `.expect()` — use `?` throughout
- Commit after each logical group (at minimum: after each checkpoint)
- `crush-parallel` must have zero compile-time dependency on `crush-cli`
- `crush-core` must have zero compile-time dependency on `crush-parallel`
