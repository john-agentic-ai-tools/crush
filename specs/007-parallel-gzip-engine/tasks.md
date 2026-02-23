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

**Purpose**: Initialize the `crush-parallel` crate within the existing Cargo workspace, create the new `CrushError` variants required by all subsequent stories, stub the two new modules that will grow in later phases, and configure mandatory pre-commit hooks.

- [X] T001 Add `crush-parallel` to `members[]` array in workspace `Cargo.toml`
- [X] T002 Create `crush-parallel/Cargo.toml` with all dependencies: `rayon`, `flate2`, `crc32fast`, `memmap2`, `thiserror`, `linkme` (workspace deps); optional feature `gpu`: `wgpu`, `pollster`
- [X] T003 [P] Add `VersionMismatch`, `InvalidFormat`, `ChecksumMismatch`, `ExpansionLimitExceeded`, `IndexCorrupted` variants and `is_cancelled()` helper method to `crush-core/src/error.rs`
- [X] T004 [P] Create stub `crush-parallel/src/lib.rs` declaring all public module paths (`engine`, `block`, `format`, `index`, `config`, `gpu`) and public re-exports; include `linkme` plugin registration stub (`#[crush_core::plugin::register] static PARALLEL_DEFLATE_PLUGIN`) with placeholder function pointers (FR-015)
- [X] T005 [P] Create stub `crush-cli/src/algorithm.rs` with `pub const DEFAULT_PARALLEL_THRESHOLD_BYTES: u64 = 25 * 1024 * 1024` and `pub fn select_algorithm(input_size: Option<u64>, explicit: Option<&str>, threshold: u64) -> &'static str` — body returns `"default"` until T031 fills in the logic (FR-016)
- [X] T006 Configure `cargo-husky` pre-commit hooks in `.cargo-husky/hooks/pre-commit`: `cargo fmt --check` and `cargo clippy --quiet` (constitution MANDATORY — must be committed so hooks activate for all contributors)
- [X] T007 Run `cargo build` to verify the workspace compiles cleanly with the new crate skeleton

---

## Phase 2: Foundational — CRSH File Format & Configuration

**Purpose**: Implement the CRSH binary format serialization layer, the `EngineConfiguration` builder, and the algorithm-selection test. These are hard prerequisites for **all** user stories — no compression or decompression logic can be implemented until this phase is complete.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

### Tests (write first — must fail before implementation)

- [X] T008 Write `test_file_header_roundtrip` property test (serialize → deserialize → assert equal, including magic rejection and version mismatch detection) in `crush-parallel/src/format.rs`
- [X] T009 [P] Write `test_block_header_roundtrip`, `test_block_index_entry_roundtrip`, `test_index_header_roundtrip`, and `test_file_footer_roundtrip` property tests in `crush-parallel/src/format.rs`
- [X] T010 [P] Write `test_engine_configuration_builder_validates_fields` (invalid block_size, invalid level, zero ratios) in `crush-parallel/src/config.rs`
- [X] T011 [P] Write `test_select_algorithm_auto_selects_parallel_above_threshold`, `test_select_algorithm_explicit_override`, and `test_select_algorithm_streaming_uses_parallel` in `crush-cli/src/algorithm.rs` — these test the stub from T005 and will fail until T031

### Implementation

