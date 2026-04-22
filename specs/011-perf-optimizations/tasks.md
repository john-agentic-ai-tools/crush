---

description: "Task list for feature 011-perf-optimizations"
---

# Tasks: Hot-Path Performance Optimizations

**Input**: Design documents from `specs/011-perf-optimizations/`
**Prerequisites**: [plan.md](./plan.md) (required), [spec.md](./spec.md) (required for user stories)
**Also referenced by later phases**: `research.md`, `data-model.md`, `quickstart.md`, `contracts/public-api.md` — these are *produced* during Phase 2 (Foundational) of this task list, not pre-existing inputs.

**Tests**: Test tasks below are limited to extending the existing test + benchmark suite. No new test frameworks. Round-trip correctness is covered entirely by the existing proptest and fuzz harnesses; new tasks only add regression guards where a specific FR needs a new assertion.

**Organization**: Tasks are grouped by user story so each story is independently implementable and deployable. User Story 1 is the MVP.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: Which user story this task belongs to (US1, US2, US3)
- File paths are absolute within the repo root

## Path Conventions

Single Cargo workspace at repo root:

- Core library: [crush-core/src/](../../crush-core/src/)
- Parallel engine: [crush-parallel/src/](../../crush-parallel/src/)
- Benchmarks: [crush-parallel/benches/](../../crush-parallel/benches/)
- Fuzz: [crush-parallel/fuzz/fuzz_targets/](../../crush-parallel/fuzz/fuzz_targets/)
- Spec docs (this feature): [specs/011-perf-optimizations/](./)

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Freeze a reproducible baseline so every later slice can be measured against it.

- [ ] T001 Capture pre-change criterion baseline for compress + decompress throughput and random-access lookup, saving to `target/criterion/` on the reference hardware by running `cargo bench --workspace --save-baseline pre-011`
- [ ] T002 [P] Capture pre-change peak-RSS baseline for full-file compress and decompress of the reference 1 GB fixture using the OS-specific recipe from [quickstart.md](./quickstart.md); record numbers in [specs/011-perf-optimizations/quickstart.md](./quickstart.md) under "Baseline"

**Checkpoint**: Baseline numbers checked in. Every later slice is measured against these.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Create the supporting documents and the targeted benchmark harness every user story depends on. No user story work begins until this phase is complete.

**⚠️ CRITICAL**: Phase 3+ depends on the public-API snapshot (T006) and the targeted micro-benchmarks (T007, T008). Everything else in Phase 2 is informational but required by the plan.

- [X] T003 Write Phase 0 research notes to [specs/011-perf-optimizations/research.md](./research.md) covering libdeflater reuse semantics, scoped-thread choice (std vs crossbeam), parallel-assembly shape, progress-callback placement, cumulative-offset table edge cases, and streaming (FR-015) deferral decision
- [X] T004 [P] Write Phase 1 internal data-model changes to [specs/011-perf-optimizations/data-model.md](./data-model.md) describing `BlockIndex.cumulative_uncompressed: Vec<u64>` and the `CompressedBlock` stored-fallback variant (Cow vs direct-write-from-input fallback) with the decision from T003
- [X] T005 [P] Write [specs/011-perf-optimizations/quickstart.md](./quickstart.md) with reference hardware, fixtures (1 GB mixed-entropy, 100 MB zeros, 100 MB random, 10k-block CRSH), exact criterion commands (`cargo bench --bench throughput --baseline pre-011`, `cargo bench --bench random_access --baseline pre-011`), test commands, fuzz command (≥100k iters), and per-OS peak-RSS recipes (Linux `/usr/bin/time -v`, macOS `/usr/bin/time -l`, Windows `Get-Process | Select PeakWorkingSet64`)
- [X] T006 [P] Write frozen public-API contract to [specs/011-perf-optimizations/contracts/public-api.md](./contracts/public-api.md) — list every `pub` item in [crush-core/src/lib.rs](../../crush-core/src/lib.rs) and [crush-parallel/src/lib.rs](../../crush-parallel/src/lib.rs) (plus their re-exports) with exact signatures; this is the frozen snapshot that must not diff
- [X] T007 Extend [crush-parallel/benches/throughput.rs](../../crush-parallel/benches/throughput.rs) with isolated micro-benchmarks for (a) compress parallel phase only, (b) compress assembly phase only, (c) decompress read-phase only, (d) decompress parallel-decompress phase only, so that each slice's effect is visible without being masked by unchanged phases
- [X] T008 [P] Extend [crush-parallel/benches/random_access.rs](../../crush-parallel/benches/random_access.rs) with a `lookup_10k_blocks` benchmark that loads a 10k-block CRSH file once and performs 10,000 `uncompressed_offset` and `block_for_offset` calls in a tight loop (serves SC-004)

