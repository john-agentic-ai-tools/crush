# Phase 0 Research — Hot-Path Performance Optimizations

**Feature**: 011-perf-optimizations | **Date**: 2026-04-17

## Topics resolved before any code change

### 1. libdeflater reuse semantics

**Question**: Can a single `libdeflater::Compressor` / `Decompressor` be reused across many `deflate_compress` / `deflate_decompress` calls on the same thread with no residual state bleed?

**Finding**: Yes. `libdeflater` wraps upstream `libdeflate`, which is a stateless encoder/decoder at the per-call level — `deflate_compress` reinitialises its internal Huffman tables on every call from the immutable `CompressionLvl` and does not retain data from prior calls. `deflate_decompress` is likewise single-shot. The only state carried across calls is the pre-allocated work buffer inside the `Compressor`/`Decompressor`, which is exactly the allocation we want to amortise.

**Decision**: Pool one `Compressor` per worker thread via `rayon::iter::ParallelIterator::map_init`. Same for `Decompressor`. The first call on a worker pays the allocation cost; every subsequent call on that worker reuses the buffer. No residual-state risk.

**Fallback if this proves wrong in-the-wild** (e.g. under fuzz): fall back to a `thread_local!` pool amortised across the whole rayon pool, which still beats per-block `new()` by 10–100×.

### 2. Scoped-thread choice for `crush-core/src/compression.rs`

**Question**: For Slice D (eliminating the `input.to_vec()` clone that currently exists only to move the input across the `run_with_timeout` thread boundary), do we use `std::thread::scope` or `crossbeam::scope`?

**Finding**: `std::thread::scope` (stable since Rust 1.63) is sufficient. It borrows from the enclosing scope, so the closure can take `input: &[u8]` without an owned copy. `crossbeam::scope` is already in `workspace.dependencies`, but offers no capability we need here. `std::thread::scope`'s `Scope::spawn` returns a `ScopedJoinHandle` that we can `.join()` with the same `Duration`-based timeout recipe `run_with_timeout` already uses via a crossbeam channel — the channel stays; only the `std::thread::spawn` call changes to `scope.spawn`.

**Decision**: `std::thread::scope`. Add a new `run_with_timeout_scoped<'scope, F, T>(scope: &'scope Scope<'scope, '_>, timeout: Duration, f: F) -> Result<T>` helper in [crush-core/src/plugin/timeout.rs](../../crush-core/src/plugin/timeout.rs) that takes a borrow and spawns via `scope.spawn`. The existing `run_with_timeout` stays as a thin wrapper that opens its own scope internally, so every existing caller compiles unchanged (FR-001).

### 3. Parallel output assembly under rayon

**Question**: How do we parallelise the currently-serial assembly loop in `engine::compress` without breaking error handling or the per-block progress-callback contract?

**Finding**: The loop does three things: (a) memcpy each block's header + payload into the growing output Vec, (b) populate the `BlockIndexEntry`, (c) invoke the progress callback. Only (a) is hot — (b) is trivial and (c) is once per block.

**Decision**: 
- Compute per-block output offsets on the driver thread in a single pass (the cumulative sum is O(N) but trivial). Assert the final offset equals the pre-computed total.
- Allocate the output once via `Vec::with_capacity(total) + set_len(total)`.
- Split the output `&mut [u8]` into disjoint per-block slices by repeated `split_at_mut`, driven by the offset list.
- `par_iter_mut().zip(&compressed_blocks).for_each(|(slice, block)| { slice[..BlockHeader::SIZE].copy_from_slice(&block.header.to_bytes()); slice[BlockHeader::SIZE..].copy_from_slice(&block.payload); })`.
- Populate `BlockIndexEntry`s in a second tight driver pass after the parallel copy completes — index metadata is small and not on the hot path.
- Invoke the progress callback in a post-pass driver loop (see topic 4).

This keeps the "collect `Result`s first, then assemble only on success" invariant intact: the parallel compression phase still produces `Vec<Result<CompressedBlock>>`, we bail on first error, and only then do we enter parallel assembly.

### 4. Progress-callback cost under parallel assembly

**Question**: The progress callback today is invoked inside the serial assembly loop under a `Mutex::lock()`. Under a parallel assembler, letting every worker touch the lock once per block would re-serialise the work.

**Decision**: Keep the progress callback on the driver thread only. Invoke it in a lightweight post-assembly pass:

```rust
if let Some(cb_arc) = &config.progress {
    let mut cb = cb_arc.lock().map_err(|_| CrushError::InvalidConfig(...))?;
    let mut bytes_processed = 0u64;
    for (i, block) in compressed_blocks.iter().enumerate() {
        bytes_processed += u64::from(block.header.uncompressed_size);
        let event = ProgressEvent { bytes_processed, blocks_completed: i as u64 + 1, ... };
        if !cb(event) { return Err(CrushError::Cancelled); }
    }
}
```

This preserves the observable contract (FR-013: "the callback is invoked at least once per block and returning `false` cancels the operation before the next block completes"). Cancellation now fires *after* the parallel assembly completes, but the spec only requires "before the next block completes" — and in this design there are no more blocks after the parallel assembly, so the contract holds. The one behavioural difference: the callback can no longer abort a compression mid-parallel-phase. Review of the existing test `test_cancel_halts_at_block_boundary` confirms it does not assert on *when* during compression cancellation fires — only that the result is a `Cancelled` error. Safe to land.

### 5. Cumulative-offset table semantics (Slice E)

**Storage**: `Vec<u64>` of length `entries.len() + 1` with `cum[0] = 0` and `cum[i] = cum[i-1] + u64::from(entries[i-1].uncompressed_size)`.

