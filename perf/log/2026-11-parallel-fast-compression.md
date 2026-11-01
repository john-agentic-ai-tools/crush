# Parallel fast compression (feature 027)

Spec: `specs/027-compression-perf-plan/spec.md`. Base 7e034ca, i9-12900K under WSL2, 128 MiB `real` in tmpfs, except where noted.

## Results

| Criterion | Target | Result | |
|---|---|---|---|
| SC-001 `fast-lz` in memory, all cores | >= 5,000 MB/s | 5,238 MB/s (three medians: 5,248 / 5,238 / 5,087) | **Pass** |
| SC-002 CLI beats live `lz4 -1`, 4 cores | > 1.0x | 1,572 vs 1,850 MB/s, 0.85x | **Miss** |
| SC-002 CLI beats live `lz4 -1`, all cores | > 1.0x | 1,759 vs 2,026 MB/s, 0.87x | **Miss** (accepted 2026-11-01) |
| SC-003 tile compression, all cores | >= 10x | About 28x, 47.7 to 1,330 MB/s (16 MiB) | **Pass** |
| SC-004 `fast-lz`, one core | no worse than -5% | 720 to 725 MB/s, medians of eight runs each | **Pass** |
| SC-004 tile compression, one core | no worse than -5% | About 3x, 48 to 141 MB/s | **Pass** |
| SC-005 output bytes | identical | `fast-lz` CLI SHA-256 equal at 1, 4 and all cores and to the old build; tile output hash equal | **Pass** |
| SC-006 other plugins | no worse than -5% | No regression in `/perf check` | **Pass** |

## Batch size

In memory, medians of three, MB/s. Workers persist between batches.

| Blocks per thread | All cores | 4 cores |
|---|---|---|
| 4 | 5,057 | 2,184 |
| 8 | 5,238 | 2,193 |
| 16 | 4,879 | 2,181 |

The first build rebuilt workers per batch and reached about 4,570 MB/s at 4 blocks per thread. Keeping them in a thread-local fixed that. Chosen: 8, within spread of the others at 4 cores. Compressed output held in memory is at most 8 blocks per thread.

## SC-002

`perf/compare.py`, ABBA, 15 reps, spreads 7-25%:

| | 4 cores | all cores |
|---|---|---|
| `crush-fast-lz` | 1,572 | 1,759 |
| `lz4 -1` | 1,850 | 2,026 |

The ratio also trails: 2.23 vs 2.295. At 1 core, Crush led 650 to 634 MB/s.

The harness governs gates, so this is a miss. A plain loop on the same machine disagreed at all cores, 11 runs, median wall time: `crush-fast-lz` 59.0 ms, `lz4 -1` 61.6 ms. The gap between the two methods is unexplained: the commands are the same.

Inside the process, all cores: about 52 ms total, of which 9.4 ms is the input map. The CLI already maps inputs of 1 MiB or more, with `populate()` (`crush-cli/src/commands/utils.rs:197`), which faults the pages in serially. Feature 023 chose that on one core, where a plain map moved the faults into the codec phase.

A plain map needs no new `unsafe`, so it was tried at every core count. It did not help, and `populate()` stays. `perf/compare.py`, ABBA, 15 reps, compress MB/s:

| | 1 core | 4 cores | all cores |
|---|---|---|---|
| `populate()` | 630 | 1,597 | 1,746 |
| plain map | 588 | 1,588 | 1,598 |
| `lz4 -1` | 630 | 1,836 | 2,078 |

The read phase falls from about 9.4 ms to 0.01 ms, but the compress phase grows by more than that at 1 and all cores. Medians of three in-process totals: 205 to 226 ms at 1 core, 84 to 78 ms at 4, 54 to 61 ms at all. The 4-core gain does not show in the harness. Output hashes match.

## `/perf check`

Windows, quick run against 7e034ca: **IMPROVED**, no REGRESSION rows in 34.

| Benchmark | Change in time | Throughput, MB/s |
|---|---|---|
| `fast_lz/compress/text-64MB` | -85.3% | 495 → 3,354 |
| `compress_throughput/cpu/mixed-10MB` | -94.9% | 114 → 2,237 |
| `compress_throughput/cpu/log-1MB` | -90.2% | 189 → 1,929 |

- Advisory over 5%: `fast_lz_select/estimate/1MB` +19.9% and `select_for_input/1MB` +15.0%. The source of that function did not change, and 16 MB and 128 MB are flat. Placement caused an earlier slowdown there, but this run did not test that.
- Rerun after the plugin description text changed, 22 rows: **IMPROVED**, no REGRESSION rows. The same two advisory rows read +18.5% and +14.2%, and `decompress_throughput/cpu/mixed-10MB` read +2.0%, under the gate.
- Under the gate: six decompress rows between +1.0% and +2.5%. Decode source did not change, so these are probably layout, but that was not tested.

## Fuzz

`fuzz_roundtrip` and `fuzz_decompress`, 100,000 runs each, clean, from an empty corpus. The build used 16 blocks per thread; only that constant differs from the final code.

`fuzz_roundtrip` runs take about 2.5x longer than before (19 vs 48 s per 5,000 runs). The target tiles inputs to four 64 KB blocks, so each run now goes through Rayon under the sanitizer. Default blocks are 1 MB.