**Checkpoint**: Research / data-model / quickstart / public-API contract written; baseline and phase-isolated benchmarks exist. User story work can now begin in parallel across US1, US2, US3.

---

## Phase 3: User Story 1 - Faster whole-file compression on multi-core machines (Priority: P1) 🎯 MVP

**Goal**: Reduce wall-clock time for full-file compress and decompress on multi-core machines by pooling libdeflater state across a worker's block stream, pre-allocating the output buffer, parallelizing output assembly, writing decompressed blocks directly into a pre-allocated output, and removing zero-init + double-copy in the stored-block fallback.

**Independent Test**: Run `cargo bench --bench throughput --baseline pre-011` on the reference fixture; observe ≥15% reduction on the compress scenario and ≥25% reduction on the decompress scenario (SC-001, SC-002). `cargo test --workspace` still green.

### Implementation for User Story 1 — Slice A (pooled compressors/decompressors)

- [X] T009 [US1] Change `compress_block` in [crush-parallel/src/block.rs](../../crush-parallel/src/block.rs) to take `&mut Compressor` instead of constructing its own, removing the `CompressionLvl::new` + `Compressor::new` calls from the per-block body
- [X] T010 [US1] Change `decompress_block_payload` in [crush-parallel/src/block.rs](../../crush-parallel/src/block.rs) to take `&mut Decompressor` instead of constructing its own
- [X] T011 [US1] In `compress` in [crush-parallel/src/engine.rs](../../crush-parallel/src/engine.rs), resolve `CompressionLvl` once on the driver, then replace `par_iter().map(...)` over blocks with `par_iter().enumerate().map_init(move || Compressor::new(lvl), |compressor, (i, chunk)| compress_block(compressor, chunk, i, config))`
- [X] T012 [US1] In `decompress_from_reader` in [crush-parallel/src/engine.rs](../../crush-parallel/src/engine.rs), replace the `par_iter().enumerate().map(...)` phase-2 loop with `map_init(Decompressor::new, |d, (i, (h, p))| decompress_block_payload(d, h, p, i as u64, checksums_enabled))`
- [ ] T013 [US1] Run `cargo test --workspace` + the throughput bench; confirm round-trip proptest (`proptest_compress_decompress_roundtrip`) still passes and compress throughput has improved measurably vs pre-011 baseline

### Implementation for User Story 1 — Slice B (pre-allocated compress output + parallel assembly)

- [X] T014 [US1] In `compress` in [crush-parallel/src/engine.rs](../../crush-parallel/src/engine.rs), after the parallel compression completes, compute exact total output size as `FileHeader::SIZE + Σ(BlockHeader::SIZE + payload.len()) + IndexHeader::SIZE + N × BlockIndexEntry::SIZE + FileFooter::SIZE` and allocate via `Vec::with_capacity(total)`
- [X] T015 [US1] In the same function, compute a `Vec<(usize /*block_write_offset*/, &CompressedBlock)>` on the driver thread before parallel assembly; assert that `offsets.last() + last.payload.len() + BlockHeader::SIZE == total - index_size - footer_size`
- [X] T016 [US1] In the same function, replace the sequential `for (i, block) in compressed_blocks.iter().enumerate()` assembly loop with a parallel write: use `split_at_mut` repeatedly to partition the output `&mut [u8]` into disjoint per-block slices, then `.par_iter_mut().zip(&compressed_blocks).for_each(...)` to copy each block's header + payload into its slice
- [X] T017 [US1] Move the progress-callback loop *after* the parallel assembly in [crush-parallel/src/engine.rs](../../crush-parallel/src/engine.rs) so it runs on the driver thread only; preserve the existing once-per-block invocation and the cancel-on-return-false contract
- [ ] T018 [US1] Run `cargo test --workspace` and the compress throughput bench; confirm the `test_cancel_halts_at_block_boundary` and `test_progress_callback_invoked_per_block` tests still pass, and compress wall-clock meets SC-001 (≥15% reduction vs pre-011)

### Implementation for User Story 1 — Slice C (direct-write decompress buffer)

