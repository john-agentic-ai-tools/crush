---

description: "Task list for the hybrid high-speed codec"
---

# Tasks: Hybrid High-Speed Codec

**Input**: Design documents from `/specs/026-hybrid-speed-codec/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md

**Tests**: Required. Constitution Principle IV makes test-first development mandatory. In each story, write the tests, get maintainer approval at the marked checkpoint, confirm they fail, then implement.

**Standing rules for every task**:

- No `.unwrap()` or `.expect()` in production code. Return errors through `crush_core::error::Result`.
- No new runtime dependency, and no C dependency (FR-015).
- No `unsafe` in `crush-sprint`. A measured wall goes through T046, never a direct fix.
- Each commit must pass `cargo fmt`, `cargo clippy --all-targets -- -D warnings` and `cargo test`.
- Benchmarks run in WSL with `PATH="$HOME/.local/bin:$PATH"`. Windows profiling is a last resort and needs a warning before the UAC prompt.

**Measurement results**: Record every result in `perf/log/2026-11-hybrid-speed-codec.md`, with machine, zstd version, build and spread.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: Which user story the task belongs to

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Create the crate and the measurement log

- [X] T001 Create crate `crush-sprint` with `#![forbid(unsafe_code)]` in `crush-sprint/src/lib.rs`, `crush-sprint/Cargo.toml` (dependencies `crush-core`, `crush-parallel`, `linkme`, `crc32fast`; dev `criterion`, `proptest`; `[lints.clippy]` matching `crush-lz/Cargo.toml`), and add it to `members` in `Cargo.toml`
- [X] T002 [P] Create `crush-sprint/fuzz/` as its own workspace, modelled on `crush-lz/fuzz/`, and add it to `exclude` in `Cargo.toml`
- [X] T003 [P] Create the results log `perf/log/2026-11-hybrid-speed-codec.md` with sections for baselines, profiles, sweep, prototype, and gates

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Trustworthy measurement and a safety net for the engine refactor. Every story depends on this phase.

**CRITICAL**: No engine or codec work begins until this phase is complete.

- [X] T004 Add an in-memory mode to `perf/compare.py` that reports `zstd -b` single-core figures, and print spreads for every row
- [X] T005 Add a build-rotation option to `perf/compare.py` that repeats a run across several separate builds, for wins under 10% (SC-011)
- [X] T006 Run the baseline on `real` and `text` at 1, 4 and all cores with `python3 perf/compare.py --tools crush,zstd-3,zstd-1 --data real,text --abba --reps 10`, plus `zstd -b1e3` on the `real` corpus, and record results and the CLI versus in-memory gap in `perf/log/2026-11-hybrid-speed-codec.md`
- [X] T007 Create golden CRSH v1 files from the current code, with block sizes 64 KiB and 1 MiB plus one stored-block case, in `crush-parallel/tests/golden/`, before any engine change
- [X] T008 Add a test in `crush-parallel/tests/golden_v1.rs` that asserts golden files decompress byte for byte, and that current compression reproduces them exactly

**Checkpoint**: Baselines recorded, golden files committed. Stop if run spread makes gates unusable, and fix the measurement method first.

---

## Phase 3: User Story 1 - Parallel decompression scales (Priority: P1) 🎯 MVP

**Goal**: Existing large archives decompress at least 3.0x faster at 4 cores and 3.3x at all cores than at 1 core, through a codec-agnostic engine.

**Independent Test**: Re-run T006 for `crush`. Golden and pre-existing archives decompress byte for byte, and `--block` still works.

### Profiling (FR-001)

- [X] T009 [US1] Profile `parallel-deflate` decompression at 1, 4 and all cores using `/perf diagnose` and `CRUSH_TIMINGS=1`, covering `crush-parallel/src/engine.rs` and `crush-core/src/decompression.rs`, and record the share of time in decode, payload copies, outer CRC, page faults, and output writing in `perf/log/2026-11-hybrid-speed-codec.md`
- [X] T010 [US1] Profile `parallel-deflate` compression the same way, and record the shares in `perf/log/2026-11-hybrid-speed-codec.md`
- [X] T011 [US1] Rank the R11 hypotheses in `specs/026-hybrid-speed-codec/research.md` against T009 and T010, mark each confirmed or rejected, and raise the SC-005 targets in `specs/026-hybrid-speed-codec/spec.md` if profiling shows more is achievable

