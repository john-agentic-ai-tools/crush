# Tasks: GDeflate GPU Decompression

**Input**: Design documents from `/specs/009-gdeflate-gpu-decompression/`
**Prerequisites**: plan.md (required), spec.md (required for user stories), research.md, data-model.md, contracts/

**Tests**: Tests are included as this is a compression library where correctness is critical (constitution: Correctness & Safety, Test-First Development).

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2)
- Include exact file paths in descriptions

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Add GDeflate dependencies and update format version

- [x] T001 Add `flate2` dependency to crush-gpu/Cargo.toml for DEFLATE encoding in GDeflate compressor
- [x] T002 Change `FORMAT_VERSION` from 1 to 2 in crush-gpu/src/format.rs and update `GpuFileHeader::new()` and `GpuFileFooter::new()` to use it
- [x] T003 Add format version test in crush-gpu/tests/format.rs: verify v2 headers/footers parse correctly, v1 and v3 are rejected

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: GDeflate CPU codec — the compressor and CPU-fallback decompressor that all user stories depend on

**CRITICAL**: No user story work can begin until this phase is complete

- [x] T004 Create crush-gpu/src/gdeflate.rs module with GDeflate bitstream constants (DEFLATE fixed Huffman tables, code length alphabet order, base lengths/distances/extra bits tables per GDeflate IETF spec)
- [x] T005 Implement `gdeflate_compress_tile()` in crush-gpu/src/gdeflate.rs: takes raw tile bytes (up to 64KB), finds LZ77 matches via hash-chain, distributes symbols round-robin across 32 sub-streams, serializes as GDeflate bitstream (128-byte initial state + interleaved data)
- [x] T006 Implement `gdeflate_decompress_tile()` (CPU fallback) in crush-gpu/src/gdeflate.rs: reads 32 sub-stream initial states, builds Huffman tables from block headers, decodes symbols from interleaved sub-streams in round-robin order, performs LZ copies, returns decompressed bytes
- [x] T007 Register `gdeflate` module in crush-gpu/src/lib.rs (`pub mod gdeflate;`)
- [x] T008 Add GDeflate codec unit tests in crush-gpu/tests/gdeflate.rs: roundtrip empty data, small data (<64KB), exact 64KB tile, various data patterns (text, binary, repeated, random)
- [x] T009 Add GDeflate compression ratio test in crush-gpu/tests/gdeflate.rs: verify GDeflate ratio is within 5% of raw DEFLATE for English text and mixed binary data
- [x] T010 Add GDeflate edge case tests in crush-gpu/tests/gdeflate.rs: incompressible data (stored blocks), single-byte input, all-zeros input, max-length matches

**Checkpoint**: GDeflate CPU codec is proven correct — compress → decompress → verify for all data types

---

## Phase 3: User Story 1 - GPU-Accelerated Decompression with GDeflate (Priority: P1) MVP

**Goal**: Port the HLSL GDeflate decompressor to WGSL and wire it into the GPU backend so tiles compressed with GDeflate are decompressed on the GPU at >1 GB/s throughput.

**Independent Test**: Compress test data with `gdeflate_compress_tile()`, dispatch to GPU via `decompress_tiles_gdeflate()`, verify byte-for-byte identical output.

### Implementation for User Story 1

