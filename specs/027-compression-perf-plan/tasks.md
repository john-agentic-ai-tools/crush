# Tasks: Parallel Fast Compression

**Input**: Design documents from `specs/027-compression-perf-plan/`
**Tests**: Included. Output and cancellation tests come first. They pass on the old code, because they guard behaviour while speed changes.

## Phase 1: Setup

- [X] T001 Add `rayon` to `crush-lz/Cargo.toml`
- [X] T002 Remove `crush-fast-lz` from `SINGLE_THREADED` in `perf/compare.py`

## Phase 2: User Story 1 - Fast compression uses every core (P1)

**Independent Test**: `cargo test -p crush-lz --test parallel_compress`, then SC-001 and SC-002 on `real`.

- [X] T003 [US1] Write hash-pinned output, pool-size, stream-equality and cancellation tests in `crush-lz/tests/parallel_compress.rs`, and pin them on the old code
- [X] T004 [US1] Load the 8-byte word once in `find_sequences` in `crush-lz/src/compress.rs`
- [X] T005 [US1] Add a reusable worker buffer to `Compressor` and use it in `push_block` in `crush-lz/src/compress.rs`
- [X] T006 [US1] Add batched parallel compression to `compress_frame_into` and `compress_frame_to` in `crush-lz/src/compress.rs`, keeping the serial path for one thread
- [X] T007 [US1] Measure SC-001, SC-002 and SC-004 and record them in `perf/log/2026-11-parallel-fast-compression.md`

## Phase 3: User Story 2 - Tile compression is usable on the CPU (P2)

**Independent Test**: `cargo test -p crush-gpu --test compress_stability`, then SC-003 on `real`.

- [X] T008 [US2] Write hash-pinned output, pool-size and cancellation tests in `crush-gpu/tests/compress_stability.rs`, and pin them on the old code
- [X] T009 [US2] Replace the bit loops in `BitWriter` with a 64-bit accumulator in `crush-gpu/src/gdeflate.rs`
- [X] T010 [US2] Build fixed codes and length and distance lookups once in `crush-gpu/src/gdeflate.rs`, with a unit test against the linear search for every length and distance
- [X] T011 [US2] Split per-tile work from layout and compress tiles in parallel in `crush-gpu/src/engine.rs`
- [X] T012 [US2] Measure SC-003 and SC-004 and record them in `perf/log/2026-11-parallel-fast-compression.md`

## Phase 4: Polish

- [X] T013 Run `/speckit-crush-perf-gate` and record SC-006 in `perf/log/2026-11-parallel-fast-compression.md`
- [X] T014 Fuzz `crush-lz` round trips for 100,000 runs. Unseeded, because the corpus is not tracked
- [X] T015 Run `.specify/scripts/powershell/detect-duplicates.ps1` on `crush-lz/src` and `crush-gpu/src`, and record the cleanup in `specs/027-compression-perf-plan/cleanup-summary.md`

## Dependencies

T003 before T004-T006. T008 before T009-T011. User stories are independent.
