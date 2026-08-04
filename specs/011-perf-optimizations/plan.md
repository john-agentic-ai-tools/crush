# Implementation Plan: Hot-Path Performance Optimizations

**Branch**: `011-perf-optimizations` | **Date**: 2026-04-17 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from [specs/011-perf-optimizations/spec.md](./spec.md)

## Summary

Eliminate the dominant sources of allocator traffic and redundant memory copies in the `crush-parallel` hot path so that compress and decompress throughput improves measurably on multi-core machines, peak memory drops, and random-access block lookups become O(1)/O(log N). The public API and CRSH format are unchanged. The work is partitioned into independently landable slices aligned with the spec's three user stories; the P1 slice alone delivers MVP value.

## Technical Context

**Language/Version**: Rust, stable toolchain pinned via `rust-toolchain.toml`
**Primary Dependencies**: `rayon` (parallel dispatch), `libdeflater` (DEFLATE encode/decode), `crc32fast` (checksums), `memmap2` (file mapping), `crossbeam` (scoped threads / channels, already in `workspace.dependencies`), `thiserror`
**Storage**: On-disk CRSH format, unchanged by this feature
**Testing**: `cargo test`, `proptest`, `cargo-fuzz` (≥100k iterations), `criterion` benchmarks
**Target Platform**: Tier-1 Rust targets — Linux, macOS, Windows — 64-bit
**Project Type**: Single Cargo workspace (library + thin CLI)
**Performance Goals**: ≥15% compress wall-clock reduction, ≥25% decompress wall-clock reduction, ≥100× faster random-access index lookups on a 10k-block file, all measured on the reference machine defined in [quickstart.md](./quickstart.md) against the current HEAD of `develop`
**Constraints**: No change to the public API of `crush-core` or `crush-parallel`; no change to the CRSH on-disk format; no new `unsafe` that is not debug-asserted for length; peak memory ≤ 1.25× uncompressed size on both paths
**Scale/Scope**: Approximately 4 files touched in `crush-parallel/src/` (`engine.rs`, `block.rs`, `index.rs`, optional small additions to `config.rs`) and 1 file in `crush-core/src/` (`compression.rs`). No workspace-level Cargo changes expected; `crossbeam` is already declared.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|-----------|--------|-------|
| I. Performance First | **PASS** (this feature is the embodiment) | Benchmark-driven; criterion gates every slice; no speculative optimization — every change is tied to a specific hot-path finding in the review. |
| II. Correctness & Safety | **PASS with care** | The plan introduces `unsafe { Vec::set_len }` in two narrow places (compressed-buffer fill, decompressed-buffer fill) to skip zero-init. Each use is debug-asserted against the library-reported bytes-written and fuzz-exercised. No `.unwrap()`/`.expect()` added in production code. All existing round-trip and fuzz harnesses continue to exercise the new paths. |
| III. Modularity & Extensibility | **PASS** | Public traits, entry points, and builders are unchanged. Internal changes live behind existing module boundaries. The plugin architecture is not touched. |
| IV. Test-First Development | **PASS** | Each slice adds a failing benchmark assertion or regression test before the optimization is implemented, per TDD. |

**Quality gates (per constitution)**: `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo doc --no-deps`, fuzz-clean over ≥100k iterations, criterion baseline established pre-change, post-change results show no >5% regression outside the targeted hot paths. All gated in CI — no exceptions.

## Project Structure

### Documentation (this feature)

```text
specs/011-perf-optimizations/
├── spec.md              # Feature specification (done)
├── plan.md              # This file
├── research.md          # Phase 0 output — allocation-profiling notes, library constraints
├── data-model.md        # Phase 1 output — internal types that change (BlockIndex cumulative table, CompressedBlock cow payload)
├── quickstart.md        # Phase 1 output — reference benchmark commands + hardware + fixtures
├── contracts/
│   └── public-api.md    # Frozen snapshot of the public API surface that MUST NOT change
├── checklists/
│   └── requirements.md  # Spec-quality checklist (done)
└── tasks.md             # Phase 2 output — produced by /speckit.tasks, not this command
```

