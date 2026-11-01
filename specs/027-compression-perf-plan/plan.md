# Implementation Plan: Parallel Fast Compression

**Branch**: `027-compression-perf-plan` | **Date**: 2026-11-01 | **Spec**: [spec.md](spec.md)

## Summary

Run `fast-lz` blocks and `crush-gpu` tiles across cores with Rayon inside each crate, keeping both formats byte-identical. Add the small single-thread fixes that measurement supported. This feature does not depend on, or change, `crush-sprint`.

## Technical Context

**Language**: Rust 1.99.1, pinned in `rust-toolchain.toml`
**Dependencies**: `rayon`, already allowed by the constitution and used by `crush-gpu`. It is new to `crush-lz` only.
**Testing**: `cargo test`, `proptest`, output pinned by hash against the previous build
**Benchmarks**: `crush-lz/examples/codec_bench.rs`, `perf/compare.py`, `crush-gpu/benches/throughput.rs`
**Constraints**: no `unsafe`, no format change, no `.unwrap()` in production code

## Constitution Check

| Gate | Status | Note |
|---|---|---|
| Performance First | PASS | Every change has a measured gain in `research.md` |
| Correctness & Safety | PASS | Safe Rust; bytes pinned; no decode or format change |
| Modularity | PASS | Each change stays inside its crate |
| Test-First | PASS | Output and cancellation tests come first and must pass on the old code |
| Dependencies | PASS | `rayon` is on the allowed list |
| Quality gates | PASS | Standard gates plus `/perf check`. Fuzz runs because compressor output reaches decoders |

## Design

**`crush-lz`** (`crush-lz/src/compress.rs`)

- A worker `Compressor` owns a reusable output buffer, so `push_block` no longer zero-fills 1 MB per block. A block's bytes do not depend on the table's old contents, because older entries fail the range check.
- Blocks go through in batches of `threads * 8`. Each batch compresses in parallel, then goes out in order. This keeps `compress_to` memory bounded, and cancellation is checked per batch.
- With one thread, or one block, the existing serial loop runs, because the parallel path measured about 7% slower on one core.
- `find_sequences` loads the 8-byte word once.

Not `BlockCodec`: it would move fast files into the CRSH container and change the format, which FR-002 forbids.

**`crush-gpu`** (`crush-gpu/src/gdeflate.rs`, `crush-gpu/src/engine.rs`)

- `BitWriter` uses a 64-bit accumulator.
- Fixed Huffman codes and the length and distance lookups are built once, in a `LazyLock`.
- `compress` splits per-tile work from layout: tiles compress in parallel, then a serial pass writes them in order.

`lz77_find_matches` does not change. `lz77.rs` is used only by tests.

**`perf/compare.py`**: remove `crush-fast-lz` from `SINGLE_THREADED`.

## Delivery Order

1. Pin output bytes and cancellation (tests pass on the old code).
2. `crush-lz` single-thread fixes, then parallel batches.
3. `crush-gpu` encoder fixes, then parallel tiles.
4. Measure against SC-001 to SC-006, run `/perf check` and fuzz.

## Risks

| Risk | Mitigation |
|---|---|
| Output depends on thread count | Hash-pinned tests at several pool sizes |
| Library callers now use the global Rayon pool | Serial path on one thread; `ThreadPool::install` still applies |
| Results vary with binary layout | Judge the small single-thread changes on `/perf check`, not on one build |