- [X] T012 Implement `FileHeader` struct (magic `[u8;4]`, format_version `u32`, engine_version `EngineVersion` 8B, block_size `u32`, compression_level `u8`, flags `FileFlags` 1B, reserved 2B, uncompressed_size `u64`, block_count `u64`, _reserved 24B = 64B total) with `to_bytes()` / `from_bytes()` in `crush-parallel/src/format.rs`
- [X] T013 [P] Implement `BlockHeader` struct (compressed_size `u32`, uncompressed_size `u32`, checksum `u32`, flags `BlockFlags` 1B, _reserved 3B = 16B total) with `to_bytes()` / `from_bytes()` in `crush-parallel/src/format.rs`
- [X] T014 [P] Implement `BlockIndexEntry` struct (block_offset `u64`, compressed_size `u32`, uncompressed_size `u32`, checksum `u32` = 20B total) with `to_bytes()` / `from_bytes()` in `crush-parallel/src/format.rs`
- [X] T015 [P] Implement `IndexHeader` struct (entry_count `u32`, index_flags `u32` = 8B) and `FileFooter` struct (index_offset `u64`, index_size `u32`, footer_checksum `u32`, format_version `u32`, magic `[u8;4]` = 24B total) with `to_bytes()` / `from_bytes()` in `crush-parallel/src/format.rs`
- [X] T016 Implement `EngineConfiguration` with builder pattern (`EngineConfigurationBuilder`), field validation in `build()` (block_size in [65536, 268435456], level in [0, 9], ratios > 0.0), and `Default` impl (1 MB blocks, level 6, workers=0, checksums=true, gpu=false) in `crush-parallel/src/config.rs`
- [X] T017 [P] Implement `ProgressEvent` struct, `ProgressPhase` enum (`Compressing`/`Decompressing`), and `ProgressCallback` type alias (`Box<dyn FnMut(ProgressEvent) -> bool + Send>`) in `crush-parallel/src/config.rs`
- [X] T018 Run `cargo test` — verify all format roundtrip tests and config validation tests pass (algorithm tests from T011 will still fail — expected)

**Checkpoint**: CRSH format layer complete — user story implementation can now begin.

---

## Phase 3: User Story 1 — Multi-Core CPU Compression (Priority: P1) 🎯 MVP

**Goal**: Compress input data (byte slice, file path, or stream) in parallel across all available CPU cores using rayon, producing CRSH-format output with per-block CRC32 checksums and a trailing block index. Includes `compress_file()` zero-copy entry point, progress callback, cancellation, and automatic CLI algorithm selection for inputs ≥ 25 MB.

**Independent Test**: Compress a multi-megabyte buffer and a temporary file with varying thread counts (1, 2, 4, 8) and verify: (1) decompressed output is byte-for-byte identical to input, (2) throughput scales with thread count, (3) incompressible data is stored raw, (4) crush-cli selects parallel-deflate automatically for files ≥ 25 MB.

### Tests for US1 (write first — must fail before implementation)

- [X] T019 [US1] Write `test_compress_roundtrip_small` (compress `b"hello world".repeat(N)` → verify decompressible and identical) in `crush-parallel/src/engine.rs`
- [X] T020 [P] [US1] Write `test_compress_incompressible_stored` (compress random/encrypted bytes → verify blocks have `stored` flag set and output size ≤ input size + overhead) in `crush-parallel/src/engine.rs`
- [X] T021 [P] [US1] Write `test_compress_output_valid_crsh_format` (verify FileHeader magic, format_version, footer magic, IndexHeader entry_count matches actual block count) in `crush-parallel/src/engine.rs`
- [X] T022 [P] [US1] Write `test_progress_callback_invoked_per_block` (count callback invocations, verify bytes_processed increases monotonically) in `crush-parallel/src/engine.rs`
- [X] T023 [P] [US1] Write `test_cancel_halts_at_block_boundary` (callback returns false after N blocks → verify `CrushError::Cancelled` returned and no partial output) in `crush-parallel/src/engine.rs`
- [X] T024 [P] [US1] Write `test_compress_file_roundtrip` (write known data to a temp file, call `compress_file(path, &config)`, decompress result, assert byte-for-byte identical to original) in `crush-parallel/src/engine.rs`

### Implementation for US1