### Source Code (repository root)

```text
crush-core/
├── src/
│   └── compression.rs       # EDIT — remove full-input clone in compress() / compress_with_options()

crush-parallel/
├── src/
│   ├── engine.rs            # EDIT — pre-allocate output, parallelize assembly, direct-write decompress, scoped borrow of input
│   ├── block.rs             # EDIT — accept &mut Compressor / &mut Decompressor, remove per-block allocation, Cow-style stored fallback
│   ├── index.rs             # EDIT — add cumulative_uncompressed: Vec<u64>; rewrite uncompressed_offset/block_for_offset/total_uncompressed_size to use it
│   └── config.rs            # (probably no change — existing builder sufficient)
├── benches/
│   ├── throughput.rs        # EXTEND — add explicit "assembly-hot" and "decompress-writeback" benches
│   └── random_access.rs     # EXTEND — add 10k-block lookup benchmark for FR-011 / SC-004
└── fuzz/
    └── fuzz_targets/        # UNCHANGED targets; they automatically cover the new code paths
```

**Structure Decision**: This is a focused internal refactor, not a new subsystem. No new crates, no new modules, no new public APIs. All changes are edits inside the four source files listed above, plus benchmark additions. The workspace layout documented in [.specify/memory/CLAUDE.md](../../.specify/memory/CLAUDE.md) is unchanged.

## Phase 0 — Research

Outputs go to [research.md](./research.md). Topics to resolve before writing any code:

1. **libdeflater reuse semantics**. Confirm that a single `Compressor` can be reused across many `deflate_compress` calls on the same thread with no residual state bleed, and likewise for `Decompressor::deflate_decompress`. Read the upstream docs and source; write a small reproducing test if the docs are ambiguous. If reuse is unsafe, fall back to a thread-local allocation amortized across blocks (still a large win over per-block new).
2. **Scoped-thread choice for `crush-core/src/compression.rs`**. `std::thread::scope` (stable since 1.63) vs. `crossbeam::scope` (already a workspace dep). Decide based on interaction with the existing `run_with_timeout` / `run_with_timeout_and_cancel` helpers, which likely use `std::thread::spawn` today. Likely choice: `std::thread::scope` to avoid adding dependency surface; the timeout path keeps its current join-handle design but takes `&[u8]` instead of `Vec<u8>`.
3. **Parallel output assembly under rayon**. Decide between (a) computing per-block offsets on the driver thread and handing each worker a `&mut [u8]` slice via `par_iter_mut` over disjoint chunks, or (b) keeping the current "compress → collect → assemble" shape but moving the assembly into a `par_iter().for_each(|| write_to_offset(...))` using the `split_at_mut` pattern. Pick (b): it keeps the existing error-handling flow (collect `Result`s first, then assemble only on success). Document why in research.
4. **Progress-callback cost under parallel assembly**. The mutex today is acquired per block on the driver. Under a parallel assembler, we need to avoid contending the lock from every worker. Decision: keep the progress callback on the driver thread, invoked once per block in a lightweight loop after the parallel write completes. This preserves observable behavior (FR-013) while removing the serial payload-copy bottleneck.
5. **Cumulative-offset table semantics**. Decide storage: `Vec<u64>` of length `entries.len() + 1` where `cum[i] = sum of entries[0..i].uncompressed_size`. This makes `uncompressed_offset(n) = cum[n]`, `total_uncompressed_size() = cum[len]`, and `block_for_offset(off) = cum.partition_point(|x| *x <= off).checked_sub(1)`. Confirm edge cases (off = 0, off at exact boundary, off past end).
6. **Streaming (FR-015) feasibility under this feature**. Scope check: a true streaming compressor needs a producer/consumer pipeline (bounded channel + ordered writeback). That is a meaningful architectural change. Decision (pending research confirmation): defer FR-015 to a follow-up feature (`012-streaming-pipeline`) and record in research. The P2 memory story still wins big from in-memory path fixes alone.
7. **Benchmark baseline**. Before any optimization lands, commit a baseline run of the existing criterion suite against `develop` HEAD. Record numbers in [quickstart.md](./quickstart.md). Every subsequent slice is measured against that baseline.