### Tests for User Story 1

- [X] T012 [P] [US1] Write `BlockCodec` contract tests in `crush-parallel/tests/block_codec.rs`, covering exact output fill, error on bad payload, determinism across worker counts, and independence from other blocks, per `contracts/block-codec.md`
- [X] T013 [P] [US1] Write header tests in `crush-parallel/tests/format_v2.rs`: "Format version must be 1 or 2. Codec id must be known, and v1 implies DEFLATE", plus older-version and unknown-codec errors
- [X] T014 [P] [US1] Write cancellation tests in `crush-parallel/tests/cancel.rs` asserting a set flag stops both directions before remaining blocks run (R12)
- [X] T015 [US1] **Approval checkpoint**: Present T012-T014 in `crush-parallel/tests/` to the maintainer, then confirm they fail

### Implementation for User Story 1

- [X] T016 [US1] Add the `BlockCodec` trait and `DeflateCodec` (`CODEC_ID = 0`) in `crush-parallel/src/codec.rs`, moving DEFLATE specifics and the incompressible probe out of `crush-parallel/src/block.rs`
- [X] T017 [US1] Make `compress` and `decompress_from_reader` generic over `BlockCodec` in `crush-parallel/src/engine.rs` with static dispatch, and keep existing public functions as DEFLATE entry points
- [X] T018 [US1] Make random access generic in `crush-parallel/src/index.rs`
- [X] T019 [US1] Accept format versions 1 and 2 and read the codec id from header byte 22 in `crush-parallel/src/format.rs`, keeping v1 output byte-identical (R9)
- [X] T020 [US1] Run `/perf check` and T008 to confirm the refactor changes neither bytes nor speed by more than 5%, and record it in `perf/log/2026-11-hybrid-speed-codec.md`. Golden files pass; the one flagged row, `decompress_medium/1MB`, is a Windows layout effect in zlib, not a code regression
- [X] T021 [US1] Add a per-block cancel check to both pipelines in `crush-parallel/src/engine.rs` (R12)
- [X] T022 [US1] Fix the top-ranked confirmed bottleneck from T011 in `crush-parallel/src/engine.rs` (expected first: borrow payloads from in-memory input instead of the serial `vec!` copies at lines 332-349), and re-measure
- [X] T023 [US1] Skipped: T009 rejected this hypothesis, because the outer CRC in `crush-core/src/decompression.rs` costs 1.4 ms
- [X] T024 [US1] Fix serial output: positional writes were measured and do not scale, so blocks decode into a preallocated mapped file instead, under approved exception E1, in `crush-cli/src/commands/mapped_output.rs`, `crush-cli/src/commands/decompress.rs` and `crush-core/src/decompression.rs`
- [X] T025 [US1] Generalise `--block` to any CRSH codec in `crush-cli/src/commands/decompress.rs`, replacing the `parallel-deflate`-only check, per `contracts/cli.md`
- [X] T026 [US1] Verify SC-005 and SC-009 with `perf/compare.py --abba`, and record the result per fix in `perf/log/2026-11-hybrid-speed-codec.md`. All-core gate passes; the 4-core miss (2.86x through the harness, 3.1x in process) was accepted by the maintainer on 2026-10-31
- [X] T027 [US1] Run `detect-duplicates.ps1 -Json`, extract shared code, and record it in `specs/026-hybrid-speed-codec/cleanup-summary.md`

**Checkpoint**: The engine fix can ship by itself. If SC-005 is missed, record the measured reason and agree the next step with the maintainer before moving on.

---

## Phase 4: User Story 2 - Compress tighter and faster (Priority: P2)

**Goal**: `sprint` matches zstd -3's ratio and beats its speed on multi-core machines.

**Independent Test**: `crush compress --plugin sprint`, decompress, compare bytes, then compare with live zstd -3 on `real`.

### Pre-design measurement

- [X] T028 [US2] Run the block-size sweep from `specs/026-hybrid-speed-codec/quickstart.md` step 3 at 1, 2, 4 and 8 MiB, record it in `perf/log/2026-11-hybrid-speed-codec.md`, and finalize R5 in `specs/026-hybrid-speed-codec/research.md`
- [X] T029 [US2] Measure `zstd -3 --no-compress-literals` on `real`, record it in `perf/log/2026-11-hybrid-speed-codec.md`, and note which features pay for themselves