- [X] T025 [US1] Implement `compress_block()` in `crush-parallel/src/block.rs`: raw DEFLATE via `flate2::Compress::new(level, false)`, CRC32 via `crc32fast::hash()` on uncompressed data, store raw if `compressed_size / uncompressed_size > max_expansion_ratio`
- [X] T026 [US1] Implement `compress(input: &[u8], config: &EngineConfiguration) -> Result<Vec<u8>, CrushError>` in `crush-parallel/src/engine.rs`: split input into blocks, `rayon::par_iter` dispatch, ordered block assembly, write FileHeader → BlockHeaders+payloads → IndexHeader+BlockIndexEntries → FileFooter
- [X] T027 [P] [US1] Implement `compress_to_writer<W: Write>(input: &[u8], writer: W, config: &EngineConfiguration) -> Result<u64, CrushError>` in `crush-parallel/src/engine.rs`
- [X] T028 [P] [US1] Implement `compress_stream<R: Read, W: Write>(reader: R, writer: W, config: &EngineConfiguration) -> Result<u64, CrushError>` in `crush-parallel/src/engine.rs` (sets uncompressed_size/block_count to `u64::MAX` in header, patches in footer)
- [X] T029 [US1] Wire `AtomicCancellationToken` (from `crush-core::cancel`) and `ProgressCallback` into `compress()` via `rayon::try_for_each` + `ControlFlow::Break`; callback returning `false` sets the token and returns `CrushError::Cancelled` after all in-flight blocks complete in `crush-parallel/src/engine.rs`
- [X] T030 [US1] Implement `compress_file(path: &Path, config: &EngineConfiguration) -> Result<Vec<u8>, CrushError>` in `crush-parallel/src/engine.rs` using `memmap2::MmapOptions::new().map(&file)` for zero-copy read, then calling `compress(mmap_slice, config)` (FR-009)
- [X] T031 [US1] Implement full `select_algorithm()` body in `crush-cli/src/algorithm.rs`: return explicit arg if set, return `"parallel-deflate"` when `input_size.is_none()` (streaming) or `input_size >= threshold`, otherwise return `"default"`; add `--algorithm` and `--parallel-threshold` flags to `crush-cli/src/args.rs` (FR-016)
- [X] T032 [US1] Wire `select_algorithm()` into `crush-cli/src/commands/compress.rs` and implement `indicatif` progress bar (`ProgressBar::new(total_bytes)`, `set_position` per callback, log selected algorithm when `--verbose`)
- [X] T033 [US1] Create compression throughput criterion benchmark (thread counts 1, 2, 4, 8; block sizes 64KB, 512KB, 1MB) in `crush-parallel/benches/throughput.rs`
- [X] T034 [US1] Run `cargo test` — verify all US1 tests and algorithm-selection tests (T011) pass; run `cargo bench --bench throughput` to capture compression baseline

**Checkpoint**: US1 complete — multi-core CPU compression and auto-selection are fully functional and independently testable. Target: >500 MB/s @ 8 cores.

---

## Phase 4: User Story 2 — Parallel Decompression (Priority: P2)

**Goal**: Decompress CRSH files in parallel using the trailing block index. Each block is decompressed independently in parallel, enabling symmetric throughput to compression. Checksum validation halts cleanly at the first corrupt block.

**Independent Test**: Compress a file with US1 engine → decompress with varying thread counts → verify output is byte-for-byte identical to original. Corrupt one block → verify exactly that block is reported and decompression halts.

### Tests for US2 (write first — must fail before implementation)

- [X] T035 [US2] Write `test_decompress_roundtrip` (compress then decompress, assert identical to input) in `crush-parallel/src/engine.rs`
- [X] T036 [P] [US2] Write `test_decompress_corrupt_block_detected` (flip bits in one compressed block, verify `CrushError::ChecksumMismatch { block_index: N }` returned) in `crush-parallel/src/engine.rs`
- [X] T037 [P] [US2] Write `test_version_mismatch_rejected` (craft FileFooter with wrong format_version, verify `CrushError::VersionMismatch` returned) in `crush-parallel/src/engine.rs`
- [X] T038 [P] [US2] Write `test_expansion_limit_exceeded` (set max_decompression_ratio=0.001, verify `CrushError::ExpansionLimitExceeded` returned) in `crush-parallel/src/engine.rs`
- [X] T039 [P] [US2] Write `test_truncated_footer_rejected` (truncate file to remove last 24 bytes, verify `CrushError::InvalidFormat` or `CrushError::IndexCorrupted`) in `crush-parallel/src/engine.rs`

### Implementation for US2

