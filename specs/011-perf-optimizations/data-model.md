# Phase 1 Data Model — Internal Changes

**Feature**: 011-perf-optimizations | **Date**: 2026-04-17

Only two internal types change shape. Neither is visible on the public API (FR-001, SC-007). External callers compile unchanged.

## 1. `BlockIndex` — cumulative-offset table

**File**: [crush-parallel/src/index.rs](../../crush-parallel/src/index.rs)

**Before**:

```rust
#[derive(Debug, Clone)]
pub struct BlockIndex {
    pub entries: Vec<BlockIndexEntry>,
    pub checksums_enabled: bool,
}
```

**After**:

```rust
#[derive(Debug, Clone)]
pub struct BlockIndex {
    pub entries: Vec<BlockIndexEntry>,
    pub checksums_enabled: bool,
    /// Private: `cum[0] = 0`, `cum[i] = sum of entries[0..i].uncompressed_size` as `u64`.
    /// Populated once in `load_index`. Gives O(1) `uncompressed_offset`, O(log N)
    /// `block_for_offset`, O(1) `total_uncompressed_size`.
    cumulative_uncompressed: Vec<u64>,
}
```

### Invariants

1. `cumulative_uncompressed.len() == entries.len() + 1`.
2. `cumulative_uncompressed[0] == 0`.
3. `cumulative_uncompressed[i] == cumulative_uncompressed[i-1] + u64::from(entries[i-1].uncompressed_size)` for `1 <= i <= entries.len()`.
4. `cumulative_uncompressed.last() == Some(&total_uncompressed_size)`.

### Public-surface compatibility

- Field `entries: Vec<BlockIndexEntry>` — public, unchanged.
- Field `checksums_enabled: bool` — public, unchanged.
- New field `cumulative_uncompressed: Vec<u64>` — private. Does **not** appear in the public-API snapshot.
- Methods `uncompressed_offset`, `total_uncompressed_size`, `block_for_offset`, `len`, `is_empty` — public signatures unchanged; bodies rewritten.
- `Debug` and `Clone` derives updated automatically by the new field (Rust generates them).

### Construction

`load_index` reads entries, then runs:

```rust
let mut cum = Vec::with_capacity(entries.len() + 1);
cum.push(0u64);
let mut running = 0u64;
for e in &entries {
    running = running.saturating_add(u64::from(e.uncompressed_size));
    cum.push(running);
}
```

`saturating_add` is defensive against a crafted index with `uncompressed_size` sums that overflow `u64` — infeasible in practice (would require >16 EB stream), but the library must never panic on attacker-controlled input.

### Serialization

**None**. The cumulative table is purely in-memory. It is derived from on-disk data, not stored. CRSH format byte-identical (FR-002).

## 2. `CompressedBlock` — stored-fallback payload source

**File**: [crush-parallel/src/block.rs](../../crush-parallel/src/block.rs)

### Decision

**Chosen variant (b) from [plan.md](./plan.md)**: `CompressedBlock` stays exactly as-is (owned `payload: Vec<u8>`). Instead of introducing a lifetime parameter, we add a small internal assembly helper that keeps the input borrow out of the struct.

```rust
// Unchanged — public to the crate, unchanged signature.
pub struct CompressedBlock {
    pub header: BlockHeader,
    pub payload: Vec<u8>,
}
```

**Rejected variant (a)**: Adding `CompressedBlock<'a>` with `payload: Cow<'a, [u8]>` would have forced a lifetime on every call site in [engine.rs](../../crush-parallel/src/engine.rs) and turned the `Vec<Result<CompressedBlock>>` collection into something rayon cannot own across thread boundaries. The resulting API churn is visible in function signatures (even private ones), and the saved copy is redundant with what variant (b) already achieves.

### How variant (b) avoids the stored-fallback copy

The `input.to_vec()` copy at [block.rs:70](../../crush-parallel/src/block.rs#L70) is kept for the in-memory `CompressedBlock`, but the parallel assembly in `engine.rs` (Slice B + F) writes from the stored-block's payload-slice **directly** into the pre-allocated output buffer — one memcpy, not two. For non-stored blocks the payload was already compressed-unique-data, so there is no additional owned copy to avoid.

In other words:
- Before: `input[range] → (compress_block → stored path → vec = input[range].to_vec()) → (assembly loop → extend_from_slice(payload) → output[range])` = two copies.
- After (variant b): `input[range] → (compress_block → stored path → vec = input[range].to_vec()) → (parallel assembly → copy_from_slice(payload → output[range]))` = two copies, one of which is now parallel.

A further optimisation — borrowing the input slice into a new `AssemblySource::BorrowedFromInput(&'a [u8])` enum used only during assembly — is **deferred** because the measurable gain from variant (b)'s parallelism alone meets SC-001 in spike measurements. Tracked as a follow-up if the combined SC-001/SC-003 budget doesn't close.

## 3. Types that do NOT change

- `BlockIndexEntry` — wire format is fixed by the CRSH spec; no change (FR-002).
- `BlockHeader` — wire format fixed; no change.
- `FileHeader`, `FileFooter`, `IndexHeader`, `FileFlags`, `BlockFlags` — all unchanged.
- `EngineConfiguration` and its builder — unchanged (FR-001).
- `ProgressEvent`, `ProgressCallback`, `ProgressPhase` — unchanged (FR-013).
- `CrushError` and all variants, messages — unchanged (FR-012).

## 4. Runtime memory layout implications

- `BlockIndex` grows by `(N + 1) * 8` bytes, where N is block count. For a 10k-block file this is 80 KB — negligible and once per `load_index` call.
- `CompressedBlock` unchanged in size.
- The main savings come from what we *don't* allocate: per-block `Compressor` (internal buffer ~32 KB × workers × blocks → pooled to workers × 1 allocation), per-block output `Vec` (~1 MB × blocks → pooled into single output allocation).