- [X] T019 [US1] In `decompress_from_reader` in [crush-parallel/src/engine.rs](../../crush-parallel/src/engine.rs), after loading the index, compute a `Vec<(usize /*output_offset*/, usize /*uncompressed_len*/)>` by scanning `index.entries` once with a running sum; this serves Slice C without depending on Slice E (US3)
- [X] T020 [US1] In the same function, allocate the final output once via `Vec::with_capacity(total_uncompressed)` and `unsafe { set_len(total_uncompressed) }` (or `vec![0u8; total_uncompressed]` as a first-pass safe variant); add `debug_assert!(total_uncompressed <= isize::MAX as u64)` before the cast
- [X] T021 [US1] Replace the Phase-2 collect-into-`Vec<Option<Vec<u8>>>` plus final `flatten().flatten().collect()` with a `par_iter_mut` over the output's disjoint per-block slices: each worker calls `deflate_decompress(payload, &mut output_slice)` directly into its slice; for stored blocks, `output_slice.copy_from_slice(payload)`
- [X] T022 [US1] In [crush-parallel/src/block.rs](../../crush-parallel/src/block.rs), make the decompress helper return `Result<usize>` (bytes-written) rather than an owned `Vec<u8>`, and have it verify `bytes_written == header.uncompressed_size`; mismatch returns `CrushError::InvalidFormat(format!("block {i} uncompressed size mismatch: header {} vs decoded {}", header.uncompressed_size, bytes_written))`
- [X] T023 [US1] Ensure CRC verification still runs per block against the decompressed slice; on mismatch return `CrushError::ChecksumMismatch { block_index: i as u64, expected: header.checksum, actual }` with exact-same message shape as before (FR-012)
- [ ] T024 [US1] Run `cargo test --workspace` including `test_decompress_corrupt_block_detected`, `test_truncated_footer_rejected`, `test_version_mismatch_rejected`, `test_expansion_limit_exceeded`, and `proptest_compress_decompress_roundtrip`; run the decompress throughput bench and confirm SC-002 (≥25% reduction vs pre-011)

### Implementation for User Story 1 — Slice F (zero-init + stored-fallback polish)

- [X] T025 [US1] In [crush-parallel/src/block.rs](../../crush-parallel/src/block.rs), replace `let mut compressed = vec![0u8; buf_size];` with `let mut compressed = Vec::with_capacity(buf_size);` followed by `deflate_compress` writing into `compressed.spare_capacity_mut()` (or safe `&mut compressed[..buf_size]` if `set_len` is used), then `unsafe { compressed.set_len(bytes_written) }`; add `debug_assert!(bytes_written <= buf_size, "libdeflater returned {bytes_written} > capacity {buf_size}")`
- [X] T026 [US1] In [crush-parallel/src/engine.rs](../../crush-parallel/src/engine.rs), in the stored-fallback path (where `use_stored == true`), route the input-slice copy through the parallel assembly in T016 rather than allocating a new `input.to_vec()` — the fallback payload is borrowed from `blocks[i]` (the original input chunk) and memcpied once directly into the output slice; this requires adjusting `CompressedBlock` per T027 or adding a side-channel `stored_indices: Vec<usize>` that the assembly consults
- [X] T027 [US1] In [crush-parallel/src/block.rs](../../crush-parallel/src/block.rs), decide between (a) changing `CompressedBlock.payload` to a `Cow<'a, [u8]>`-style enum keyed to the input lifetime, or (b) leaving `CompressedBlock` as-is and adding a small `enum AssemblySource<'a> { Owned(Vec<u8>), BorrowedFromInput(&'a [u8]) }` used only by the assembly code. Prefer (b) if it avoids a lifetime parameter on `CompressedBlock`; document the decision in [data-model.md](./data-model.md)
- [ ] T028 [US1] Run `cargo fuzz run fuzz_roundtrip -- -runs=100000` against the updated block.rs to exercise the `set_len` safety contract under fuzz; any panic (including debug_assert firing) counts as a failure

**Checkpoint (US1 exit)**: `cargo test --workspace` green, `cargo clippy --all-targets -- -D warnings` clean, the compress throughput bench shows ≥15% reduction vs `pre-011` (SC-001), the decompress throughput bench shows ≥25% reduction (SC-002), fuzz run ≥100k iters clean. User Story 1 is independently shippable as MVP at this point.

---

## Phase 4: User Story 2 - Lower peak memory use during compress and decompress (Priority: P2)