- [X] T040 [US2] Implement `BlockIndex` struct (wrapping `Vec<BlockIndexEntry>`) with `len()`, `total_uncompressed_size()` in `crush-parallel/src/index.rs`
- [X] T041 [US2] Implement `load_index<R: Read + Seek>(reader: &mut R) -> Result<BlockIndex, CrushError>` in `crush-parallel/src/index.rs`: seek to `file_size - 24`, read FileFooter, validate magic + format_version + footer_checksum, seek to `index_offset`, read IndexHeader + N BlockIndexEntry records
- [X] T042 [US2] Implement `decompress(input: &[u8], config: &EngineConfiguration) -> Result<Vec<u8>, CrushError>` in `crush-parallel/src/engine.rs`: load index via `load_index`, parallel block decompression via `rayon::par_iter` over index entries, each block: seek to `entry.block_offset`, read BlockHeader + payload, DEFLATE decompress, verify CRC32 checksum
- [X] T043 [P] [US2] Implement `decompress_from_reader<R: Read + Seek>(reader: R, config: &EngineConfiguration) -> Result<Vec<u8>, CrushError>` in `crush-parallel/src/engine.rs`
- [X] T044 [US2] Implement per-block checksum validation (`ChecksumMismatch { block_index, expected, actual }`) and `ExpansionLimitExceeded { block_index }` check in decompression path in `crush-parallel/src/engine.rs`
- [X] T045 [US2] Wire `AtomicCancellationToken` and `ProgressCallback` into `decompress()` loop (same pattern as compress — callback false → `CrushError::Cancelled`) in `crush-parallel/src/engine.rs`
- [X] T046 [US2] Implement `indicatif` progress bar in `crush-cli/src/commands/decompress.rs` wired to `crush_parallel::decompress`
- [X] T047 [US2] Add decompression throughput criterion benchmark (thread counts 1, 2, 4, 8 on a pre-compressed CRSH fixture) to `crush-parallel/benches/throughput.rs`; target: within 20% of compression throughput (SC-003)
- [X] T048 [US2] Run `cargo test` — verify all US2 tests pass; run `cargo bench --bench throughput` to capture decompression baseline

**Checkpoint**: US1 and US2 complete — symmetric parallel compression and decompression independently functional.

---

## Phase 5: User Story 3 — GPU-Accelerated Compression (Priority: P3)

**Goal**: Offload block compression to GPU via `wgpu` when `config.gpu = true` and a compatible adapter is present. Falls back silently to CPU if no adapter is found. GPU output is byte-for-byte identical to CPU output.

**Independent Test**: Compress the same input with CPU-only and GPU-enabled modes → verify outputs are identical (byte-for-byte) → verify GPU mode shows ≥20% throughput improvement on hardware with a supported GPU.

> **Note**: All GPU code is behind `#[cfg(feature = "gpu")]`. Default builds are unaffected. Tests run conditionally based on adapter availability.

### Tests for US3 (write first — must fail before implementation)

- [X] T049 [US3] Write `test_gpu_produces_identical_output_to_cpu` (conditional: `#[cfg(feature = "gpu")]`, skip if `GpuWorker::new()` returns `None`) in `crush-parallel/src/gpu/mod.rs`
- [X] T050 [P] [US3] Write `test_gpu_fallback_when_no_adapter` (mock no-adapter scenario, verify compress completes successfully via CPU) in `crush-parallel/src/gpu/mod.rs`

### Implementation for US3

- [X] T051 [US3] Implement `GpuWorker::new() -> Option<GpuWorker>` in `crush-parallel/src/gpu/worker.rs`: `pollster::block_on(wgpu::Instance::request_adapter(...))`, returns `None` when no compatible adapter; device, queue, and pipeline initialization on success
- [X] T052 [US3] Implement WGSL compute shader for parallel block compression (GDeflate-derived algorithm, one workgroup per block) in `crush-parallel/src/gpu/shaders/deflate.wgsl`
- [X] T053 [US3] Implement `GpuWorker::compress_block(&self, input: &[u8]) -> Result<Vec<u8>, CrushError>` in `crush-parallel/src/gpu/worker.rs`: write input buffer, dispatch compute, `device.poll(PollType::Wait)` for synchronous readback, return compressed bytes
- [X] T054 [US3] Wire `GpuWorker` into `compress()` in `crush-parallel/src/engine.rs`: when `config.gpu = true` and `GpuWorker::new()` returns `Some(worker)`, dispatch blocks to GPU; on GPU error mid-compression, fall back to CPU for remaining blocks (log at debug level)
- [X] T055 [US3] Add GPU vs CPU throughput criterion benchmark (compress same input with `config.gpu=false` then `config.gpu=true`, record both; benchmark auto-skips when no GPU adapter is found) in `crush-parallel/benches/throughput.rs`
- [X] T056 [US3] Run `cargo test --features gpu` — verify GPU tests pass (auto-skip if no adapter on CI)

