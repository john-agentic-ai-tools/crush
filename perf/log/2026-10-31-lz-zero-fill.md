# Remove the fast-lz output zero-fill (SC-002)

Branch `025-perf-lz-zero-fill`, base afc4bad. Follows
`2026-10-31-diagnose-decompress-and-decoder.md`. Machine:
`windows-12th-gen-intel-core-i9-12900k-16c24t`, power profile Ultimate
Performance. A/Bs are Windows criterion ABBA runs; the SC-002 line is
`perf/lz-vs-lz4.sh 7` in WSL.

## Goal record

```
metric:      throughput
operation:   decompress
engine:      crush-lz (fast-lz), one core, in memory
workload:    128 MB real-file corpus; fast_lz/decompress/file, plus
             fast_lz/decompress_plugin/* and fast_lz/decompress_to/*
threads:     1
target:      SC-002, decompress >= 0.60x live lz4
constraints: safe Rust only, no ratio change, allowed dependencies only
```

**0.60x governs, not 2,650 MB/s.** The spec's 2,650 assumed lz4 at 4,417 MB/s.
`perf/lz-vs-lz4.sh` divides by live lz4, and today's best run would meet 2,650
at about 0.57x. The spec file is unchanged.

Scope decisions: product paths are in scope, because no product called the one
path the old benchmark covered. Compress is out of scope: all its zero-fill is
3.5% of the benchmark, under its 5.8% floor, so nothing there could be verified.
SC-001 stays unmet. No reusable-output API. `None`-path sizing is capped at 16x
payload, since a 256x cap would let a forged 1 GiB file request 256 GiB.

## Result

| criterion | before (afc4bad) | after (74f5269) | |
|---|---|---|---|
| SC-002 decompress / lz4 | 2,582 / 4,406 = 0.59x MISS | 2,872 / 4,597 = 0.62x | PASS |
| SC-003 ratio | 2.230 | 2.230 | PASS |
| SC-001 compress / lz4 | not the goal | 743 / 816 = 0.91x | MISS, as before |

Live lz4 ranged 4,375-4,597 MB/s across these runs, so the ratio is the figure
to read. An intermediate run at 8f0dde7 gave 0.61x. The script exits 1 because
SC-001 misses.

Cumulative, 2220227 vs 74f5269 (`goal-lz-zero-fill-cumulative`). 2220227 only
adds benchmarks to afc4bad, so this run covers the plugin and streaming rows
that afc4bad lacks:

| benchmark | A | B | Δ time | 95% CI | throughput | verdict |
|---|---|---|---|---|---|---|
| fast_lz/decompress/file | 64.5 ms | 56.6 ms | -12.2% | [-12.8, -11.7] | 2,082 → 2,372 MB/s | improved |
| fast_lz/decompress/text-64MB | 28.9 ms | 25 ms | -13.6% | [-14.3, -12.9] | 2,322 → 2,687 MB/s | improved |
| fast_lz/decompress/random-64MB | 14.1 ms | 14.1 ms | +0.1% | [-0.5, +0.8] | 4,754 → 4,748 MB/s | noise |
| fast_lz/decompress_plugin/file | 87.5 ms | 56.3 ms | -35.7% | [-36.1, -35.2] | 1,534 → 2,384 MB/s | improved |
| fast_lz/decompress_plugin/text-64MB | 39.9 ms | 24.7 ms | -38.0% | [-38.6, -37.4] | 1,681 → 2,713 MB/s | improved |
| fast_lz/decompress_plugin/random-64MB | 25.6 ms | 14.1 ms | -45.1% | [-45.6, -44.7] | 2,622 → 4,775 MB/s | improved |
| fast_lz/decompress_to/file | 40.6 ms | 38.7 ms | -4.7% | [-5.3, -4.1] | 3,306 → 3,470 MB/s | improved |
| fast_lz/decompress_to/text-64MB | 17.1 ms | 16 ms | -6.2% | [-6.9, -5.4] | 3,930 → 4,189 MB/s | improved |
| fast_lz/compress/file | 198 ms | 195 ms | -1.7% | [-2.4, -1.1] | 677 → 689 MB/s | noise |

