# Perf goal: beat pigz on incompressible data at 24 cores

Goal: +15-20% end-to-end `crush compress` throughput on 128 MB random data at
all cores (WSL2: crush 633 MB/s vs pigz -6 682, pigz -1 696). Safe Rust, no
ratio loss on compressible data, <5% regression elsewhere.
Machine: windows-12th-gen-intel-core-i9-12900k-16c24t, Ultimate Performance.
GOAL_BASE: 844698c. Benches: suite `cpu-compress-random`.

Before (criterion, noise floor median 2.9%, worst 6.2%):

| bench | time | MB/s |
|---|---|---|
| compress_incompressible/threads=default/random-128MB | 101 ms | 1,327 |
| compress_incompressible/threads=1/random-128MB | 1.09 s | 124 |
| compress_with_options_incompressible/threads=default/random-128MB | 152 ms | 883 |
| compress_throughput/threads=default/block=1024KB | 113 ms | 1,191 |

No profiler available (samply/perf not installed, WPR needs elevation).

## Exp 1 (H1a): crush-core outer format in one buffer, parallel CRC32 — KEPT

`compress_with_options` copied the payload twice (metadata+payload
concatenation, then header+CRC+that) and ran a serial CRC32. Now
`assemble_output` writes header, CRC placeholder, metadata and payload into one
exact-size Vec and `parallel_crc32` (4 MB chunks, `Hasher::combine`, ≥8 MB)
fills the CRC in. Byte-identical (golden test vs the old assembly).

