# Cleanup Summary — 011-perf-optimizations

**Feature**: 011-perf-optimizations | **Date**: 2026-04-17 | **Constitution**: v1.6.0 MVP Delivery Workflow

This document satisfies the constitution's "every feature ends with a cleanup summary" gate. It records what changed per user story, what code was removed or consolidated, what was deferred, and the baseline-vs-post numbers captured on the reference hardware.

## What shipped (by user story)

### US1 — Faster compress/decompress on multi-core machines (P1, MVP)

- **Slice A — pooled libdeflater state** ([crush-parallel/src/block.rs](../../crush-parallel/src/block.rs), [crush-parallel/src/engine.rs](../../crush-parallel/src/engine.rs)): `compress_block` and the decompress helpers now take `&mut Compressor` / `&mut Decompressor`. The driver resolves `CompressionLvl` once, then rayon's `par_iter().map_init(..)` gives each worker a reused encoder/decoder instance for its whole block stream. Eliminates per-block allocation of libdeflater's ~32 KB internal state.
- **Slice B — pre-allocated output + parallel assembly** ([crush-parallel/src/engine.rs](../../crush-parallel/src/engine.rs)): after parallel compression completes, `compress` computes the exact total output size (`FileHeader::SIZE + Σ(BlockHeader::SIZE + payload.len()) + IndexHeader::SIZE + N × BlockIndexEntry::SIZE + FileFooter::SIZE`), allocates once, partitions into disjoint per-block slices via a `split_at_mut` chain, and `par_iter_mut().zip(&compressed_blocks).for_each(..)` copies each block's header + payload in parallel. The progress callback was moved into a driver-thread post-pass to preserve the once-per-block and cancel-on-false contract (FR-013) without re-serializing parallel workers through a mutex.
- **Slice C — direct-write decompress buffer** ([crush-parallel/src/engine.rs](../../crush-parallel/src/engine.rs), [crush-parallel/src/block.rs](../../crush-parallel/src/block.rs)): `decompress_from_reader` allocates one `Vec<u8>` at the known `total_uncompressed_size`, partitions it, and each worker calls the new `decompress_block_into(&mut Decompressor, header, payload, &mut output_slice, ..)` that writes DEFLATE output or the stored-block payload directly into its slice. Eliminates the old `Vec<Option<Vec<u8>>>` + `flatten().flatten().collect()` copy.
- **Slice F — zero-init polish + safe `set_len`** ([crush-parallel/src/block.rs](../../crush-parallel/src/block.rs)): `compress_block` keeps the `vec![0u8; buf_size]` first-pass as the safe variant (libdeflater requires a valid mutable slice argument) but adds `debug_assert!(bytes_written <= buf_size)` as the documented safety contract for any future `set_len` switch. The unsafe `set_len` upgrade was deferred — see "Deferred" below.

### US2 — Lower peak RSS on `crush-core::compress` (P2)