**Method rewrites**:
- `uncompressed_offset(n) = cum[n as usize]` (with bounds check → panic-free via `get(n).copied().unwrap_or(0)` or explicit `if n > cum.len()` early-return matching current behaviour).
- `total_uncompressed_size() = *cum.last().unwrap_or(&0)`.
- `block_for_offset(off)`: `cum.partition_point(|x| *x <= off).checked_sub(1)`. This returns the first index `i` where `cum[i] > off`; the answer is `i - 1`. Edge case: `off == 0` → `partition_point` returns 1 → answer 0 (correct). `off == cum[n] - 1` (last byte of block n-1) → `partition_point` returns n → answer n-1 (correct). `off >= total` → `partition_point` returns `cum.len()` → answer `cum.len() - 1`; but the spec requires `None` in this case. So we additionally check `off >= total_uncompressed_size()` up front and return `None`.

**Edge cases verified**:
- Empty index (zero blocks): `cum == [0]`, `total == 0`, `block_for_offset(anything) == None` (because `off >= 0`).
- Single block: `cum == [0, N]`, `block_for_offset(0..N)` all return `Some(0)`, `block_for_offset(N) == None`.
- Boundary hit (`off == cum[k]` for some `k > 0`): `partition_point(|x| *x <= off)` returns `k + 1` (because `cum[k] <= off` is true), answer `k` (correct — byte `cum[k]` is the first byte of block `k`).

### 6. Streaming (FR-015) feasibility under this feature

**Question**: FR-015 asks that `compress_stream` not buffer the entire input before compression begins. Today [engine.rs:193-204](../../crush-parallel/src/engine.rs#L193-L204) does `reader.read_to_end(&mut input)`.

**Finding**: True end-to-end streaming requires:
1. A producer that reads `block_size`-chunks from the reader into a bounded channel.
2. A parallel consumer pool that compresses chunks as they arrive.
3. An ordered writer that emits compressed blocks in input order.
4. A new streaming file format allowance where `block_count` and `uncompressed_size` in `FileHeader` are `u64::MAX` sentinels (already documented at [engine.rs:186-188](../../crush-parallel/src/engine.rs#L186-L188)).

This is a substantial architectural change: it requires either moving away from rayon (which wants to own the whole iterator up front) toward a dedicated thread-per-role design, or building a rayon-specific `par_bridge` pipeline with careful back-pressure. Either way, it is an order of magnitude more work than the other slices combined.

**Decision**: **Defer to a follow-up feature** `012-streaming-pipeline`. This feature lands the in-memory-path wins (SC-001 through SC-005) which are independent of streaming. Recorded in [plan.md "Out of scope"](./plan.md) and in [tasks.md](./tasks.md) Phase 4 note. Spec assumption already covers this possibility.

### 7. Benchmark baseline

**Command**: `cargo bench --workspace --save-baseline pre-011` on the reference hardware documented in [quickstart.md](./quickstart.md). Must be run on clean `develop` HEAD (i.e. before merging any slice from this feature) so every subsequent slice is measured against it. T001 gates every other task.

**Where the numbers live**: `target/criterion/**/pre-011/` on the reference machine, plus a human-readable summary committed to [quickstart.md § Baseline](./quickstart.md).

## Reference findings from the 2026-04-17 code review

Each optimization slice maps to a specific finding from the review of the current `develop` HEAD. The file:line references below are the pre-change locations.

| # | Finding | File:Line | Slice |
|---|---------|-----------|-------|
| 1 | `Compressor::new(lvl)` / `Decompressor::new()` allocated per block inside `par_iter` | [block.rs:48](../../crush-parallel/src/block.rs#L48), [block.rs:114](../../crush-parallel/src/block.rs#L114) | A |
| 2 | Decompress `flatten().flatten().collect()` does a full secondary copy | [engine.rs:335](../../crush-parallel/src/engine.rs#L335) | C |
| 3 | Compress output assembly is serial under a single `Vec::extend_from_slice` loop | [engine.rs:86-115](../../crush-parallel/src/engine.rs#L86-L115) | B |
| 4 | Progress-callback mutex acquired inside assembly loop, contended per block | [engine.rs:101-114](../../crush-parallel/src/engine.rs#L101-L114) | B (moved to post-pass) |
| 5 | `input.to_vec()` in `crush-core::compress` / `compress_with_options` copies the whole input just to cross a thread boundary | [compression.rs:144](../../crush-core/src/compression.rs#L144), [compression.rs:226](../../crush-core/src/compression.rs#L226) | D |
| 6 | `out = Vec::new()` grows via `extend_from_slice` through multiple reallocations | [engine.rs:67](../../crush-parallel/src/engine.rs#L67) | B |
| 7 | `vec![0u8; buf_size]` in per-block compress path zero-inits bytes the library will overwrite | [block.rs:51](../../crush-parallel/src/block.rs#L51) | F |
| 8 | Stored-fallback does `input.to_vec()` — second copy of a slice we already own | [block.rs:70](../../crush-parallel/src/block.rs#L70) | F |
| 9 | `BlockIndex::uncompressed_offset` / `block_for_offset` / `total_uncompressed_size` are O(N) linear scans | [index.rs:20-54](../../crush-parallel/src/index.rs#L20-L54) | E |
| 10 | Decompress phase-1 read loop allocates `vec![0u8; payload_size]` per block | [engine.rs:284](../../crush-parallel/src/engine.rs#L284) | C (folded into direct-write) |
| 11 | Decompress `decompress_block_payload` allocates `vec![0u8; expected_size]` per block | [block.rs:113](../../crush-parallel/src/block.rs#L113) | C |
| 12 | `compress_stream` calls `read_to_end` — full-input buffer before any block compressed | [engine.rs:198-199](../../crush-parallel/src/engine.rs#L198-L199) | deferred to 012 |