## Phase 1 — Design & Contracts

### Public-API contract (`contracts/public-api.md`)

Record a **frozen snapshot** of every public item currently exported by `crush-core` and `crush-parallel` (function signatures, pub struct fields, pub enum variants, pub trait items). The contract is: every entry in this snapshot is preserved, byte-identical, after this feature lands. CI check: `cargo public-api diff` or an equivalent doc-scan in a pre-merge job. No new diffs allowed in the core crates.

### Internal data-model changes (`data-model.md`)

Exactly two internal types change shape; neither is `pub`-exported externally:

1. **`BlockIndex`** ([crush-parallel/src/index.rs](../../crush-parallel/src/index.rs)) gains a private `cumulative_uncompressed: Vec<u64>` field, populated once at `load_index` time. The existing `entries` field stays as-is (needed for `decompress_block` which reads `block_offset` / `compressed_size`). Methods `uncompressed_offset`, `block_for_offset`, `total_uncompressed_size` are rewritten against the new field; their signatures are unchanged.
2. **`CompressedBlock`** ([crush-parallel/src/block.rs](../../crush-parallel/src/block.rs)) changes its `payload` field from `Vec<u8>` to a small internal `enum { Owned(Vec<u8>), Borrowed(&'a [u8]) }` (or `Cow<'a, [u8]>`) so that the stored-block fallback can borrow from the input rather than copy. The struct may need a lifetime parameter `CompressedBlock<'a>` — this is acceptable because the type is `pub` but all exposed methods already take `&[u8]` or consume the struct internally. If a lifetime proves disruptive to call sites, fall back to keeping `Vec<u8>` and instead route the stored fallback through a dedicated assembler API that writes directly from `&input` into the output buffer (i.e., avoid owning a copy at all). Research will pick whichever is less invasive.

### Quickstart (`quickstart.md`)

Documents:
- Reference hardware (CPU model, core count, RAM, OS, kernel).
- Reference fixtures: location and entropy characteristics of the 1 GB mixed-entropy input, a 100 MB highly compressible input (zeros), a 100 MB random-bytes input, and a 10k-block CRSH file for random-access.
- Exact commands: `cargo bench --bench throughput --baseline pre-011`, `cargo bench --bench random_access --baseline pre-011`, `cargo test --release`, `cargo fuzz run fuzz_roundtrip -- -runs=100000`.
- Peak-RSS measurement recipe (`/usr/bin/time -v` on Linux; `Get-Process | Select PeakWorkingSet64` on Windows; `/usr/bin/time -l` on macOS).

### Constitution re-check

After Phase 1 design is written, re-run the table above. The two design decisions that warrant explicit review:
- The narrow `unsafe { set_len }` uses (Principle II). Gate: each site has a debug_assert and fuzz coverage.
- The possible `CompressedBlock<'a>` lifetime parameter (Principle III: modularity). Gate: type stays crate-private in behavior even if `pub`; no downstream caller uses it. Confirmed by the public-API snapshot diff.

If either gate would not hold, the plan falls back to the less invasive variant described above.

## Implementation Slices (aligned with user stories)

Each slice is a single commit-topic that can land independently, preserves round-trip correctness, and moves a benchmark number. Slices are ordered by ROI so that the MVP (US1) is reachable as early as possible.

### Slice A — Pooled compressors/decompressors (US1, P1)

**Maps to**: FR-007. Review finding #1.

**Change**: Replace `par_iter().map(...)` in `engine.rs::compress` and the parallel decompress loop with `par_iter().map_init(|| Compressor::new(lvl), |c, item| ...)` / `map_init(Decompressor::new, ...)`. Push the `CompressionLvl::new` call out of the per-block body. The `compress_block` and `decompress_block_payload` helpers in `block.rs` are refactored to take `&mut Compressor` / `&mut Decompressor` instead of constructing their own.