- [x] T011 [US1] Create crush-gpu/src/shader/gdeflate_decompress.wgsl with buffer bindings: `@binding(0) compressed: array<u32>` (read), `@binding(1) control: array<u32>` (read_write), `@binding(2) output: array<u32>` (read_write), `@binding(3) scratch: array<u32>` (read_write); workgroup size 32; workgroup shared memory declarations (`g_tmp`, `g_buf`, `g_lut`)
- [x] T012 [US1] Implement BitReader in crush-gpu/src/shader/gdeflate_decompress.wgsl: 64-bit state emulated as `(lo: u32, hi: u32)` pair, `read_bits(n)`, `peek_bits(n)`, `refill()` from interleaved sub-stream, bit reversal helper
- [x] T013 [US1] Implement Huffman table builder in crush-gpu/src/shader/gdeflate_decompress.wgsl: `build_huffman_table()` that reads code lengths from shared memory, computes base codes and offsets via prefix sum (emulated with `var<workgroup>` + `workgroupBarrier()`), fills `g_lut` symbol table
- [x] T014 [US1] Implement Huffman decoder in crush-gpu/src/shader/gdeflate_decompress.wgsl: `decode_symbol()` reads bits from BitReader, reverse-bit lookups against base codes in `g_lut`, returns decoded symbol
- [x] T015 [US1] Implement decode loop in crush-gpu/src/shader/gdeflate_decompress.wgsl: read block header (BFINAL + BTYPE via lane 0), dispatch to dynamic/fixed/non-compressed block handlers, decode literal/length/distance symbols in SIMD rounds, write output via shared prefix-sum-based scatter
- [x] T016 [US1] Implement LZ copy in crush-gpu/src/shader/gdeflate_decompress.wgsl: broadcast copy distance/length to all 32 threads, distribute copy work (`for i in tid..len step 32`), handle overlapping copies
- [x] T017 [US1] Implement wave intrinsic emulation helpers in crush-gpu/src/shader/gdeflate_decompress.wgsl: `emulated_prefix_sum()`, `emulated_broadcast()`, `emulated_ballot()` using `var<workgroup>` arrays + `workgroupBarrier()`
- [x] T018 [US1] Add `decompress_tiles_gdeflate()` method to `ComputeBackend` trait in crush-gpu/src/backend/mod.rs with same signature as `decompress_tiles()` but for GDeflate-encoded tiles
- [x] T019 [US1] Implement `decompress_tiles_gdeflate()` in crush-gpu/src/backend/wgpu_backend.rs: create GDeflate compute pipeline at backend init (alongside existing LZ77 pipeline), create GDeflate-specific bind group layout, dispatch per-tile workgroups, readback decompressed data; wrap in `catch_unwind` for GPU panic safety
- [x] T020 [US1] Add GPU GDeflate roundtrip test in crush-gpu/tests/backend.rs: compress with `gdeflate_compress_tile()`, dispatch to GPU via `decompress_tiles_gdeflate()`, verify byte-for-byte match for 1KB, 32KB, and 64KB tiles
- [x] T021 [US1] Add GPU GDeflate throughput smoke test in crush-gpu/tests/backend.rs: decompress 1MB of GDeflate tiles on GPU, assert throughput exceeds 100 MiB/s (sanity check, not full benchmark)

**Checkpoint**: GPU GDeflate decompression works end-to-end. Compress on CPU, decompress on GPU, verified correct.

---

## Phase 4: User Story 2 - Engine Integration & GDeflate Compression (Priority: P2)

**Goal**: Update the engine to produce GDeflate-format files (v2) and use GDeflate decompression exclusively.

**Independent Test**: Call `compress()` → produces v2 file, then `decompress()` → uses GPU if available, produces correct output.

### Implementation for User Story 2

- [x] T022 [US2] Update `compress()` in crush-gpu/src/engine.rs to use `gdeflate::gdeflate_compress_tile()` instead of `lz77::lz77_encode()` for tile compression, write format version 2 in file header, write tile version 2 in tile headers
- [x] T023 [US2] Add `decompress_tiles_cpu_gdeflate()` function in crush-gpu/src/engine.rs: reads GDeflate tile payloads, calls `gdeflate::gdeflate_decompress_tile()` per tile, validates CRC32 checksums, assembles output
- [x] T024 [US2] Update `decompress()` in crush-gpu/src/engine.rs: try GPU GDeflate path first via `backend.decompress_tiles_gdeflate()`, fall back to `decompress_tiles_cpu_gdeflate()` on failure or when `force_cpu` is set
- [x] T025 [US2] Update `read_and_decompress_tile()` in crush-gpu/src/engine.rs to call `gdeflate::gdeflate_decompress_tile()` instead of `lz77::lz77_decode()` for CPU-path single-tile decompression
- [x] T026 [US2] Update `decompress_tile_by_index()` in crush-gpu/src/engine.rs for random-access GDeflate tile decompression
- [x] T027 [US2] Update full roundtrip tests in crush-gpu/tests/roundtrip.rs: verify `compress()` → `decompress()` roundtrip produces correct output with GDeflate format for all existing test cases (empty, small, 1MB, tile boundaries, random access)
- [x] T028 [US2] Add property-based roundtrip test in crush-gpu/tests/roundtrip.rs: `proptest` with arbitrary byte vectors up to 256KB, compress → decompress → assert equal
- [x] T029 [US2] Add CPU-fallback test in crush-gpu/tests/roundtrip.rs: compress with GDeflate, decompress with `force_cpu: true`, verify correct output

**Checkpoint**: Full engine works with GDeflate format. compress/decompress API unchanged. GPU and CPU paths both produce correct output.

---

## Phase 5: User Story 3 - Performance Benchmarking (Priority: P3)

**Goal**: Benchmark suite validates GDeflate throughput improvement and compression ratio parity.

**Independent Test**: Run `cargo bench` and compare GDeflate numbers against previous LZ77 baseline.

