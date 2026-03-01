# Tasks: GPU Compression Engine

**Input**: Design documents from `specs/008-gpu-compression-engine/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/plugin-api.md, quickstart.md

**Tests**: TDD is mandatory per constitution. Tests are written first, verified to fail, then implementation follows.

**Organization**: Tasks grouped by user story for independent implementation and testing.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story (US1–US5)
- All paths relative to repository root

---

## Phase 1: Setup (Project Initialization)

**Purpose**: Create crush-gpu crate, configure workspace, establish module skeleton

- [x] T001 Create crush-gpu crate directory and `crush-gpu/Cargo.toml` with dependencies per quickstart.md (wgpu, bytemuck, crc32fast, memmap2, rayon, thiserror, linkme, crush-core; optional cudarc behind `cuda` feature; dev-deps: criterion, proptest, tempfile)
- [x] T002 Add `"crush-gpu"` to workspace members in `Cargo.toml` and add `wgpu`, `bytemuck`, `pollster` to `[workspace.dependencies]`
- [x] T003 Create module skeleton with empty files: `crush-gpu/src/lib.rs`, `crush-gpu/src/engine.rs`, `crush-gpu/src/format.rs`, `crush-gpu/src/entropy.rs`, `crush-gpu/src/scorer.rs`, `crush-gpu/src/vectorize.rs`, `crush-gpu/src/backend/mod.rs`, `crush-gpu/src/backend/wgpu.rs`, `crush-gpu/src/backend/cuda.rs`
- [x] T004 [P] Configure clippy lints in `crush-gpu/Cargo.toml` matching crush-parallel pattern (all=deny, pedantic=warn, unwrap_used=deny, expect_used=deny, panic=deny, panic_in_result_fn=deny)
- [x] T005 [P] Create shader directory and placeholder files: `crush-gpu/src/shader/compress.wgsl`, `crush-gpu/src/shader/decompress.wgsl`
- [x] T006 [P] Create test file skeletons: `crush-gpu/tests/roundtrip.rs`, `crush-gpu/tests/format.rs`, `crush-gpu/tests/eligibility.rs`, `crush-gpu/tests/backend.rs`
- [x] T007 [P] Create benchmark skeletons: `crush-gpu/benches/throughput.rs`, `crush-gpu/benches/ratio.rs`
- [x] T008 Verify `cargo check -p crush-gpu` succeeds with empty module skeleton

**Checkpoint**: Crate compiles, all modules exist as stubs, workspace builds cleanly

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: GPU tile format serialization, entropy calculator, ComputeBackend trait, error types — MUST complete before any user story

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

### Tests (write first, verify they fail)

- [x] T009 [P] Write GpuFileHeader serialization round-trip tests in `crush-gpu/tests/format.rs` — verify header writes 64 bytes, reads back identically, rejects invalid magic/version
- [x] T010 [P] Write TileHeader serialization round-trip tests in `crush-gpu/tests/format.rs` — verify header writes 32 bytes, reads back identically, rejects unknown tile version
- [x] T011 [P] Write TileIndexEntry and TileIndexHeader round-trip tests in `crush-gpu/tests/format.rs` — verify 24-byte entry and 8-byte header round-trip
- [x] T012 [P] Write GpuFileFooter round-trip tests in `crush-gpu/tests/format.rs` — verify 24 bytes, CRC32 validation, magic check
- [x] T013 [P] Write Shannon entropy calculator tests in `crush-gpu/tests/eligibility.rs` — verify entropy of all-zeros (0.0), random data (~8.0), English text (~4.5), threshold at 7.5

### Implementation

- [x] T014 [P] Implement GpuFileHeader (64 bytes) in `crush-gpu/src/format.rs` — magic "CGPU" `[0x43, 0x47, 0x50, 0x55]`, format_version, engine_version, tile_size, sub_stream_count, flags, uncompressed_size, tile_count per data-model.md
- [x] T015 [P] Implement TileHeader (32 bytes) in `crush-gpu/src/format.rs` — version byte, flags (STORED, LAST_TILE), compressed_size, uncompressed_size, checksum, sub_stream_offsets_size per data-model.md
- [x] T016 [P] Implement TileIndexEntry (24 bytes) and TileIndexHeader (8 bytes) in `crush-gpu/src/format.rs` — tile_offset, compressed_size, uncompressed_size, checksum, flags per data-model.md
- [x] T017 [P] Implement GpuFileFooter (24 bytes) in `crush-gpu/src/format.rs` — index_offset, index_size, footer_checksum (CRC32 of first 12 bytes), format_version, magic per data-model.md
- [x] T018 Implement Shannon entropy calculator in `crush-gpu/src/entropy.rs` — sample up to 1MB, compute `H = -Σ(p_i × log2(p_i))` over 256-bucket byte frequency distribution, return f64
- [x] T019 Define ComputeBackend trait in `crush-gpu/src/backend/mod.rs` — `name()`, `gpu_info()`, `decompress_tiles()`, `release()` per contracts/plugin-api.md; define GpuInfo and GpuVendor types
- [x] T020 Define GPU-specific error types in `crush-gpu/src/format.rs` or dedicated error module — GpuError for backend failures, tile version mismatch, GPU memory exceeded, shader compilation failure
- [x] T021 Verify all foundational tests pass: `cargo test -p crush-gpu`

**Checkpoint**: Format serializes/deserializes correctly, entropy calculator works, ComputeBackend trait defined — user story implementation can begin

---

## Phase 3: User Story 1 — GPU-Accelerated Compression of Large Files (Priority: P1) 🎯 MVP

**Goal**: Compress files >100MB using GPU-parallel tile-based processing on at least one GPU vendor (NVIDIA via wgpu). Decompress on any system (GPU or CPU fallback). Byte-for-byte round-trip guarantee.

**Independent Test**: Compress a 200MB file on a system with a GPU, verify valid archive, decompress to identical copy.

### Tests for User Story 1 (write first, verify they fail)

- [x] T022 [P] [US1] Write end-to-end round-trip test in `crush-gpu/tests/roundtrip.rs` — compress 1MB test data via engine, decompress, assert byte-for-byte identical
- [x] T023 [P] [US1] Write round-trip property test in `crush-gpu/tests/roundtrip.rs` using proptest — compress arbitrary byte vectors, decompress, verify identity
- [x] T024 [P] [US1] Write CPU fallback decompression test in `crush-gpu/tests/roundtrip.rs` — compress data, force CPU-only decompress path, verify identical output
- [x] T025 [P] [US1] Write tile boundary test in `crush-gpu/tests/roundtrip.rs` — compress data that is exactly N×64KB, N×64KB+1, and N×64KB-1 bytes, verify correct tile splitting

### Implementation for User Story 1

- [x] T026 [US1] Implement CPU-side LZ77 matching + Huffman encoding in `crush-gpu/src/engine.rs` — for each 64KB tile: find LZ77 matches, assign Huffman codes, interleave output across 32 sub-streams
- [x] T027 [US1] Implement tile writer in `crush-gpu/src/engine.rs` — split input into 64KB tiles, compress each tile (T026), write TileHeader + payload padded to 128-byte alignment, track tile offsets for index
- [x] T028 [US1] Implement archive writer in `crush-gpu/src/engine.rs` — write GpuFileHeader, all tiles (T027), TileIndexHeader + TileIndexEntry array, GpuFileFooter; public `compress(input: &[u8], config: &EngineConfig) -> Result<Vec<u8>>`
- [x] T029 [US1] Implement CPU fallback tile decompressor in `crush-gpu/src/engine.rs` — read 32 sub-streams from tile payload, decode Huffman codes sequentially per sub-stream, execute LZ77 copies, validate CRC32
- [x] T030 [US1] Implement archive reader in `crush-gpu/src/engine.rs` — read footer → index → header → decompress all tiles (parallel via rayon for CPU path); public `decompress(input: &[u8], config: &EngineConfig) -> Result<Vec<u8>>`
- [x] T031 [US1] Implement wgpu backend initialization in `crush-gpu/src/backend/wgpu.rs` — create wgpu Instance, discover adapters, select GPU meeting minimum requirements (Vulkan 1.2 / Metal 2 + 2GB VRAM), create Device + Queue
- [ ] T032 [US1] Write WGSL decompression compute shader in `crush-gpu/src/shader/decompress.wgsl` — workgroup size 32, each thread decodes one sub-stream from a tile, shared memory for LZ77 copy reconstruction
- [ ] T033 [US1] Implement wgpu GPU decompression dispatch in `crush-gpu/src/backend/wgpu.rs` — upload compressed tiles to GPU buffer, dispatch one workgroup (32 threads) per tile, read back decompressed tiles, respect 256MB GPU memory budget (batch if needed)
- [x] T034 [US1] Integrate GPU decompression path in `crush-gpu/src/engine.rs` — if GPU available use wgpu backend for decompression, else fall back to CPU path (T029/T030)
- [x] T035 [US1] Implement cooperative cancellation in `crush-gpu/src/engine.rs` — check cancel_flag after each tile batch during compression and decompression, return `CrushError::Cancelled` if set, release GPU resources
- [x] T036 [US1] Implement progress reporting in `crush-gpu/src/engine.rs` — bridge crush-core progress callback, report progress per tile during compression and decompression
- [x] T037 [US1] Register GPU plugin via linkme in `crush-gpu/src/lib.rs` — implement `CompressionAlgorithm` trait with magic `[0x43, 0x52, 0x01, 0x03]`, name "gpu-deflate", metadata per contracts/plugin-api.md, wire compress/decompress to engine
- [x] T038 [US1] Verify all US1 tests pass and round-trip is correct: `cargo test -p crush-gpu`

**Checkpoint**: GPU compression and decompression works end-to-end on at least one backend. CPU fallback decompression works. Plugin is registered.

---

## Phase 4: User Story 2 — Cross-Platform GPU Support with Vendor Fallback (Priority: P2)

**Goal**: Support NVIDIA (CUDA fast path), AMD (Vulkan), and Apple Silicon (Metal) through backend auto-selection. Graceful CPU fallback when no GPU is available. Archives are interchangeable across backends.

**Independent Test**: Run compression on systems with different GPU vendors, verify backend auto-selection and identical archive output.

### Tests for User Story 2

- [x] T039 [P] [US2] Write backend discovery tests in `crush-gpu/tests/backend.rs` — verify `discover_backends()` returns available backends, verify selection priority (CUDA > wgpu-Vulkan > wgpu-Metal)
- [x] T040 [P] [US2] Write CPU fallback integration test in `crush-gpu/tests/backend.rs` — mock no GPU available, verify compression falls back to CPU parallel plugin path with informational message

### Implementation for User Story 2

- [x] T041 [US2] Implement CUDA backend in `crush-gpu/src/backend/cuda.rs` (feature-gated behind `cuda`) — initialize CUDA context via cudarc, compile PTX decompression kernel at runtime via nvrtc, implement `ComputeBackend` trait
- [x] T042 [US2] Implement backend auto-selection in `crush-gpu/src/backend/mod.rs` — `discover_gpu() -> Option<Box<dyn ComputeBackend>>` that probes CUDA first (if feature enabled), then wgpu adapters, filters by minimum requirements, selects highest-capability GPU
- [x] T043 [US2] Implement GPU fallback to CPU in `crush-gpu/src/engine.rs` — when `discover_gpu()` returns None, log informational message and use rayon-based CPU tile decompression; during compression, use CPU-only LZ77 path (compression is always CPU per research R-004)
- [x] T044 [US2] Verify cross-backend archive compatibility — compress with wgpu backend, decompress with CPU fallback, verify identical; compress with CPU, decompress with wgpu, verify identical
- [x] T045 [US2] Verify all US2 tests pass: `cargo test -p crush-gpu` and `cargo test -p crush-gpu --features cuda` (if CUDA available)

**Checkpoint**: Backend auto-selection works. Archives produced by any backend decompress correctly on any other backend or CPU fallback.

---

## Phase 5: User Story 3 — Automatic GPU Eligibility Detection (Priority: P3)

**Goal**: GPU plugin automatically evaluates three criteria (file >100MB, GPU present, entropy ≤7.5 bits/byte) and only claims compression tasks when all pass. Plugin selector integration via scoring.

**Independent Test**: Present files of varying sizes and types to the plugin selector, verify GPU plugin only claims when all three criteria are met.

### Tests for User Story 3

- [x] T046 [P] [US3] Write eligibility scorer unit tests in `crush-gpu/tests/eligibility.rs` — file <100MB → score 0.0; no GPU → score 0.0; entropy >7.5 → score 0.0; all pass → score 0.95
- [x] T047 [P] [US3] Write entropy threshold tests in `crush-gpu/tests/eligibility.rs` — encrypted file (entropy ~8.0) → rejected; JPEG (entropy ~7.8) → rejected; CSV log file (entropy ~4.5) → accepted; binary executable (entropy ~6.5) → accepted

### Implementation for User Story 3

- [x] T048 [US3] Implement EligibilityResult struct and eligibility scorer in `crush-gpu/src/scorer.rs` — evaluate file_size_ok (>100MB), gpu_available (discover_gpu is Some), entropy_ok (Shannon entropy ≤7.5 via entropy.rs), combine into score per contracts/plugin-api.md
- [x] T049 [US3] Integrate scorer into plugin's `detect()` method in `crush-gpu/src/lib.rs` — call `entropy::calculate_entropy()` on file header sample, combine with file size and GPU availability, return appropriate score to plugin selector
- [x] T050 [US3] Wire eligibility check into `compress()` path in `crush-gpu/src/engine.rs` — if scorer determines file is ineligible, return error so crush-core routes to a different plugin
- [x] T051 [US3] Verify all US3 tests pass: `cargo test -p crush-gpu`

**Checkpoint**: GPU plugin correctly claims only eligible files. Files under 100MB, files on GPU-less systems, and high-entropy files are declined.

---

## Phase 6: User Story 4 — GPU-Optimized Tile-Based Random Access (Priority: P4)

**Goal**: Decompress individual tiles by index without reading the entire archive. Verify 128-byte alignment and O(1) tile lookup via the index.

**Independent Test**: Compress a file, request decompression of a single tile by index, verify only that tile's data is read and decompressed correctly.

### Tests for User Story 4

- [x] T052 [P] [US4] Write random access decompression tests in `crush-gpu/tests/roundtrip.rs` — compress multi-tile file, decompress tile 0, tile N/2, last tile individually, verify each matches corresponding 64KB slice of original
- [x] T053 [P] [US4] Write 128-byte alignment verification test in `crush-gpu/tests/format.rs` — compress data, inspect raw archive bytes, verify every tile payload starts at a 128-byte aligned offset
- [x] T054 [P] [US4] Write tile index O(1) lookup test in `crush-gpu/tests/format.rs` — load tile index from archive, verify `get_tile_entry(n)` returns correct offset and size for arbitrary tile indices

### Implementation for User Story 4

- [x] T055 [US4] Implement `load_tile_index(archive: &[u8]) -> Result<TileIndex>` in `crush-gpu/src/engine.rs` — read footer, seek to index_offset, deserialize all TileIndexEntry records into a Vec for O(1) access
- [x] T056 [US4] Implement `decompress_tile(archive: &[u8], tile_index: usize) -> Result<Vec<u8>>` in `crush-gpu/src/engine.rs` — load tile index, read only the target tile's TileHeader + payload, decompress (GPU or CPU), validate CRC32, return decompressed tile data
- [x] T057 [US4] Export random access API from `crush-gpu/src/lib.rs` — public `load_tile_index`, `decompress_tile`, `TileIndex` type
- [x] T058 [US4] Verify all US4 tests pass: `cargo test -p crush-gpu`

**Checkpoint**: Single-tile decompression works without reading other tiles. Tile index provides O(1) lookup. All tile boundaries are 128-byte aligned.

---

## Phase 7: User Story 5 — Vectorized Pattern Matching for Improved Compression (Priority: P5)

**Goal**: SIMD-accelerated LZ77 matching that activates only when it produces smaller output. String density and entropy heuristics gate activation.

**Independent Test**: Compress a corpus of text-heavy and binary files with and without vectorized matching, verify it only activates when output is smaller.

### Tests for User Story 5

- [x] T059 [P] [US5] Write vectorized matching comparison tests in `crush-gpu/tests/roundtrip.rs` — compress CSV/log data with and without vectorized matching, verify vectorized output is ≥1% smaller
- [x] T060 [P] [US5] Write activation heuristic tests in `crush-gpu/tests/eligibility.rs` — verify vectorized matching activates for string density >70% + entropy <6.0, skips for binary data
- [x] T061 [P] [US5] Write safety test in `crush-gpu/tests/roundtrip.rs` — verify vectorized matching never produces output larger than standard matching for any test input

### Implementation for User Story 5

- [x] T062 [US5] Implement SIMD string matching in `crush-gpu/src/vectorize.rs` — SSE4.2/AVX2 via `std::arch` for 16/32-byte parallel byte comparison, hash-based match finding with SIMD-accelerated hash computation
- [x] T063 [US5] Implement activation heuristic in `crush-gpu/src/vectorize.rs` — compute string density (printable ASCII ratio) on 1MB sample; if density >70% AND entropy <6.0: sample-compress 1MB with both methods, use whichever is smaller
- [x] T064 [US5] Integrate vectorized matching into compression pipeline in `crush-gpu/src/engine.rs` — during tile compression, check activation heuristic per-file (not per-tile), use vectorized LZ77 matching if activated, set `VECTORIZE_USED` flag in GpuFileHeader
- [x] T065 [US5] Verify all US5 tests pass: `cargo test -p crush-gpu`

**Checkpoint**: Vectorized matching produces measurably smaller output on text-heavy data. Never produces larger output. Activates only when beneficial.

---

## Phase 8: Polish & Cross-Cutting Concerns

**Purpose**: Benchmarks, fuzz testing, documentation, quality gates

- [x] T066 [P] Implement throughput benchmarks in `crush-gpu/benches/throughput.rs` — benchmark compress and decompress at 1MB, 10MB, 100MB sizes using criterion; compare GPU vs CPU path
- [x] T067 [P] Implement compression ratio benchmarks in `crush-gpu/benches/ratio.rs` — compare GPU plugin ratio vs crush-parallel (DEFLATE) across text, binary, mixed corpora
- [x] T068 [P] Create fuzz target for format parsing in `crush-gpu/fuzz/fuzz_targets/fuzz_format.rs` — fuzz GpuFileHeader, TileHeader, GpuFileFooter deserialization with arbitrary bytes
- [x] T069 [P] Create fuzz target for decompression in `crush-gpu/fuzz/fuzz_targets/fuzz_decompress.rs` — fuzz full decompression path with arbitrary input
- [x] T070 [P] Add `cargo doc` documentation for all public APIs in `crush-gpu/src/lib.rs`, `crush-gpu/src/engine.rs`, `crush-gpu/src/format.rs`
- [x] T071 Run full quality gate validation: `cargo fmt --all -- --check && cargo clippy -p crush-gpu --all-targets -- -D warnings && cargo test -p crush-gpu && cargo doc -p crush-gpu --no-deps`
- [x] T072 Run post-MVP cleanup: detect-duplicates, extract shared utilities, verify no code duplication >20 lines per constitution cleanup requirements

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — start immediately
- **Foundational (Phase 2)**: Depends on Setup — BLOCKS all user stories
- **US1 (Phase 3)**: Depends on Foundational — core engine, MVP target
- **US2 (Phase 4)**: Depends on US1 — adds CUDA backend + auto-selection
- **US3 (Phase 5)**: Depends on Foundational — can run in parallel with US1/US2, but benefits from US1 for end-to-end validation
- **US4 (Phase 6)**: Depends on US1 (format + engine must work) — adds random access API
- **US5 (Phase 7)**: Depends on US1 (compression pipeline must work) — adds vectorized matching
- **Polish (Phase 8)**: Depends on all desired user stories being complete

### User Story Dependencies

```text
Setup → Foundational → US1 (MVP) → US2 (cross-platform)
                     ↘ US3 (eligibility) — can start after Foundational
                     US1 → US4 (random access)
                     US1 → US5 (vectorized matching)
                     All stories → Polish