**Goal**: Drop peak resident memory for both `compress()` and `decompress()` on the top-level `crush-core` API by eliminating the full-input clone before the timeout thread boundary. US1's Slice C already serves the decompress-side memory goal; US2 is the remaining compress-side fix.

**Note**: US2 is independent of US1. If US1 has already landed, US2 still delivers an additional memory reduction on the `crush-core::compress` / `compress_with_options` path. If US1 has *not* landed, US2 can be shipped first.

**Independent Test**: Compress a 100 MB reference input through `crush_core::compress()` with peak-RSS measurement; confirm peak RSS is measurably lower than the pre-011 baseline captured in T002 by at least the size of the input buffer (the eliminated clone). `cargo test --workspace` green.

### Implementation for User Story 2 — Slice D (scoped borrow in `crush-core`)

- [X] T029 [P] [US2] Inspect [crush-core/src/plugin/timeout.rs](../../crush-core/src/plugin/timeout.rs) to confirm `run_with_timeout` and `run_with_timeout_and_cancel` signatures; document whether they currently spawn via `std::thread::spawn` (owned closures) in [research.md](./research.md)
- [X] T030 [US2] Add a `run_with_timeout_scoped<'scope, F, T>(scope: &'scope Scope, timeout: Duration, f: F) -> Result<T>` variant to [crush-core/src/plugin/timeout.rs](../../crush-core/src/plugin/timeout.rs) that uses `std::thread::Scope::spawn` so the closure can borrow from `'scope`; preserve the existing non-scoped API as a thin wrapper that opens its own scope internally so no caller has to change
- [X] T031 [US2] Mirror the above with `run_with_timeout_and_cancel_scoped` in the same file for the cancel-token flow
- [X] T032 [US2] Refactor `compress` in [crush-core/src/compression.rs](../../crush-core/src/compression.rs) to call the scoped variant, passing `input: &[u8]` directly into the closure instead of cloning into `input_owned: Vec<u8>`
- [X] T033 [US2] Refactor `compress_with_options` in [crush-core/src/compression.rs](../../crush-core/src/compression.rs) identically, removing the `let input_owned = input.to_vec();` line
- [ ] T034 [US2] Run `cargo test --workspace` including `test_compress_basic`, `test_compress_empty`, `test_compress_large`, `test_compress_with_options_*`, and the timeout + cancellation tests
- [ ] T035 [US2] Measure peak RSS on a 100 MB input via the recipe from [quickstart.md](./quickstart.md); confirm reduction meets SC-003 (peak ≤ 1.25× uncompressed size on the compress path)

**Note on FR-015 streaming**: `compress_stream` in [crush-parallel/src/engine.rs](../../crush-parallel/src/engine.rs) still calls `read_to_end`. Per [plan.md](./plan.md) "Out of scope", streaming is deferred to a follow-up feature `012-streaming-pipeline`; no task in this phase.

**Checkpoint (US2 exit)**: Peak RSS during `crush_core::compress` of a 100 MB input drops by approximately the input size; all tests green; public-API diff still zero.

---

## Phase 5: User Story 3 - Predictably fast random-access block lookups (Priority: P3)

**Goal**: Replace linear-scan index lookups with a cumulative-offset table so that `uncompressed_offset`, `total_uncompressed_size`, and `block_for_offset` are O(1) / O(log N) per call.

**Independent Test**: Run `cargo bench --bench random_access --baseline pre-011`; confirm the `lookup_10k_blocks` scenario (added in T008) meets SC-004 (≥100× reduction vs pre-011). `test_block_for_offset` still green with identical results on every input.

### Implementation for User Story 3 — Slice E (cumulative-offset BlockIndex)

