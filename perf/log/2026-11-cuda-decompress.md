# CUDA decompression through the CLI

2026-11-02, i9-12900K, RTX 3060 Ti, Windows, driver 617.42, CUDA toolkit 13.1. `crush decompress --gpu-backend cuda` on 128 MiB of real files.

**Goal:** at least +20%, 242 to 290 MB/s, with no ratio change. **Result:** met. 243.3 to 300.4 MB/s, +23.5%, in a direct ABBA of 10 reps, with interquartile ranges of 2.1% and 0.9%.

## Where the time went

From `-vv` timestamps, median of 7 runs, ms:

| Phase | Before | After |
|---|---|---|
| Setup | ~40 | ~32 |
| CUDA context | 78 | 76 |
| Kernel compile, LZ77 and GDeflate | 38 | 38 |
| Tile structs | 18 | 17 |
| GPU decode, 4 batches | 154 | 156 |
| Checksums and placing tiles | 41 | 29 |
| After decode | 102 | 5 |
| Last log line to exit | 81 | 82 |
| **Total** | **557** | **438** |

The GPU decode took 28% of the run before, so the gains were all on the CPU side.

## Experiments

| Change | Build compared | Result | Verdict |
|---|---|---|---|
| H1: decode straight into the mapped output | baseline | 238.8 to 282.5 MB/s, +18.3% | Kept |
| H2: check and place tiles in parallel | H1 | 277.1 to 301.8 MB/s, +8.9% | Kept |

H1 predicted the 102 ms and some of the exit time would shrink. The 102 ms fell to 5 ms. Exit did not move, so that part of the prediction was wrong.

H2 predicted 41 ms would fall under 10 ms. It reached 29 ms. Tiles are now written to a fresh mapping, so page faults are the likely cost, but that was not measured.

H3 (not compiling the unused LZ77 kernel, about 28 ms) and H4 were not tried, because the plan stopped at the target. H3 is the obvious next gain.

H1 also checks every size against the tile index before allocating. Before, a crafted header could ask for an unchecked allocation.

## Other results

`wgpu` on `real`: 163.5 to 189.4 MB/s, +15.8%, in the same A/B.

`log` in the same A/B read +80.8%, but its baseline interquartile range was 130%, so it is not quoted.

In the full `perf/compare_gpu.py` comparison, 7 runs:

| Data | Parallel | wgpu | CUDA |
|---|---|---|---|
| `real` | 591, 1,164 | 527, 174 | 531, 305 |
| `log` | 673, 1,217 | 561, 194 | 585, 328 |
| `repetitive` | 1,265, 1,536 | 817, 225 | 803, 393 |
| `text` | 414, 902 | 279, 297 | 285, 473 |
| `random` | 549, 958 | 284, 307 | 286, 478 |

MB/s, compress then decompress. `text` and `random` do no GPU decoding. The first full run gave `log` CUDA compress 67 MB/s with an interquartile range of 805%, with no code change, so `log` was measured again on its own and replaced. The stall was not identified.

## Gates

- Full `ab` against 98b4a9a: IMPROVED, no regression. `decompress_throughput/gpu/mixed-10MB` went from 382 to 461 MB/s. Largest slowdowns on ms-scale benchmarks: +1.6%, on `decompress_throughput/cpu/mixed-10MB`, under the gate.
- Fuzz: `fuzz_decompress` and `fuzz_format`, 100,000 runs each, clean, from empty corpora.

## Profile after the changes

samply, 12 runs on Windows, against the same profile before. These are sampled call stacks and kernel time, not hardware counters.

| Inclusive share | Before | After |
|---|---|---|
| `NtFreeVirtualMemory` | 23.3% | 3.8% |
| `decompress_tiles_gdeflate` | 29.2% | 29.6% |
| `discover_gpu` | 24.0% | 23.4% |

The frees were the large output buffer and its tiles, as H1 supposed. The attribution rests on that drop, because kernel frames were not tied to callers.

The 82 ms from the last log line to exit is not CPU time in this process: `ExitProcess` is 1.2% of samples. It is therefore waiting of some kind. What it waits for was not established, and a sampling profile cannot show it.

The parallel placement is about 20% of samples. Page faults on the fresh mapping remain unmeasured.

## Next

The LZ77 kernel is unused: only tests reach `decompress_tiles`, and only format version 2 is accepted. Removing it should save about 28 ms on each CUDA run, not yet measured.