```

### Within Each User Story

1. Tests MUST be written and FAIL before implementation
2. Format/model code before engine/service code
3. Core logic before integration
4. Integration before plugin registration
5. Verify tests pass as final task

### Parallel Opportunities

**Within Phase 1**: T004, T005, T006, T007 can all run in parallel after T003
**Within Phase 2**: T009–T013 (tests) all parallel; T014–T017 (format structs) all parallel; T018–T020 sequential
**Within US1**: T022–T025 (tests) all parallel; T026–T028 sequential (LZ77→tile writer→archive writer); T031–T033 sequential (wgpu init→shader→dispatch)
**Across stories**: US3 can start in parallel with US1 after Foundational; US4 and US5 can start in parallel after US1

---

## Parallel Example: User Story 1

```text
# Write all US1 tests in parallel:
T022: Round-trip test in crush-gpu/tests/roundtrip.rs
T023: Property test in crush-gpu/tests/roundtrip.rs
T024: CPU fallback test in crush-gpu/tests/roundtrip.rs
T025: Tile boundary test in crush-gpu/tests/roundtrip.rs

# Then implement compression pipeline (sequential):
T026 → T027 → T028 (LZ77 → tile writer → archive writer)

# In parallel, implement decompression:
T029 → T030 (CPU decompress → archive reader)
T031 → T032 → T033 (wgpu init → shader → GPU dispatch)