| bench | A HEAD | B | Δ | 95% CI | verdict |
|---|---|---|---|---|---|
| compress_with_options_incompressible/threads=default | 152 ms | 127 ms | -16.5% | [-18.5, -14.9] | improved |
| compress_incompressible/threads=default | 101 ms | 99.9 ms | -1.0% | [-1.9, -0.2] | noise |
| compress_incompressible/threads=1 | 1.07 s | 1.07 s | +0.3% | [-0.8, +1.4] | noise |
| compress_throughput/threads=default/block=1024KB | 115 ms | 109 ms | -5.4% | [-6.2, -4.4] | noise (floor 6.2%) |
| compress_throughput/threads=1/block=1024KB | 1.34 s | 1.34 s | -0.3% | [-1.0, +0.5] | noise |
| compress_mixed/* (5 variants) | | | -4.9% … +1.7% | | improved/noise |

Wrapper overhead (with_options minus engine): 51 ms → 27 ms.

## Exp 2 (H3): per-worker DEFLATE scratch, stored blocks borrow input — KEPT (second variant)

First variant (exp2): scratch reused for every block, compressed output copied
out with `to_vec`. Random improved (-8.3%) but near-incompressible-2pct
regressed +6.8% [+2.3, +11.0] and compress_throughput default +4.1%: the copy
of each nearly-1 MB compressed payload costs more than the old calloc+truncate.

Second variant (exp2b, kept): a stored block borrows the input (`Cow::Borrowed`)
and leaves `scratch` for the next block; a compressed block takes `scratch` as
its payload (`mem::take` + truncate, no copy) and the worker callocs a new one.
`compress_block` (public) keeps its signature; the engine uses
`compress_block_with_scratch`. Golden test pins `engine::compress` output
(crc32 + length) for random, text, zeros, mixed with a partial last block.

| bench | A HEAD | B | Δ | 95% CI | verdict |
|---|---|---|---|---|---|
| compress_incompressible/threads=default | 101 ms | 93.4 ms | -7.9% | [-9.1, -6.6] | improved |
| compress_incompressible/threads=1 | 1.08 s | 1.03 s | -4.8% | [-5.5, -4.1] | improved |
| compress_with_options_incompressible/threads=default | 127 ms | 125 ms | -1.7% | [-4.9, +2.2] | noise |
| compress_mixed/near-incompressible-2pct | 53.8 ms | 51.1 ms | -5.0% | [-9.0, -1.8] | improved |
| compress_mixed/* (other 4) | | | -1.7% … +0.2% | | improved/noise |
| compress_throughput/threads=1/block=1024KB | 1.32 s | 1.35 s | +1.9% | [+1.0, +2.8] | slower, under gate |
| compress_throughput/threads=default/block=1024KB | 114 ms | 116 ms | +1.3% | [-2.7, +4.9] | noise |

## Exp 3 (H2): incompressibility probe + entropy guard — KEPT (third variant)

`compress_block_with_scratch` first asks `looks_incompressible`; if true the
block is stored without a full DEFLATE attempt. `EngineConfiguration::
incompressible_probe` (default true; false = previous behaviour).

Ratio report (`cargo test -p crush-parallel --release --test probe_ratio --
--ignored --nocapture`, 64 MB inputs, 1 MB blocks), size change probe on vs off:

| input | v1: 8 samples | v2/v3: samples + entropy guard |
|---|---|---|
| random | 0 | 0 |
| corpus | 0 | 0 |
| adversarial-gaps (random at sample positions, text between) | **+137.5%** | 0 |
| near-incompressible 2/5/10% | 0 | 0 |
| mixed-archive (compressed + text segments, unaligned) | **+0.239%** | 0 |

v1 (8 × 16 KB DEFLATE samples, store if none shrinks) missed text between
samples: 0.24% loss on the realistic mix, over the 0.1% limit. v2 adds a
byte-entropy guard: every 4 KB window must have collision entropy ≥ 7.8
bits/byte (integer histogram, sum of squares ≤ 75,281). A/B of v2: random
-74%, but adversarial-gaps +9.5% REGRESSION (8 doomed samples + scan + full
DEFLATE). v3 (kept) reorders the same checks: sample 0, entropy scan (stops at
the first text-like window), samples 1-7.

v3 A/B vs 7668978:

| bench | A HEAD | B | Δ | 95% CI | verdict |
|---|---|---|---|---|---|
| compress_incompressible/threads=default | 95.3 ms | 25.2 ms | -73.5% | [-74.0, -73.0] | improved |
| compress_incompressible/threads=1 | 1.02 s | 132 ms | -87.1% | [-87.2, -87.0] | improved |
| compress_with_options_incompressible/threads=default | 119 ms | 52.1 ms | -56.1% | [-56.7, -55.5] | improved |
| compress_mixed/adversarial-gaps | 59 ms | 58.6 ms | -0.7% | [-4.8, +2.9] | noise |
| compress_mixed/mixed-archive | 54.4 ms | 49.5 ms | -8.9% | [-9.8, -7.8] | improved |
| compress_mixed/near-incompressible-2pct | 52.3 ms | 54.3 ms | +3.7% | [+0.9, +6.9] | slower, under gate |
| compress_mixed/near-incompressible-5pct | 52.4 ms | 53.8 ms | +2.6% | [-0.7, +5.6] | noise |
| compress_mixed/near-incompressible-10pct | 50.3 ms | 50 ms | -0.4% | [-1.8, +1.1] | noise |
| compress_throughput/threads=1/block=1024KB | 1.35 s | 1.37 s | +1.5% | [+0.3, +2.6] | noise |
| compress_throughput/threads=default/block=1024KB | 113 ms | 116 ms | +2.5% | [-0.1, +4.8] | noise |

## Goal check: end to end (WSL2, 128 MB, all 24 CPUs, ABBA, median of 10)

| data | tool | MB/s | vs start | ratio |
|---|---|---|---|---|
| random | crush 844698c (start) | 571 | | 1.0000 |
| random | crush 4664ca2 | 1,067 | +86.8% | 1.0000 |
| random | pigz -6 | 692 | | 0.9997 |
| random | pigz -1 | 728 | | 0.9997 |
| real files | crush 844698c | 956 | | 3.5030 |
| real files | crush 4664ca2 | 992 | +3.8% | 3.5030 |
| real files | pigz -6 | 601 | | 3.4809 |
| real files | pigz -1 | 1,324 | | 3.0294 |

Goal (+15-20%, beat pigz) met: crush is 47-54% faster than pigz on random
data; real-file ratio unchanged. Run spread 15-34% (WSL), far below the gain.

## Gate

- Quality gate: fmt, clippy, tests (502), docs, cuda clippy pass. `cargo audit`
  / `cargo deny` fail on RUSTSEC-2026-0418 (libz-sys 1.1.30 via flate2), which
  is in develop's lockfile too; fix separately with `cargo update -p libz-sys`.
- Fuzz (WSL, nightly), 100k runs each, clean: fuzz_probe_roundtrip (new),
  fuzz_roundtrip, crush-core compress. The crush-parallel fuzz crate could not
  build before (missing `[workspace]`); fixed.
- Full suite ABBA vs develop (3feab3e): 11 REGRESSION rows at threads=4/8 and
  in decompression (identical code on both sides), GPU decompress -9 to -14%
  (untouched). Confirmation runs, same day:
  - A/A noise floors at threads=8: compress 5.6-7.1%, decompress 6.8%,
    decompress threads=default 12.6%.
  - ab vs 3feab3e, cpu-compress: threads=4 noise; threads=8 -10 to -14%
    (faster); all else within ±2.8% (noise / under gate).
  - ab vs 3feab3e, cpu-decompress: threads=8 -14.6% (faster), threads=1 +3.9%
    (under gate), rest noise.
  - ab vs 3feab3e, quick-core: NEUTRAL. roundtrip/102400 +3.3% [+1.6, +5.7]
    (full run: +6.6%), under its 3.8% floor and the 5% gate; consistent
    direction in both runs, no code cause found (compress() assembly does the
    same work for 100 KB). Flagged, not claimed as noise.
  Conclusion: the threads=4/8 swings are machine variance on the 8P+8E CPU
  (they reversed sign between sessions); no confirmed regression > 5%.
