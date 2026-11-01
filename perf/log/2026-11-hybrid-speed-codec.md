# Hybrid high-speed codec results (feature 026)

Machine, zstd version, build and spread are recorded with every result. Spec: `specs/026-hybrid-speed-codec/spec.md`.

## Baselines

2026-10-31, i9-12900K under WSL2, zstd 1.5.7, 128 MiB, 10 reps, ABBA, commit 6d213da plus setup. Median MB/s, with spread in brackets.

| `real` | 1 core | 4 cores | all cores | ratio |
|---|---|---|---|---|
| crush compress | 121.7 (4%) | 449.0 (7%) | 1,048.7 (8%) | 3.503 |
| crush decompress | 709.9 (54%) | 1,531.2 (22%) | 1,843.5 (15%) | |
| zstd -3 compress | 188.8 (12%) | 1,004.3 (19%) | 1,580.3 (13%) | 3.70 |
| zstd -3 decompress | 958.8 (9%) | 1,172.8 (3%) | 1,166.7 (9%) | |
| zstd -1 compress | 250.8 (5%) | 1,439.8 (30%) | 2,580.6 (66%) | 3.233 |
| zstd -1 decompress | 1,045.8 (13%) | 1,274.3 (93%) | 1,260.9 (2%) | |

| `text` | 1 core | 4 cores | all cores | ratio |
|---|---|---|---|---|
| crush compress | 109.2 | 379.3 | 784.0 | 1.324 |
| crush decompress | 444.9 | 985.0 | 1,197.8 | |
| zstd -3 compress | 463.6 | 1,629.2 | 2,209.8 | 1.327 |
| zstd -3 decompress | 894.3 | 1,089.5 | 1,087.0 | |

In memory, single-threaded, `zstd -b1e3 --single-thread -i5`:

| Level | `real` compress | `real` decompress | `real` ratio | `text` compress | `text` decompress |
|---|---|---|---|---|---|
| 1 | 621.8 | 1,752.5 | 3.241 | 1,589.6 | 1,545.4 |
| 2 | 522.5 | 1,718.9 | 3.502 | 1,585.2 | 1,540.0 |
| 3 | 380.9 | 1,586.5 | 3.720 | 1,355.3 | 1,541.4 |

Findings:

- The in-memory floor at 0.85x is 324 MB/s compress and 1,349 MB/s decompress on `real`.
- zstd -3 is 2.0x faster in memory than through the CLI at one pinned core (380.9 vs 188.8). The one-core CLI gate is much easier than the in-memory gate.
- zstd -2 already reaches Crush's DEFLATE ratio (3.502 vs 3.503), in memory at 522.5 MB/s.
- Single-run spreads reach 93% on some rows, even with ABBA, so a multiple is meaningful only with its spread. Medians from 10 reps were stable against the 2026-10-30 file: ratios matched exactly and crush rows moved under 10%.

## Profiles

Timed directly, on `real`, 128 MiB, 9-run medians. `samply` was not needed, because the phases account for the total.

**Decompression (`parallel-deflate`)**

| Phase, ms | 1 core | 4 cores | all cores | Nature |
|---|---|---|---|---|
| CLI total | 171 | 82 | 67 | |
| Whole in-memory decompress | 137.8 | 46.1 | 29.8 | |
| Serial payload copies (within the line above) | 12.0 | 12.0 | 12.3 | Serial, removable |
| Remaining parallel decode | 125.8 | 34.1 | 17.5 | Scales 3.7x and 7.2x |
| Output write to tmpfs | ~35 | ~35 | ~35 | Serial |
| Outer CRC over compressed data | 1.4 | 1.9 | 1.4 | Negligible |
| Freeing output | 0.2 | 0.2 | 0.2 | Negligible |

Decode itself scales well. The output write is 52% of all-core CLI time, and with the copy it is 70%.

**Compression (`parallel-deflate`)**: 1,090 / 284 / 107 ms at 1 / 4 / all cores, which is 3.8x and 10.2x. Reading input costs 9.5 ms. No bottleneck needs a fix for the current gates.

**Ways to write 128 MiB to one tmpfs file**, 7-run medians, ms:

| Method | 1 thread | 4 | 8 | 16 |
|---|---|---|---|---|
| Single `write_all` | 34.9 | | | |
| Positional writes, one file (safe) | 38.1 | 37.1 | 37.1 | 39.5 |
| Shared `mmap` copy (needs `unsafe`) | 50.2 | 21.7 | 17.9 | 19.2 |

Positional writes do not scale at all, which points to per-file write serialization in the kernel. A writer can therefore not go below about 35 ms for this file in safe Rust, about 3.8 GB/s.

**Consequence**, computed from the figures above, not measured:

| Gate | Needs | Safe Rust, copies removed | Result |
|---|---|---|---|
| SC-005 at 4 cores | ≥ 3.0x | 161 → 69 ms = 2.3x | **Missed** |
| SC-005 at all cores | ≥ 3.3x | 161 → 53 ms = 3.0x | **Missed** |
| SC-003 at all cores, sprint at 1,349 MB/s decode | ≥ 3.0x, 3,500 MB/s | 99/7.2 + 35 = 49 ms, 2,740 MB/s, 2.35x | **Missed** |