**Tests**: existing round-trip and proptest suites cover correctness; new micro-bench in `throughput.rs` asserts a speed-up floor (initially record-only, converted to an assertion once baseline is frozen).

**Risk**: libdeflater state bleed across calls. Mitigation: research task 1 above; fuzz target already exercises this path for ≥100k iters.

**Exit criterion**: compress throughput improves by a measurable margin (target: ≥10% of total SC-001 budget from this slice alone) with zero correctness regressions.

### Slice B — Pre-allocated compress output + parallel assembly (US1, P1)

**Maps to**: FR-008, and the serial-assembly bottleneck from review findings #3 and #6.

**Change**: After the parallel compression produces `Vec<CompressedBlock>`, compute total output size exactly (header + Σ(block_header + payload) + index header + Σ(index_entry) + footer). `Vec::with_capacity(total).` Compute per-block write offsets on the driver. Use `par_iter_mut` over disjoint slices of the output Vec (via repeated `split_at_mut`) to memcpy each block's header+payload in parallel. Progress callback stays driver-side, invoked in a cheap post-pass loop.

**Tests**: existing round-trip tests assert correctness. New benchmark in `throughput.rs` isolates the assembly phase so regressions show up without being masked by compression time.

**Risk**: offset-arithmetic bugs — mitigate by computing offsets once, asserting `cumulative == total` at the end, and fuzzing on random block sizes (already covered by proptest).

**Exit criterion**: SC-001 (≥15% compress wall-clock reduction) is met by the combination of Slices A + B.

### Slice C — Direct-write decompress buffer (US1 + US2, P1/P2)

**Maps to**: FR-009, review finding #2.

**Change**: Replace `Vec<Option<Vec<u8>>> → flatten().flatten().collect()` with a single pre-allocated `Vec<u8>` of size `index.total_uncompressed_size()` (already known). Compute each block's destination slice via the cumulative table from Slice E. Use `par_iter_mut` over disjoint chunks; each worker calls `deflate_decompress` directly into its own `&mut [u8]` slice. For stored blocks, memcpy from payload into the slice. After the parallel pass, verify CRCs (or do so inline if the scheduler permits; research task 4 will decide).

**Tests**: round-trip property tests at large sizes; corrupt-payload test continues to trigger `ChecksumMismatch` / `InvalidFormat` unchanged. New bench in `throughput.rs` for decompress throughput.

**Risk**: library-reported write lengths shorter than the header's `uncompressed_size`. Mitigation: explicit equality check against `header.uncompressed_size`; mismatch returns `InvalidFormat` rather than leaving a partially-written slice.

**Exit criterion**: SC-002 (≥25% decompress wall-clock reduction) and SC-003 memory budget met.

### Slice D — Remove full-input copy in `crush-core/compression.rs` (US2, P2)

**Maps to**: FR-010, review finding #5.

**Change**: Refactor `compress()` and `compress_with_options()` so that `input: &[u8]` is borrowed across the timeout/cancel thread boundary, using `std::thread::scope`. The `run_with_timeout` / `run_with_timeout_and_cancel` helpers grow a `_scoped` variant (or become generic over a scope) so callers can pass a borrow instead of an owned buffer. Existing non-scoped callers (if any) continue to work.

**Tests**: existing `test_compress_large` (1 MB zeros) verifies correctness; extend with a 100 MB test in release-only mode to confirm the copy is gone (measurable via peak RSS).

**Risk**: lifetime surgery in `run_with_timeout` callers. Mitigation: keep the old signature as a thin wrapper that calls the scoped variant with an owned buffer. All existing call sites compile unchanged.

**Exit criterion**: peak RSS during top-level `compress()` drops by the size of the input buffer; no API break.

### Slice E — Cumulative-offset table for `BlockIndex` (US3, P3)