- [X] T036 [P] [US3] Extend `test_block_for_offset` in [crush-parallel/src/index.rs](../../crush-parallel/src/index.rs) with additional assertions for offset `0`, offset one-less-than-last, offset exactly on a block boundary (`uncompressed_offset(k)` for every `k in 1..=N`), and offset at the very end of the stream; these must pass against both the pre-change and post-change implementation
- [X] T037 [US3] Add a private `cumulative_uncompressed: Vec<u64>` field (length `entries.len() + 1`, `cum[0] = 0`, `cum[i] = cum[i-1] + entries[i-1].uncompressed_size as u64`) to `BlockIndex` in [crush-parallel/src/index.rs](../../crush-parallel/src/index.rs); update `Debug`/`Clone` derives as needed
- [X] T038 [US3] Populate `cumulative_uncompressed` at the end of `load_index` in [crush-parallel/src/index.rs](../../crush-parallel/src/index.rs), using a single running-sum pass over `entries`
- [X] T039 [US3] Rewrite `uncompressed_offset` in [crush-parallel/src/index.rs](../../crush-parallel/src/index.rs) as a direct indexed read: `self.cumulative_uncompressed[n as usize]` with bounds check
- [X] T040 [US3] Rewrite `total_uncompressed_size` in [crush-parallel/src/index.rs](../../crush-parallel/src/index.rs) as `*self.cumulative_uncompressed.last().unwrap_or(&0)`
- [X] T041 [US3] Rewrite `block_for_offset` in [crush-parallel/src/index.rs](../../crush-parallel/src/index.rs) using `partition_point` on `cumulative_uncompressed` (returns first index where `cum[i] > off`, then `i - 1` is the answer); handle `off >= total_uncompressed_size` → `None` the same as before
- [ ] T042 [US3] Run `cargo test --workspace` including `test_decompress_block_n`, `test_block_for_offset`, `test_random_access_does_not_read_other_blocks`; run `cargo bench --bench random_access --baseline pre-011` and confirm SC-004

**Checkpoint (US3 exit)**: Random-access benchmarks show ≥100× reduction; correctness tests pass with identical outputs; public-API diff still zero.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Feature-level quality gates from [plan.md](./plan.md) "Exit Criteria"; the cleanup-summary required by the constitution's MVP Delivery Workflow (v1.6.0).

- [ ] T043 [P] Run `cargo fuzz run fuzz_roundtrip -- -runs=100000` from [crush-parallel/fuzz/](../../crush-parallel/fuzz/) and `cargo fuzz run fuzz_decompress -- -runs=100000` — both must exit clean
- [ ] T044 [P] Run `cargo public-api diff` against `develop` HEAD for both `crush-core` and `crush-parallel`; confirm zero diffs (FR-001, SC-007). If `cargo-public-api` is not installed, `cargo install cargo-public-api` first
- [ ] T045 [P] Run `cargo bench --workspace --baseline pre-011` and inspect every non-hot-path benchmark; confirm no regression >5% vs baseline (SC-005 / constitution gate)
- [X] T046 [P] Run `cargo clippy --all-targets --all-features -- -D warnings` across the workspace
- [X] T047 [P] Run `cargo doc --no-deps --workspace` and confirm zero warnings (constitution gate)
- [X] T048 Write [specs/011-perf-optimizations/cleanup-summary.md](./cleanup-summary.md) per constitution v1.6.0 MVP Delivery Workflow: summarize what was implemented per story, any duplications removed (e.g., if T027 introduced an assembly helper, note it), any follow-ups deferred (FR-015 streaming), and baseline-vs-post numbers for SC-001 through SC-004
- [ ] T049 Run end-to-end [quickstart.md](./quickstart.md) validation: every command listed there succeeds on the reference hardware and produces numbers meeting each SC
- [ ] T050 Open a PR to `develop` with a PR body that (a) links to spec.md + plan.md, (b) includes the baseline-vs-post numbers from T049, (c) links to the `cargo public-api diff` output confirming SC-007

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1, T001-T002)**: No dependencies — baseline capture runs on `develop` before any code change.
- **Foundational (Phase 2, T003-T008)**: Depends on Setup; T007 and T008 build on top of the pre-011 baseline saved in T001. **BLOCKS** all user-story phases.
- **User Story 1 (Phase 3, T009-T028)**: Depends on Foundational. Internally, Slice A (T009-T013) → Slice B (T014-T018) → Slice C (T019-T024) → Slice F (T025-T028). Slices B and C can start in parallel after A lands (different regions of engine.rs, but both touch `engine.rs` so expect a merge — prefer sequential in a single-developer setting, parallel across developers on distinct branches).
- **User Story 2 (Phase 4, T029-T035)**: Depends on Foundational. **Independent of US1** — touches `crush-core/src/compression.rs` and `crush-core/src/plugin/timeout.rs`, which US1 does not touch.
- **User Story 3 (Phase 5, T036-T042)**: Depends on Foundational. **Independent of US1 and US2** — touches `crush-parallel/src/index.rs` only; US1 Slice C (T019) computes its offsets inline without requiring this work to land first.
- **Polish (Phase 6, T043-T050)**: Depends on any user story you intend to ship. Fuzz (T043), public-API diff (T044), criterion suite (T045) must all pass before merge.