Decoding directly into a shared `mmap` removes both the write and the extra output buffer. Scaling would approach decode's 3.7x and 7.2x, less tmpfs fault costs that are not yet measured.

## Engine results (User Story 1)

Changes: decode borrows payloads instead of copying them, and blocks decode straight into a preallocated mapped output file (exception E1 in `specs/026-hybrid-speed-codec/unsafe-exceptions.md`).

`perf/compare.py --builds before,after --abba --reps 15`, working tree on 6d213da, i9-12900K under WSL2, tmpfs, zstd 1.5.7. MB/s, with spread in brackets.

| `real` decompress | 1 core | 4 cores | all cores |
|---|---|---|---|
| crush before | 733.6 (10%) | 1,598.4 (30%) | 1,960.1 (14%) |
| crush after | 847.4 (12%) | 2,425.7 (5%) | 3,692.5 (26%) |
| zstd -3 | 1,019.1 (25%) | 1,154.1 (14%) | 1,157.6 (4%) |
| after vs zstd -3 | 0.83x | **2.10x** | **3.19x** |

| `text` decompress | 1 core | 4 cores | all cores |
|---|---|---|---|
| crush before | 453.3 | 1,030.7 | 1,278.7 |
| crush after | 508.4 (75%) | 1,568.4 | 3,118.2 |
| zstd -3 | 909.3 | 1,084.5 | 1,079.5 |

Compression moved by under 2% on every `real` row, inside its spread. Ratios are unchanged.

**Scaling on `real`, compared with 1 core**

| Source | 4 cores | all cores |
|---|---|---|
| Before, compare.py | 2.18x | 2.67x |
| After, compare.py | **2.86x** | 4.36x |
| After, `CRUSH_TIMINGS`, three runs (158 / 51 / 27 ms) | 3.1x | 5.8x |

SC-005 needs 3.0x and 3.3x. The all-core gate passes both ways. At 4 cores the harness shows 2.86x, which includes process start and the shell, and 3.1x inside the process. The harness figure governs gates, so this is a narrow miss. The maintainer accepted it on 2026-10-31 with this note, and the 3.0x target in the spec stands.

On ext4, three runs each, seconds at 1 / 4 / all cores: before 0.19 / 0.16 / 0.14, after 0.17 / 0.05 / 0.04. One-decimal `time` output, so these are rough.

**`/perf check`** (Windows criterion, against 649cf2e, quick run): `decompress_throughput/threads/default` -37.9% (4,651 → 7,495 MB/s), `threads/1` -4.6%, and `compress_large/10MB` -4.5%. One regression, `decompress_medium/1MB` at +22.7%. A full ABBA run confirmed it at +17.5% (CI +16.2% to +18.7%, 457 → 537 µs).

**`decompress_medium/1MB` diagnosis**, in WSL:

- Instructions are identical: 6,641,965 on both sides, with an empty changed table.
- Wall time is flat: -0.6%, inside the 1.6% spread between runs of the same build.
- About 99.8% of the time is in zlib and libc, and the input is 1 MB of one byte.
- Class from the `/perf` table: flat on WSL, slower on Windows, so platform-specific. No other decompress row slowed down.

Windows `profile-diff --env here` settled it: **layout, not a code regression.** Six ABBA runs per binary, mean of criterion medians:

| Binaries | Base 649cf2e | Working tree | Delta |
|---|---|---|---|
| Original A/B (no debug info) | 445.1 µs | 527.6 µs | +18.5% |
| Profiled (`debug=true`, same sources) | 535.5 µs | 445.7 µs | -16.8% |

- Times fall into two states, about 445 and 530 µs, and each source side lands in both. Adding debug info alone reverses the sign.
- The hot loop is the overlapping byte copy in the statically linked zlib (`src/zlib/inffast.c:254-256`). Its 40 bytes are identical in all four binaries, and `inflate_fast` accounts for about 84 of the 90 µs gap.
- Moving only that loop in patched scratch copies took a fast binary from 445 to about 535 µs, and changed a slow one by up to 2x across positions.
- No simple alignment rule fits: positions 0 and 32 mod 64 differ by 2x. The microarchitectural mechanism is unknown, because WSL exposes no hardware counters and this run did not collect any.
- Windows instruction counts were not measured. The patched copies add two jumps per match, so that test is not a pure move.

Consequence: `decompress_medium/1MB` can swing about ±18% between builds of identical source on Windows, so it cannot judge a 5% gate there. WSL uses system zlib, so it does not show this. crush-parallel uses libdeflate and sprint will use neither, so neither is on this loop.

**Fuzzing**: 100,000 runs each of `fuzz_decompress` and `fuzz_decompress_into`, seeded with golden files, clean (coverage 822 and 837). Unseeded runs finish in 0 seconds and barely reach decode, so they do not count.

**Final `/perf check`** on the staged tree (Windows, quick run, against 649cf2e): `decompress_throughput/threads/default` -37.0%, `threads/1` -5.6%, compress rows flat or better. `decompress_medium/1MB` is flagged at +14.6%, within the layout swing diagnosed above and not re-profiled. `roundtrip/1024` is +4.6%, advisory.

**Miri**: four `block_codec` tests passed with no undefined behaviour under Tree Borrows, in 5.6 hours. Limits are in exception E1.

## Block-size sweep

## Prototype

## Gates