### Tests for User Story 2

- [X] T030 [P] [US2] Write bit-stream and Huffman unit tests in `crush-sprint/tests/huffman.rs`, including "Code lengths must form a complete code with a maximum of 11 bits"
- [X] T031 [P] [US2] Write tANS unit tests in `crush-sprint/tests/tans.rs`, including "Normalized counts must sum to the table size" and table-log maximums of 9, 8 and 9
- [X] T032 [P] [US2] Write `proptest` round trips over empty, tiny, random, repetitive and mixed inputs in `crush-sprint/tests/roundtrip.rs`
- [X] T033 [P] [US2] Write malformed-input tests in `crush-sprint/tests/malformed.rs` covering every validation rule in `specs/026-hybrid-speed-codec/data-model.md`, including "Match offsets must not exceed bytes already produced in the block, and must be non-zero", "Output must never exceed the segment's regenerated size", and "Repeat mode with no earlier table in the block is an error"
- [X] T034 [P] [US2] Write format tests in `crush-sprint/tests/format.rs` for header layout, `sprint_version`, reserved segment type 3, and "Trailing bytes after the last segment are an error"
- [X] T035 [P] [US2] Write plugin tests in `crush-sprint/tests/plugin.rs` for magic `43 52 01 05`, detection, cancellation, and selection staying unchanged with sprint registered
- [X] T036 [P] [US2] Add CLI tests in `crush-cli/tests/sprint.rs` for file and piped round trips, `--block`, error cases, and Ctrl-C leaving no partial output, per `contracts/cli.md`
- [X] T037 [US2] **Approval checkpoint**: Present T030-T036 in `crush-sprint/tests/` and `crush-cli/tests/sprint.rs` to the maintainer, then confirm they fail

### Prototype (go/no-go)

- [X] T038 [US2] Implement safe bit reader and writer in `crush-sprint/src/bits.rs`, using fixed-size reads and masked table indices (R3)
- [X] T039 [US2] Implement Huffman build, encode, and 4-stream decode in `crush-sprint/src/huffman.rs`, with 11-bit maximum code length and fixed-size masked tables
- [X] T040 [US2] Implement the two-table match finder in `crush-sprint/src/matcher.rs`: 8-byte hashes at 2^17 entries, 5-byte at 2^16 (maintainer decision after T028), repeat-offset probe at ip+1, and step growth
- [X] T041 [US2] Create `crush-sprint/examples/codec_bench.rs`, and measure T039 and T040 on `real` against live zstd in memory, recording results in `perf/log/2026-11-hybrid-speed-codec.md`
- [X] T042 [US2] Inspect optimized code for T039 and T040 and remove remaining hot-loop bounds checks by safe means, recording before and after figures in `perf/log/2026-11-hybrid-speed-codec.md`
- [X] T043 [US2] **Go/no-go**: Report T041 and T042 from `perf/log/2026-11-hybrid-speed-codec.md` to the maintainer. At 0.8x of zstd or better, continue. Below 0.8x, go to T046 before continuing

### Implementation for User Story 2