### User Story Dependencies

- **US1 (P1)**: Can start after Foundational. No dependency on US2 or US3.
- **US2 (P2)**: Can start after Foundational. No dependency on US1 or US3.
- **US3 (P3)**: Can start after Foundational. No dependency on US1 or US2.

### Within Each User Story

- Slice A tasks (T009-T013) → then Slice B (T014-T018) → then Slice C (T019-T024) → then Slice F (T025-T028): each slice's validation task gates the next.
- Slice D tasks (T029-T035): T030/T031 (timeout.rs) must land before T032/T033 (compression.rs) since the latter call the former.
- Slice E tasks (T036-T042): T036 (test extension) first; T037 (field) before T038 (population); T037 before T039-T041 (methods); T042 (validation) last.

### Parallel Opportunities

- All T003-T006 (research.md, data-model.md, quickstart.md, contracts/public-api.md) can be written in parallel — different files, no dependencies.
- T007 and T008 extend different benchmark files and can land in parallel.
- Across user stories: US1, US2, US3 can each be assigned to a different developer after Phase 2 lands; they touch disjoint files with one coordination point (engine.rs is US1-only).
- Phase 6 tasks T043-T047 are all independent CI-style checks and can run in parallel.

---

## Parallel Example: Phase 2 (Foundational)

```bash
# Four docs authored in parallel — different files, no interlock:
Task: "Write research.md (T003)"
Task: "Write data-model.md (T004)"
Task: "Write quickstart.md (T005)"
Task: "Write contracts/public-api.md (T006)"

# Two bench extensions in parallel:
Task: "Extend throughput.rs with phase-isolated benches (T007)"
Task: "Extend random_access.rs with lookup_10k_blocks bench (T008)"
```

## Parallel Example: Post-Phase-2 team split

```bash
# Three developers, one per story, after Foundational lands:
Developer A: T009 → T028  (US1, crush-parallel/src/engine.rs + block.rs)
Developer B: T029 → T035  (US2, crush-core/src/compression.rs + plugin/timeout.rs)
Developer C: T036 → T042  (US3, crush-parallel/src/index.rs)
```

---

## Implementation Strategy

### MVP First (User Story 1 only)

1. Complete Phase 1: Setup (baseline captured).
2. Complete Phase 2: Foundational (docs, contracts, phase-isolated benches).
3. Complete Phase 3: User Story 1 (Slices A → B → C → F).
4. **STOP and VALIDATE**: run the US1 exit checkpoint — SC-001, SC-002, round-trip tests, fuzz, clippy.
5. Ship US1 as an incremental release — peak memory and random-access lookups still use the pre-011 code paths but compress / decompress throughput is measurably improved.

### Incremental Delivery

1. Setup + Foundational → foundation ready.
2. Add US1 → validate → demo (MVP: faster compression).
3. Add US2 → validate → demo (memory footprint drop on the top-level API).
4. Add US3 → validate → demo (faster random-access for downstream tools).
5. Each story passes SC-006 (all existing tests still green) independently.

### Parallel Team Strategy

With three developers after Foundational:

1. Developer A takes the biggest slice: US1 (T009-T028).
2. Developer B takes US2 (T029-T035).
3. Developer C takes US3 (T036-T042).
4. One reviewer (or all three rotating) handles Phase 6 (T043-T050) once merges land.
5. Integration: US1, US2, US3 touch disjoint files except for trivial `Cargo.lock` and shared docs; merge conflicts should be limited to [cleanup-summary.md](./cleanup-summary.md) in T048.

---

## Notes

- [P] tasks = different files, no dependencies.
- [Story] label maps each task to the user story it serves so stories remain traceable in PR descriptions.
- Every code-change task names an exact file. Validation tasks run concrete commands.
- Slice F's `unsafe { set_len }` work (T025) must land with the debug_assert and fuzz run (T028) in the same PR — never split safety primitives.
- Commit after each logical group (typically each slice). Do not batch multiple slices into one commit.
- Stop at any US checkpoint to validate independently; the plan explicitly supports shipping US1 alone as MVP.
- Avoid: vague tasks, cross-story dependencies that break independence, reintroducing `vec![0u8; N]` in the hot path during later edits, any change to the public-API contract frozen in T006.