- **Slice D — scoped borrow** ([crush-core/src/plugin/timeout.rs](../../crush-core/src/plugin/timeout.rs), [crush-core/src/compression.rs](../../crush-core/src/compression.rs)): added `pub(crate) fn run_with_timeout_scoped<'scope, 'env, F, T>` and `run_with_timeout_and_cancel_scoped` — both use `std::thread::scope` so the closure can borrow non-`'static` data. `compress` and `compress_with_options` now wrap `std::thread::scope(|s| run_with_timeout_scoped(s, .., |cancel| plugin.compress(input, cancel)))` and pass `input: &[u8]` directly. The two `let input_owned = input.to_vec();` clones at the previous [compression.rs:144](../../crush-core/src/compression.rs#L144) and [compression.rs:226](../../crush-core/src/compression.rs#L226) are gone. Net: peak RSS on this hot path drops by roughly one input-buffer size.

### US3 — Predictably fast random-access lookups (P3)

- **Slice E — cumulative-offset `BlockIndex`** ([crush-parallel/src/index.rs](../../crush-parallel/src/index.rs)): added a private `cumulative_uncompressed: Vec<u64>` of length `entries.len() + 1`, populated once in `load_index` with `saturating_add`. `uncompressed_offset(n)` → O(1) indexed read, `total_uncompressed_size()` → O(1), `block_for_offset(off)` → O(log N) via `Vec::partition_point(|&x| x <= off)`. `test_block_for_offset` was extended with boundary assertions: offset 0, every block-start, last byte of stream, total equals input length.

## Benchmark extensions (T007, T008)

- **Phase-isolated throughput micro-benches** ([crush-parallel/benches/throughput.rs](../../crush-parallel/benches/throughput.rs)): added `compress_parallel_dominated` (4 MB blocks — parallel DEFLATE cost), `compress_assembly_dominated` (16 KB blocks — assembly/index cost), `decompress_read_phase_only` (sequential `decompress_block` walk), `decompress_parallel_dominated` (full `decompress`). Each slice's effect is visible in criterion output without being masked by unchanged phases.
- **10k-block random-access bench** ([crush-parallel/benches/random_access.rs](../../crush-parallel/benches/random_access.rs)): added `lookup_10k_blocks` — builds a ≥10 000-block CRSH file (10 240 × 4 KB), runs 10 000 `uncompressed_offset` + `block_for_offset` lookups per iteration. Serves SC-004.

## Duplications / dead code removed

- The `Arc<AtomicBool> cancelled` handle in the pre-011 `engine::compress` was dead — the progress-callback cancel-on-false contract is enforced via the callback's return value, and no one checked the AtomicBool. Dropped.
- Per-block `Compressor::new()` / `Decompressor::new()` allocations (previously once per block × N blocks) are now once per worker × rayon pool size. For a 1 GB / 1 MB-block workload that is 1024 allocations → ~16 allocations on a 16-core machine.
- Per-block `Vec<u8>` output allocations during decompress (previously N × ~1 MB) are now one allocation of exactly `total_uncompressed_size`.
- Double-copy of stored blocks on the compress-assembly path (old: `input.to_vec()` in worker + `extend_from_slice` in driver) is now one parallelized `copy_from_slice` — the in-worker `to_vec()` remains as the source for `CompressedBlock.payload` but is itself parallelized across workers rather than serialized through the driver.
- Two `let input_owned = input.to_vec();` clones in `crush-core::compression` (one in `compress`, one in `compress_with_options`) are removed by the Slice D scoped-borrow.

## Deferred / follow-ups

- **FR-015 streaming for `compress_stream`** — `crush-parallel::compress_stream` still does a `read_to_end` before dispatching to `compress`. Per [plan.md](./plan.md) "Out of scope" and T122 in tasks.md, full streaming is deferred to a follow-up feature `012-streaming-pipeline` that also restructures `decompress` to stream output.
- **Unsafe `set_len` + `spare_capacity_mut` in `compress_block`** — the safe `vec![0u8; buf_size]` path is retained with a `debug_assert!(bytes_written <= buf_size)`. Switching to `MaybeUninit` + `set_len` is a fuzz-gated change worth its own PR; the measurable gain on top of Slice A is small and the risk surface is non-trivial.
- **`AssemblySource::BorrowedFromInput(&'a [u8])` enum for stored-block fallback** — variant (b) in [data-model.md](./data-model.md) chose to keep `CompressedBlock.payload: Vec<u8>` and parallelize the second copy. If the combined SC-001/SC-003 budget does not close on reference hardware, the borrowed-input variant is the next lever.

## Baseline-vs-post numbers

**Pending T001/T002/T013/T018/T024/T028/T035/T042/T045/T049 — user-run on reference hardware.** This table is populated at PR time by running the commands in [quickstart.md § Commands](./quickstart.md#commands) and recording the criterion `% change` output.

| Metric | Command | Pre-011 | Post-011 | Target | SC |
|--------|---------|---------|----------|--------|----|
| Compress 1 GB wall-clock | `cargo bench --bench throughput -- compress_1gb` | TBD | TBD | ≥ 15% less | SC-001 |
| Decompress 1 GB wall-clock | `cargo bench --bench throughput -- decompress_1gb` | TBD | TBD | ≥ 25% less | SC-002 |
| Peak RSS compress 1 GB | `/usr/bin/time -v` recipe | TBD | TBD | ≤ 1.25× input | SC-003 |
| Peak RSS decompress 1 GB | `/usr/bin/time -v` recipe | TBD | TBD | ≤ 1.25× input | SC-003 |
| 10k random-access lookups | `cargo bench --bench random_access -- lookup_10k_blocks` | TBD | TBD | ≥ 100× less total | SC-004 |
| Non-hot-path regression | `cargo bench --workspace --baseline pre-011` | 0% | TBD | ≤ 5% | SC-005 |

## Public-API diff (SC-007)

- **Intent**: strict zero-diff. New scoped-timeout variants (`run_with_timeout_scoped`, `run_with_timeout_and_cancel_scoped`) are `pub(crate)`, not `pub`, so they do not appear in the public surface. The new `cumulative_uncompressed` field on `BlockIndex` is private and not visible in `cargo public-api`.
- **Note — `crush-parallel::block::{compress_block, decompress_block_into, decompress_block_payload}`**: these are `pub fn` inside `pub mod block`, and their signatures changed in Slice A (now take `&mut Compressor` / `&mut Decompressor`). They are **not** listed among the frozen re-exports in [contracts/public-api.md](./contracts/public-api.md). Two readings:
  1. The contract's frozen surface is only the crate-root re-exports → these changes are in-bounds.
  2. Strict `cargo public-api diff` will list the signature change as a diff → this is flagged to the reviewer in the PR description and accepted under the "additive/additions-OK" reading of SC-007.
  Final call rests with the T044 user-run of `cargo public-api diff` — record the output in the PR.

## Gate status at this snapshot

- [x] `cargo clippy --all-targets -- -D warnings` — clean (T046)
- [x] `cargo doc --no-deps --workspace` — zero warnings (T047)
- [x] `cargo test --workspace` — 132 passed; one pre-existing crush-cli env-var parallelism flake (`test_save_and_load_config_roundtrip`) unrelated to this feature, confirmed via isolated run on `develop`
- [ ] `cargo fuzz run fuzz_roundtrip -- -runs=100000` (T043) — user-run on reference HW
- [ ] `cargo fuzz run fuzz_decompress -- -runs=100000` (T043) — user-run on reference HW
- [ ] `cargo public-api diff --deny=all` on both crates (T044) — user-run on reference HW
- [ ] `cargo bench --workspace --baseline pre-011` (T045) — user-run on reference HW
- [ ] Quickstart end-to-end validation (T049) — user-run on reference HW
- [ ] PR to `develop` (T050) — opened by user after T001/T049 complete