`fast_lz/decompress_to/random-64MB` takes about 630 ns because stored blocks go
straight to `io::sink()`. It is not a decode measurement.

### CLI, end to end

`perf/compare.py --cores 1 --tools crush-fast-lz,lz4-1 --data real --abba
--reps 6` in WSL on tmpfs, pinned to one CPU, same 128 MiB corpus. Three rounds
per build in the order before, after, after, before, before, after. Each row is
one round's median, crush / lz4:

| round | before compress | after compress | before decompress | after decompress |
|---|---|---|---|---|
| 1 | 1.05x | 1.04x | 1.17x | 1.30x |
| 2 | 1.10x | 1.08x | 1.18x | 1.23x |
| 3 | 1.11x | 1.05x | 1.16x | 1.19x |
| mean | 1.09x | 1.06x | 1.17x | 1.24x |

Median of rounds for crush: decompress 1,548 → 1,807 MB/s, compress 611 → 601
MB/s, ratio 2.23 against lz4's 2.295. lz4 itself moved between 1,307 and 1,564
MB/s on decompress, so read the ratios. The decompress ranges do not overlap;
the compress ranges do, and the compressor did not change. This is not a
criterion ABBA and has no confidence interval. Crush was already ahead of lz4
before this branch, against 0.90x and 0.83x on 2026-10-29. That earlier gain
comes from other work and was not traced here.

## Experiments

### H1: one allocation and a write cursor in `decode_frame`

Prediction: memset samples vanish, about 5 ms of page faults moves into the
decoder, total falls 9-11% on Linux and 4-9% on Windows.

- exp1: decompress/file -10.6%, text -12.9%, but `decompress_plugin/random`
  **+7.4% REGRESSION**: stored blocks were zero-filled through `resize` before
  being overwritten. Fixed by appending stored blocks when the buffer is short.
- exp1b: decompress/file -11.7%, text -14.7%, plugin/random -3.7%, but
  `decompress_to/file` **+6.6% REGRESSION**, on a path whose logic did not
  change (+1.5% in exp1). Cause unproven; inlining or layout is suspected. H1
  was committed together with H2 rather than alone because of this.

### H2: keep `decode_frame_to` scratch at high-water length

Prediction: memset vanishes from `decompress_to`, gain 8-13%.

exp2 (H1+H2 vs 2220227): decompress/file -15.5% (1,926 → 2,280 MB/s), text
-15.4%, decompress_to/file -3.1%, decompress_to/text -6.6%, plugin/file +3.1%
(under the gate), plugin/random -3.6%. Verdict IMPROVED. Kept as 8f0dde7.

**`decompress_to/file` missed H2's 5% keep criterion and its 8-13% prediction.**
The cumulative run measures it at -4.7%. It was kept because the change also
clears the +6.6% above and the text corpus does clear 5%.

### H3: size the `None` path from block headers

Prediction: memset and reallocation samples vanish from `decompress_plugin/file`,
gain at least H1's.

exp3 (H3 vs 8f0dde7): plugin/file -35.8%, plugin/random -44.4%, plugin/text
-37.3%, far above the prediction. decompress/random was flagged at +5.4%
[+2.8, +8.8]; focused exp3b gave +1.0% [-1.2, +3.2] against a 2.2% floor, so it
did not reproduce. decompress/text +2.0% (under the gate). Kept as 74f5269.

The reserve hypothesis (long-literal wild loop) was not needed.

## Prediction check: H1 was right about memset, wrong about how much it would pay

WSL `profile-diff`, 2220227 vs 8f0dde7, decompress/file:

- memset fell from 24.6% to 0.0%, as predicted.
- About 76% of that time reappeared in the decoder. Page faults cost about
  9.5 ms, not the 5 ms estimated, and zeroing itself was about 4 ms.