- [X] T044 [US2] Implement tANS table build, encode and decode in `crush-sprint/src/tans.rs`, with predefined, RLE, explicit and repeat modes, and maximum table logs of 9, 8 and 9
- [X] T045 [US2] Implement byte statistics and sampled early exit in `crush-sprint/src/histogram.rs` (R8)
- [ ] T046 [US2] **Only if a wall is measured**: Create `specs/026-hybrid-speed-codec/unsafe-exceptions.md` with the fields in `data-model.md`, try safe alternatives first, escalate hard diagnosis to Fable within Pro plan usage (stating it when done), and get maintainer approval before any `unsafe`. An approved exception lives in one audited module, the rest of `crush-sprint` keeps `deny(unsafe_code)`, and it needs fuzz coverage and Miri
- [X] T047 [US2] Implement segment and section headers in `crush-sprint/src/format.rs` per `contracts/sprint-format.md`, with "Regenerated size must be at most 128 KiB" and all section bounds checks
- [X] T048 [US2] Implement sequence encode and decode in `crush-sprint/src/sequences.rs`, with repeat offsets starting at 1, 4, 8
- [X] T049 [US2] Implement block compression in `crush-sprint/src/codec.rs`: 128 KiB segments, block entropy state, table reuse, and raw and RLE segments
- [X] T050 [US2] Implement block decompression in `crush-sprint/src/decompress.rs`, enforcing "Segment sizes must sum exactly to the block's uncompressed size" and "Bitstreams must end exactly at their initial position"
- [X] T051 [US2] Implement `BlockCodec` with `CODEC_ID = 1` for sprint in `crush-sprint/src/codec.rs`, using 4 MiB blocks (T028)
- [X] T052 [US2] Add provided method `auto_selectable()`, defaulting to `true`, in `crush-core/src/plugin/contract.rs`, and skip plugins returning `false` in `crush-core/src/plugin/selector.rs` (R14)
- [X] T053 [US2] Implement and register `SprintPlugin` in `crush-sprint/src/lib.rs` with magic `43 52 01 05` and `auto_selectable() == false`
- [X] T054 [US2] Wire `crush-sprint` into `crush-cli/Cargo.toml`, and keep `crush plugins list` and `crush inspect` correct in `crush-cli/src/commands/plugins.rs` and `crush-cli/src/commands/inspect.rs`
- [X] T055 [P] [US2] Add fuzz targets `fuzz_decompress`, `fuzz_roundtrip` and `fuzz_tables` in `crush-sprint/fuzz/fuzz_targets/`
- [X] T056 [P] [US2] Add suite `sprint` to `.claude/skills/perf/bench.py` and benchmarks in `crush-sprint/benches/codec.rs`

### Format experiments and freeze

- [X] T057 [US2] Test table persistence across segments against reset per segment (R6), using `crush-sprint/src/codec.rs`, and record it in `perf/log/2026-11-hybrid-speed-codec.md`
- [ ] T058 [US2] Test repeat offsets on versus off, and shared versus per-field sequence bit streams, in `crush-sprint/src/sequences.rs`, and record it in `perf/log/2026-11-hybrid-speed-codec.md`
- [ ] T059 [US2] Tune predefined distributions in `crush-sprint/src/tans.rs` using all benchmark corpora, without over-fitting `real`
- [X] T060 [US2] Try the tag-filtered bucket match finder in `crush-sprint/src/matcher.rs` (R7), and keep it only if it wins on a same-session run
- [ ] T061 [US2] Freeze the format with maintainer approval: complete and mark final `specs/026-hybrid-speed-codec/contracts/sprint-format.md`, then add golden files in `crush-sprint/tests/golden/`

### Verification

- [ ] T062 [US2] Run all fuzz targets for at least 100,000 iterations each (SC-007) and fix every finding, in `crush-sprint/fuzz/`
- [ ] T063 [US2] Measure memory per thread (SC-006) and single-block retrieval time (SC-008), and record them in `perf/log/2026-11-hybrid-speed-codec.md`
- [ ] T064 [US2] Run `detect-duplicates.ps1 -Json`, extract shared code, and update `specs/026-hybrid-speed-codec/cleanup-summary.md`

**Checkpoint**: Sprint works by name, automatic selection is unchanged, and the format is frozen.

---

## Phase 5: User Story 3 - Trust the speed claim (Priority: P3)

**Goal**: Any claim is reproducible from one command.

**Independent Test**: The head-to-head produces every SC metric, and names any failure.

### Tests for User Story 3

- [ ] T065 [US3] Write tests in `perf/test_compare.py` for multiples, the under-10% rule, and failed-gate naming, then get approval and confirm they fail

### Implementation for User Story 3

- [ ] T066 [US3] Add a `crush-sprint` row to `tools()` in `perf/compare.py`
- [ ] T067 [US3] Add a gate report to `perf/compare.py` that evaluates SC-001 to SC-005, SC-010 and SC-011 and names each failed metric with its multiple
- [ ] T068 [US3] Add a log or text-like corpus to `DATASETS` in `perf/compare.py`
- [ ] T069 [US3] Run the gates on all corpora, rotating builds for wins under 10%, and record them in `perf/log/2026-11-hybrid-speed-codec.md`
- [ ] T070 [US3] If a gate fails, report the metric and cause from `perf/log/2026-11-hybrid-speed-codec.md` to the maintainer and return to Phase 4 or T046, without weakening the gate
- [ ] T071 [US3] Publish results with machine, corpus, zstd version and spread in `perf/README.md` and `README.md`, limiting claims to the conditions measured (FR-022)
- [ ] T072 [US3] Attempt Stretch A and Stretch B, and record outcomes in `perf/log/2026-11-hybrid-speed-codec.md`. Misses here do not block release