**Checkpoint**: US3 complete — GPU acceleration available as opt-in feature with automatic CPU fallback.

---

## Phase 6: User Story 4 — Seekable Random Access (Priority: P4)

**Goal**: Decompress a single block by index in O(1) time (one seek + one read), without reading or decompressing any other block. Enables analytics workloads that need specific byte ranges from large compressed files.

**Independent Test**: Compress a known multi-block dataset → request block N via `decompress_block()` → verify output matches the original N-th block slice → verify no other block offsets were read.

### Tests for US4 (write first — must fail before implementation)

- [X] T057 [US4] Write `test_decompress_block_n` (compress multi-block data, decompress block 0, middle, last — each independently, verify correct slice) in `crush-parallel/src/index.rs`
- [X] T058 [P] [US4] Write `test_block_for_offset` (verify `BlockIndex::block_for_offset(offset)` returns correct block index for known offsets) in `crush-parallel/src/index.rs`
- [X] T059 [P] [US4] Write `test_random_access_does_not_read_other_blocks` (instrument reader with a read counter, call `decompress_block()`, verify ≤2 seeks/reads beyond index load) in `crush-parallel/src/index.rs`

### Implementation for US4

- [X] T060 [US4] Implement `BlockIndex::uncompressed_offset(block_n: u64) -> u64` (cumulative sum of preceding `uncompressed_size` values) in `crush-parallel/src/index.rs`
- [X] T061 [P] [US4] Implement `BlockIndex::block_for_offset(uncompressed_offset: u64) -> Option<u64>` (binary search over cumulative uncompressed sizes) in `crush-parallel/src/index.rs`
- [X] T062 [US4] Implement `decompress_block<R: Read + Seek>(reader: &mut R, block_index: &BlockIndex, block_n: u64, config: &EngineConfiguration) -> Result<Vec<u8>, CrushError>` in `crush-parallel/src/index.rs`: seek to `entry[block_n].block_offset`, read BlockHeader, validate sizes, read payload, DEFLATE decompress, verify checksum
- [X] T063 [US4] Add `--block N` flag to `crush-cli/src/commands/decompress.rs` that calls `load_index()` + `decompress_block()` for single-block random access
- [X] T064 [US4] Create random access criterion benchmark (load_index + decompress_block latency for first/middle/last block on a large synthetic file) in `crush-parallel/benches/random_access.rs`
- [X] T065 [US4] Run `cargo test` — verify all US4 tests pass; run `cargo bench --bench random_access` to verify random access latency target (<100 ms for last block on a large file)

**Checkpoint**: All four user stories complete. Full API functional: compress, decompress, GPU opt-in, random access.

---

## Phase 7: Plugin Registration, Fuzz Testing & Polish

**Purpose**: Complete the linkme plugin registration with real function pointers, add fuzz targets, proptest round-trip, documentation, and enforce all constitution quality gates including code coverage and benchmark verification.

