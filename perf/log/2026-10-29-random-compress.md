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