- sys time per iteration stayed flat, about 9.5 vs 9.8 ms.
- Criterion in WSL improved about 6.1-6.6%, and lz-vs-lz4 3.7%. The 9-11% Linux
  prediction missed.

So most of the old memset cost on Linux was first-touch faults, which any fresh
output buffer pays. This fits the larger gain on Windows, but why the fault cost
is so high is untested: transparent huge pages (`[always]`) are a hypothesis
only. Removing those faults would need a reusable output buffer, which is out of
scope.

## Gate

Full suite, afc4bad vs 74f5269 (`goal-lz-zero-fill-final`): the verdict was
**REGRESSION on 7 rows**. All 7 cleared on focused reruns, and the first pass
ran slow overall: the same code measured 209 ms there and 195 ms in the
cumulative run on `fast_lz/compress/file`.

| flagged row | full suite | focused rerun | verdict |
|---|---|---|---|
| decompress_throughput/gpu/binary-1MB | +7.8% | -0.1% [-0.4, +0.2] | noise |
| decompress_throughput/gpu/log-1MB | +8.3% | +0.1% [-0.4, +0.6] | noise |
| decompress_throughput/gpu/mixed-10MB | +13.8% | +0.4% [-0.6, +1.4] | noise |
| decompress_throughput/gpu/mixed-1MB | +6.4% | +0.7% [+0.4, +1.0] | slower (<gate) |
| fast_lz/compress/file | +6.8% | +3.9% [+2.8, +4.8] | noise |
| fast_lz_select/estimate/128MB | +5.3% | +3.4% [+3.0, +3.8] | slower (<gate) |
| fast_lz_select/select_for_input/128MB | +5.2% | +2.4% [+2.0, +2.7] | slower (<gate) |

- The branch changes only `crush-lz`, and `crush-gpu` does not depend on it.
- `fast_lz/compress/file` read -1.7% against 2220227, which has the same
  compressor and differs from afc4bad only in benchmarks, so +3.9% against
  afc4bad is build-to-build variation rather than a compress change. Its "noise"
  label relies on an older floor that was not re-measured here.
- **`fast_lz_select` is consistently slower and that is not explained.**
  estimate/1MB read +16.1% then +8.8%, and select_for_input/1MB +12.7% then
  +9.2%, against A/A floors of 1.8% and 0.6%. Those are advisory microbenchmarks.
  The 16 MB and 128 MB rows read +2.0 to +3.8%, under the gate. Estimation source
  did not change, so layout is suspected, but it has not been profiled.
- compress_throughput and decompress_throughput `cpu` rows were slower by up to
  4.0% in the same slow pass, under the gate, and were not rerun.

The six `decompress_plugin` and `decompress_to` rows do not exist at afc4bad, so
the full-suite run skipped them. The cumulative table covers them.

**Baseline caveat:** `perf/baselines/<fingerprint>.json` was saved from B1 of the
slow full-suite pass. It reads `fast_lz/compress/file` at 213.6 ms against 198 ms
in the focused rerun, so its numbers run high. Baselines are context only, and no
decision here used them.

## Verification

- `quality-gate` agent at 74f5269: PASS (fmt, clippy, 581 tests passed, 6
  ignored, doc, audit, deny, cuda clippy).
- Fuzz in WSL at 74f5269: `fuzz_decompress` and `fuzz_roundtrip`, 100k runs each,
  clean. `fuzz_decompress` now also compares `decompress` with `decompress_to`.
- Mutation check on the exact-slice rule: an oversized-slice mutant survives
  `block_overrun_is_rejected_with_room_after_it` alone, for a reason not
  established. The `debug_assert_eq!(dst.len(), ulen + WILD)` in `decode_into`
  kills it (2 tests panic). That test therefore does not prove the bound by
  itself, and the assertion is what guards it, in debug builds only.

## Not proven

- Why `decompress_to/file` went +6.6% in exp1b.
- Why page faults cost about 9.5 ms (THP is untested).
- The size of the CLI gain beyond the three rounds above.
- Why `fast_lz_select` is slower.