**Checkpoint**: Opt-in release ready.

---

## Phase 6: User Story 4 - Get the codec automatically (Priority: P4)

**Goal**: After all gates pass, users get sprint without naming it.

**Independent Test**: Compare selection before and after across small, large, compressible and incompressible inputs.

**Blocked until**: SC-001 to SC-011 pass and the maintainer approves promotion.

- [ ] T073 [US4] Write routing tests in `crush-cli/tests/routing.rs` for each input class, then get approval and confirm they fail
- [ ] T074 [US4] Change `auto_selectable()` to `true` in `crush-sprint/src/lib.rs`, with measured metadata
- [ ] T075 [US4] Route files of 25 MiB or more and piped input to sprint in `crush-cli/src/algorithm.rs`
- [ ] T076 [US4] Confirm no regression for inputs outside sprint's intended use, and record it in `perf/log/2026-11-hybrid-speed-codec.md`

---

## Phase 7: Polish & Cross-Cutting Concerns

- [ ] T077 [P] Update `.specify/memory/claude.md` with the CRSH v2 format, `BlockCodec`, `auto_selectable()`, and the sprint crate
- [ ] T078 [P] Update `README.md` and `CONTRIBUTING.md` with sprint usage and limits
- [ ] T079 Fix the stale constitution version in `CLAUDE.md`, and propose the amendment for head-to-head benchmarks, zstd targets, and the dependency list in `.specify/memory/constitution.md`, following its 7-day feedback process
- [ ] T080 Run the `quality-gate` agent, coverage above 80%, and every step in `specs/026-hybrid-speed-codec/quickstart.md`
- [ ] T081 Add a `plugin 0x05` row to the regression benchmark baseline with `/perf baseline`, recorded under `perf/`

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (1)**: None
- **Foundational (2)**: After Setup. Blocks all stories
- **US1 (3)**: After Foundational
- **US2 (4)**: After `BlockCodec` exists (T016-T019). Multi-core gates need all of US1
- **US3 (5)**: T065-T068 can start after Foundational. T069-T072 need US2
- **US4 (6)**: After every gate passes and the maintainer approves
- **Polish (7)**: After the stories being shipped

### Key Task Dependencies

- T007 and T008 come before any engine edit
- T009 and T010 come before T016-T024, as FR-001 requires
- T022-T024 each depend on T011 confirming that bottleneck. Skip and record any rejected hypothesis
- T043 blocks T044 onward
- T057-T060 must finish before T061, because the freeze ends format changes
- T046 runs only on a measured wall

### Within Each Story

Tests → Approval → Tests fail → Implement → Refactor → Cleanup.

### Parallel Opportunities

- T002, T003
- T012, T013, T014
- T030 to T036
- T055, T056
- US3 harness tasks T065-T068 alongside US1
- T077, T078

Performance measurement tasks must never run in parallel with each other or with builds, because they disturb results.

### Parallel Example: User Story 2 tests

```text
Task: "Write Huffman tests in crush-sprint/tests/huffman.rs"
Task: "Write tANS tests in crush-sprint/tests/tans.rs"
Task: "Write round trips in crush-sprint/tests/roundtrip.rs"
Task: "Write malformed-input tests in crush-sprint/tests/malformed.rs"
```

## Implementation Strategy

### MVP First

1. Phases 1 and 2: Measurement and safety net
2. Phase 3: Engine fix, which helps existing users by itself
3. **Stop and validate** SC-005 and SC-009, and ship through a PR to `develop`

### Incremental Delivery

1. Engine fix PR
2. Prototype result reported at T043
3. Sprint as opt-in once gates pass
4. Published head-to-head
5. Promotion to automatic selection, as a separate decision

### Exit Conditions

- SC-005 missed after confirmed bottlenecks are fixed: report the measured limit
- Safe code below 0.8x at T043: exception process, with no C fallback
- Gates missed after T046: report the gap and let the maintainer decide. Gates are never lowered to pass