### Implementation for User Story 3

- [x] T030 [P] [US3] Update crush-gpu/benches/throughput.rs: replace LZ77 benchmark groups with GDeflate compress/decompress benchmarks (1MB, 10MB), GPU and CPU paths
- [x] T031 [P] [US3] Update crush-gpu/benches/ratio.rs: replace LZ77 ratio benchmarks with GDeflate ratio benchmarks for text, binary, and mixed data
- [x] T032 [US3] Run benchmarks, verify GDeflate GPU decompression throughput exceeds 650 MiB/s (5x over LZ77 baseline of ~130 MiB/s), document results

**Checkpoint**: Benchmarks confirm GDeflate GPU path achieves target throughput.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Quality gates, cleanup, and documentation

- [x] T033 Run `cargo fmt --package crush-gpu -- --check` and fix any formatting issues
- [x] T034 Run `cargo clippy --package crush-gpu --all-targets -- -D warnings` and fix all warnings
- [x] T035 Run `cargo test --workspace` and verify all tests pass (existing tests plus new GDeflate tests)
- [x] T036 Run `cargo doc --package crush-gpu --no-deps` and fix any documentation warnings
- [x] T037 Run code duplication analysis via `detect-duplicates.ps1 -Json` and extract duplicates >20 lines into shared utilities
- [x] T038 Create specs/009-gdeflate-gpu-decompression/cleanup-summary.md documenting duplication findings and resolutions

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — can start immediately
- **Foundational (Phase 2)**: Depends on Setup completion — BLOCKS all user stories
- **User Story 1 (Phase 3)**: Depends on Foundational phase (GDeflate CPU codec must exist to generate test data for GPU)
- **User Story 2 (Phase 4)**: Depends on US1 (GPU path must work before engine integration wires it up)
- **User Story 3 (Phase 5)**: Depends on US2 (engine integration must be complete for benchmarks)
- **Polish (Phase 6)**: Depends on all user stories being complete

### User Story Dependencies

- **US1 (GPU Decompression)**: Requires Foundational GDeflate codec (Phase 2) for test data generation
- **US2 (Engine Integration)**: Requires US1 GPU path working + Foundational codec
- **US3 (Benchmarks)**: Requires US2 complete engine

### Within Each User Story

- Shader implementation before backend integration (US1)
- Engine dispatch before tests (US2)
- Benchmark setup before benchmark execution (US3)

### Parallel Opportunities

- T004, T005, T006 must be sequential (T005 depends on T004 constants, T006 on T005 format)
- T011 through T017 are sequential (shader builds incrementally)
- T030, T031 can run in parallel (different benchmark files)

---

## Parallel Example: User Story 1

```text
# Sequential shader implementation (each builds on prior):
T011 → T012 → T013 → T014 → T015 → T016 → T017

# Then backend integration:
T018 → T019

# Then verification:
T020, T021 (can run in parallel — different test functions)
```

## Parallel Example: User Story 3

```text
# Benchmark files can be written in parallel:
T030 (throughput.rs) || T031 (ratio.rs)

# Then run benchmarks:
T032
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup (format version update)
2. Complete Phase 2: Foundational (GDeflate CPU codec)
3. Complete Phase 3: User Story 1 (GPU shader + backend)
4. **STOP and VALIDATE**: Compress on CPU with `gdeflate_compress_tile()`, decompress on GPU, verify correctness and measure throughput
5. If throughput >1 GB/s: MVP proven

### Incremental Delivery

1. Setup + Foundational → GDeflate codec proven correct
2. US1 (GPU Decompression) → GPU path works → Throughput validated
3. US2 (Engine Integration) → Full compress/decompress API works with GDeflate
4. US3 (Benchmarks) → Performance documented
5. Polish → Quality gates pass

### Risk Mitigation

- **WGSL shader complexity**: The GDeflate shader is the highest-risk task. If the full Huffman decoder proves too complex for WGSL, a simplified GDeflate variant (fixed Huffman only, no dynamic blocks) can be implemented first as a stepping stone.
- **GPU throughput target**: The 1 GB/s target assumes shared-memory wave emulation. If subgroup operations become available in wgpu, throughput could improve 3-5x.
- **Compression ratio parity**: The 5% tolerance accounts for GDeflate's tail overhead from sub-stream interleaving.

---

## Notes

- [P] tasks = different files, no dependencies
- [Story] label maps task to specific user story for traceability
- Each user story should be independently completable and testable
- Commit after each task or logical group
- Stop at any checkpoint to validate story independently
- Quality hook: `cargo fmt`, `cargo clippy -D warnings`, `cargo test` must pass after each phase