- [X] T066 Complete `PARALLEL_DEFLATE_PLUGIN` registration in `crush-parallel/src/lib.rs`: replace placeholder function pointers from T004 with the real `parallel_compress_fn` and `parallel_decompress_fn` wrappers (requires US1 and US2 complete)
- [X] T067 [P] Create `crush-parallel/fuzz/Cargo.toml` for `cargo-fuzz` setup with `fuzz_decompress` and `fuzz_roundtrip` targets
- [X] T068 Create `fuzz_decompress` target: arbitrary bytes → `decompress()` → must not panic (verify only `Err(...)` returned, never `panic`) in `crush-parallel/fuzz/fuzz_targets/fuzz_decompress.rs`
- [X] T069 [P] Create `fuzz_roundtrip` target: random data → `compress()` → `decompress()` → assert byte-for-byte identical to input in `crush-parallel/fuzz/fuzz_targets/fuzz_roundtrip.rs`
- [X] T070 Add `proptest` round-trip property test (arbitrary `Vec<u8>` input, all block sizes 64KB–4MB, levels 0/6/9 → compress → decompress → identical) in `crush-parallel/src/engine.rs`
- [X] T071 [P] Add `///` doc comments and `# Example` sections to all public API items in `crush-parallel/src/lib.rs`, `engine.rs`, `index.rs`, `config.rs`, `format.rs`
- [X] T072 Run `cargo doc --no-deps` — verify zero documentation warnings
- [X] T073 Run `cargo clippy --all-targets -- -D warnings` — fix all warnings across `crush-parallel`, `crush-core`, `crush-cli`
- [X] T074 SC-006 size comparison: compress a 100 MB compressible text file with both `crush-parallel` (level 6) and system `gzip -6`; assert crush-parallel output size ≤ gzip output size × 1.05 (within 5%)
- [X] T075 Run `cargo tarpaulin --all-features -- --test-threads=1` and verify code coverage > 80% across `crush-parallel` and `crush-core` (constitution quality gate — MANDATORY)
- [X] T076 Manual benchmark verification: run `cargo bench` and confirm SC-001 (>500 MB/s @ 8 cores, 1MB blocks), SC-003 (decompress within 20% of compress throughput), SC-004 (<100 ms random access on last block of ≥1 GB file); document results in a comment in `crush-parallel/benches/throughput.rs`
- [X] T077 Run `cargo fmt --all -- --check` — fix any formatting issues
- [X] T078 Run `cargo test` — verify complete test suite (all phases) passes with zero failures
- [X] T079 Run `cargo fuzz run fuzz_decompress -- -runs=100000` in `crush-parallel/fuzz`
- [X] T080 [P] Run `cargo fuzz run fuzz_roundtrip -- -runs=100000` in `crush-parallel/fuzz`
- [X] T081 Verify FR-016: compress a 50 MB test file via `crush-cli` without `--algorithm` flag, verify verbose output confirms `"parallel-deflate"` was selected; verify a 10 MB file uses the default algorithm

---

## Phase 8: Post-MVP Cleanup (Constitution MANDATORY)

**Purpose**: Run the mandatory post-MVP duplicate detection and cleanup pass required by the project constitution. Every feature must complete this phase before merge.

- [X] T082 Run `.specify/scripts/powershell/detect-duplicates.ps1` targeting `crush-parallel/src/` and identify any code patterns longer than 20 lines; output findings to `specs/007-parallel-gzip-engine/duplication-report.json`
- [X] T083 Refactor any duplicated code patterns > 20 lines found in T082 into shared helpers, traits, or utility functions within `crush-parallel/src/`; re-run `cargo test` to verify no regressions
- [X] T084 Create `specs/007-parallel-gzip-engine/cleanup-summary.md` documenting: number of duplications found, what was refactored (or why nothing needed refactoring), and final status; move `duplication-report.json` to `specs/007-parallel-gzip-engine/duplication-report.json` for archival

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
- **Phase 8 (Cleanup)**: Depends on Phase 7 completion — runs after all code is stable

### User Story Dependencies

| Story    | Depends On       | Can Start After                    |
|----------|------------------|------------------------------------|
| US1 (P1) | Phase 2 complete | Foundational phase                 |
| US2 (P2) | Phase 2 complete | Foundational phase (independently) |
| US3 (P3) | US1 complete     | Phase 3 complete                   |
| US4 (P4) | US2 complete     | Phase 4 complete                   |

### Within Each User Story

1. Tests MUST be written and verified to FAIL before implementation begins
2. Format/struct definitions before logic that uses them
3. Core compress/decompress logic before CLI wiring
4. Benchmarks after core implementation (capture baseline before optimization)

### Parallel Opportunities

- T013, T014, T015 (format structs) can run in parallel within Phase 2
- T019, T020, T021, T022, T023, T024 (US1 tests) can be written in parallel
- T027, T028 (`compress_to_writer`, `compress_stream`) can run in parallel after T026
- T036, T037, T038, T039 (US2 tests) can be written in parallel
- T043 (`decompress_from_reader`) can run in parallel after T042
- T058, T059 (US4 tests) can be written in parallel
- T068, T069 (fuzz targets) can be created in parallel
- T079, T080 (fuzz runs) can run in parallel