**Maps to**: FR-011, review finding #9.

**Change**: In [crush-parallel/src/index.rs](../../crush-parallel/src/index.rs), extend the struct with `cumulative_uncompressed: Vec<u64>`. Populate in `load_index` right after entries are read. Rewrite `uncompressed_offset` (O(1) indexed read), `total_uncompressed_size` (return last element), and `block_for_offset` (`partition_point`, O(log N)).

**Tests**: extend existing `test_block_for_offset` with parameterized boundary cases (offset 0, last-byte, exact block-start). Add a criterion bench that performs 10k lookups on a 10k-block file (SC-004).

**Risk**: low. This is a pure-additive memoization.

**Exit criterion**: SC-004 met (≥100× faster).

### Slice F — Minor cleanup: skip zero-init + fold stored fallback (US1, P1 polish)

**Maps to**: FR-014, review findings #7, #8.

**Change**: Replace `vec![0u8; n]` in the three hot sites with `Vec::with_capacity(n)` + `unsafe { v.set_len(bytes_written) }` *after* the library call; each site gets a `debug_assert!(bytes_written <= cap)`. Fold the stored-block fallback into the direct-write pipeline so the input slice is copied exactly once (the copy that writes into the final output buffer), not twice.

**Tests**: fuzz targets (already minimum 100k iters) cover the safety contract; debug_assert fires in debug builds if the library ever regresses. Release builds rely on the library's documented invariant.

**Exit criterion**: no `vec![0u8; ...]` remain in the per-block hot path; no additional `unsafe` not paired with a debug-assert.

### Out of scope for this feature

- **FR-015 streaming**: moved to `012-streaming-pipeline` unless research task 6 proves the pipeline change is small. Recorded explicitly in [research.md](./research.md).
- **Switching compression library**: libdeflater stays.
- **GPU path changes**: out of scope; none of the findings apply to `crush-gpu`.
- **CLI changes**: zero user-visible behavior change; the CLI path benefits automatically.

## Complexity Tracking

> Fill ONLY if Constitution Check has violations that must be justified.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| Two narrow `unsafe { Vec::set_len }` sites (Principle II) | Zero-initializing ~1 MB per block × N blocks adds up to measurable memory-bandwidth waste; skipping the zero-init is a documented-idiomatic win in Rust perf work. | Leaving the zeros in place fails SC-001/SC-002 by a meaningful margin in pre-research spike measurements; `MaybeUninit<[u8]>` changes the caller surface for no safety gain over the narrow `set_len` + debug-assert pattern. |
| Possible `CompressedBlock<'a>` lifetime (Principle III) | Avoids a full copy of the input for incompressible stored-fallback blocks. | Keeping `Vec<u8>` forces a copy that defeats part of SC-003. If the lifetime parameter proves too invasive in practice, Slice F switches to the "direct-write stored fallback" variant which keeps `CompressedBlock` as-is — this variant is recorded as the fallback in the design. |

## Exit Criteria (feature-level)

Ship when, on the reference hardware documented in [quickstart.md](./quickstart.md), **all** of:

1. `cargo test --workspace` green.
2. `cargo clippy --all-targets -- -D warnings` clean.
3. `cargo doc --no-deps` clean.
4. `cargo fuzz run fuzz_roundtrip -- -runs=100000` clean.
5. `cargo bench --bench throughput` shows ≥15% compress and ≥25% decompress wall-clock reduction vs. the pre-slice-A baseline.
6. `cargo bench --bench random_access` shows ≥100× reduction on the 10k-lookup scenario.
7. Peak-RSS measurements (recipe in [quickstart.md](./quickstart.md)) show compress and decompress peak ≤ 1.25× uncompressed size.
8. `cargo public-api diff` against `develop` shows zero changes to `crush-core` or `crush-parallel` public surfaces.
9. No unrelated benchmark regresses by more than 5%.

Anything short of all of the above is a non-ship; the failing slice is reverted or fixed before merge.