# Then integrate:
T034 → T035 → T036 → T037 → T038
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup
2. Complete Phase 2: Foundational (CRITICAL — blocks all stories)
3. Complete Phase 3: User Story 1
4. **STOP and VALIDATE**: Compress + decompress round-trip, GPU and CPU paths
5. This is the deployable MVP — GPU compression works on one platform

### Incremental Delivery

1. Setup + Foundational → Foundation ready
2. Add US1 → GPU compression works (MVP!)
3. Add US2 → Cross-platform backends (NVIDIA/AMD/Apple/CPU)
4. Add US3 → Automatic eligibility detection (smart plugin selection)
5. Add US4 → Random access decompression (tile-level API)
6. Add US5 → Vectorized matching (improved ratios on text data)
7. Each story adds value without breaking previous stories

### Parallel Team Strategy

With multiple developers after Foundational:

- Developer A: US1 (core engine — critical path)
- Developer B: US3 (eligibility — independent of engine internals)
- After US1 completes:
  - Developer A: US2 (cross-platform)
  - Developer B: US4 (random access)
  - Developer C: US5 (vectorized matching)

---

## Notes

- Constitution requires TDD: all test tasks must complete and fail before implementation
- Constitution requires no `.unwrap()` in production code — use `?` operator throughout
- Plugin ID `0x03` — magic `[0x43, 0x52, 0x01, 0x03]`
- GPU tile magic "CGPU" — `[0x43, 0x47, 0x50, 0x55]`
- CUDA backend is feature-gated (`--features cuda`) — all tests must pass without it
- 128-byte alignment padding uses zero bytes
- Verify `cargo clippy -p crush-gpu --all-targets -- -D warnings` after every task group