---

## Parallel Execution Examples

### Phase 2 — Format Structs (run together after T012)

```text
T013: Implement BlockHeader in crush-parallel/src/format.rs
T014: Implement BlockIndexEntry in crush-parallel/src/format.rs
T015: Implement IndexHeader + FileFooter in crush-parallel/src/format.rs
```

### Phase 3 — US1 Test Writing (run together)

```text
T019: test_compress_roundtrip_small
T020: test_compress_incompressible_stored
T021: test_compress_output_valid_crsh_format
T022: test_progress_callback_invoked_per_block
T023: test_cancel_halts_at_block_boundary
T024: test_compress_file_roundtrip
```

### Phase 4 — US2 Test Writing (run together)

```text
T035: test_decompress_roundtrip
T036: test_decompress_corrupt_block_detected
T037: test_version_mismatch_rejected
T038: test_expansion_limit_exceeded
T039: test_truncated_footer_rejected
```

---

## Implementation Strategy

### MVP First (US1 Only — Phases 1–3)

1. Complete Phase 1: Workspace setup + cargo-husky hooks
2. Complete Phase 2: CRSH format layer
3. Complete Phase 3: US1 — CPU parallel compression with progress and auto-selection
4. **STOP and VALIDATE**: `cargo test`, `cargo bench` — confirm >500 MB/s @ 8 cores; confirm crush-cli selects parallel-deflate for a 50 MB test file automatically
5. Demo: compress a real file via `crush-cli` and verify output

### Incremental Delivery

1. Setup + Foundational → CRSH format layer ready
2. US1 → Parallel compression + auto-selection works → benchmark baseline captured
3. US2 → Parallel decompression works → full roundtrip benchmarked
4. US3 → GPU acceleration (optional hardware dependency — skip if no GPU available)
5. US4 → Random access → analytics use cases unlocked
6. Polish → Quality gates pass → ready for merge
7. Cleanup → Duplication detection → ready for merge

### Quality Gates (must all pass before merge)

- [ ] `cargo test` — zero failures
- [ ] `cargo clippy --all-targets -- -D warnings` — zero warnings
- [ ] `cargo fmt --all -- --check` — clean
- [ ] `cargo doc --no-deps` — zero documentation warnings
- [ ] `cargo tarpaulin` — code coverage > 80% (MANDATORY per constitution)
- [ ] `cargo bench` — no regression vs baseline (< 5% slowdown)
- [ ] Fuzz: `fuzz_decompress` + `fuzz_roundtrip` — 100k iterations each, no panics (MANDATORY per constitution)
- [ ] SC-001: >500 MB/s @ 8 cores (1 MB blocks, default level) — verified in T076
- [ ] SC-003: decompression throughput within 20% of compression throughput — verified in T076
- [ ] SC-004: <100 ms random access on last block of ≥1 GB file — verified in T076
- [ ] SC-006: crush-parallel output within 5% of gzip output size at same level — verified in T074
- [ ] SC-007: 100% byte-for-byte roundtrip fidelity across all paths (CPU, GPU, stored blocks)
- [ ] FR-016: crush-cli auto-selects `parallel-deflate` for a 50 MB input without `--algorithm` flag — verified in T081
- [ ] Post-MVP cleanup complete (T082–T084 checked off)

---

## Notes

- `[P]` tasks operate on different files or independent data — safe to parallelize
- TDD: red → green → refactor strictly enforced. Do not write implementation before tests fail.
- GPU tests auto-skip when no compatible adapter is present (do not fail CI)
- All production code: no `.unwrap()`, no `.expect()` — use `?` throughout
- Commit after each logical group (at minimum: after each checkpoint)
- `crush-parallel` must have zero compile-time dependency on `crush-cli`
- `crush-core` must have zero compile-time dependency on `crush-parallel`
- Algorithm selection policy lives entirely in `crush-cli/src/algorithm.rs` — changing the threshold does not require recompiling `crush-parallel`
- `compress_file()` is the preferred entry point for large files; it uses `memmap2` internally for zero-copy reads (FR-009)
- `cargo-husky` hooks must be committed (`.cargo-husky/hooks/pre-commit`) so they activate for all contributors, not just the first committer
