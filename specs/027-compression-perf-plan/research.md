# Research: Parallel Fast Compression

Replaces the original audit. Each suggestion was checked against the code and, where possible, measured.

Measured 2026-11-01 with patches on `a8cdfe4`, i9-12900K under WSL2, 128 MiB `real` in tmpfs (16 MiB for `crush-gpu`).

## Decisions

### R1: Parallel `fast-lz` inside the crate, not through `BlockCodec`

- **Decision**: Rayon over blocks in `crush-lz`, same frame format.
- **Rationale**: Bytes matched the serial output exactly on `real`. Throughput by CPU set (`codec_bench`, median MB/s):

| CPUs | 0 | 0,2,4,6 | 0-7 | 0-15 | 0-23 |
|---|---|---|---|---|---|
| MB/s | 668 | 2,286 | 3,749 | 4,869 | 5,182 |

- **CLI**: about 226 / 76 / 52 ms at 1 / 4 / all cores, or about 590 / 1,770 / 2,580 MB/s. `lz4 -1` was 603 / 1,601 / 1,973 MB/s through `compare.py`, a different harness, so SC-002 is measured live.
- **Alternatives**: `BlockCodec` moves files to the CRSH container, so "bit-for-bit compatible" cannot hold.

### R2: Keep the single-core path

- **Decision**: Serial loop on one thread.
- **Rationale**: The parallel build ran about 7% slower on one core (668 vs 717 MB/s).

### R3: Small `fast-lz` fixes

- **Decision**: Keep them, as cleanups.
- **Rationale**: Single load about +1.7%; single load plus a reused safe buffer about +3.2% (eight rounds of medians). Both are inside this benchmark's layout swing, and the original +15-25% was not found. `Vec::spare_capacity_mut` was not used, because it needs `unsafe` and the safe buffer did the job.

### R4: `crush-gpu` encoder

- **Decision**: 64-bit accumulator and parallel tiles. Leave the match finder.
- **Rationale**: 47 to 106 MB/s from the accumulator alone, with the same output hash. Parallel tiles reached about 590 MB/s alone and about 930 MB/s with the accumulator, on the tile loop without assembly.
- **Rejected**: Replacing `HashMap` in `lz77.rs`. Only tests call `lz77_encode`. The real path, `lz77_find_matches` in `gdeflate.rs`, already uses one flat table.

## Rejected suggestions

| Suggestion | Why |
|---|---|
| Recycle `crush-parallel` worker buffers | Compress changed -0.1%, +2.9%, -3.3% at 1 / 4 / all cores, all inside spread |
| Zero worker allocations | Each block's bytes must live until assembly, so one allocation per block remains without a shared output arena |
| `capacity()` check in `DeflateCodec` | `out[..bound]` needs length, and growing it zero-fills |
| Parallel outer CRC | 1.4 ms on `real`, already rejected as T023 of feature 026 |
| Mapped compress output | Block offsets depend on earlier compressed sizes, it needs a new `unsafe` approval, and writes are not the main cost |
| `compress_into` for deflate plugins | Assembly is about 6.5 of 114 ms at all cores; the extra copy was not measured separately. Deferred |
| SIMD literal copies | `put_literals` already copies in `WILD`-sized chunks |
