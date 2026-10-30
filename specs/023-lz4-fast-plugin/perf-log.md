# Perf log: fast-lz (feature 023)

Targets (spec SC-001..SC-003), one core, in memory, 128 MB real-file corpus
(first 128 MB of `tar -cf - /usr/lib /usr/share`), same session as
`lz4 -b1` (lz4 1.10.0): compress ≥ 1.15 × lz4, decompress ≥ 0.60 × lz4,
ratio ≥ 2.18. Measured with `perf/lz-vs-lz4.sh` (WSL2 Ubuntu 20.04,
i9-12900K, best of 5).

## v0 — first working version (T022), 2026-10-29

Greedy LZ77, 2^14-entry hash table of 4-byte hashes reused across blocks,
8-byte match extension, LZ4-style skip acceleration (`1 + misses >> 6`),
backward match extension; decoder with `extend_from_slice` literals and
`extend_from_within` matches.

| codec | compress MB/s | decompress MB/s | ratio |
|---|---|---|---|
| lz4 -1 | 789 | 4,433 | 2.296 |
| fast-lz v0 | 467 | 1,016 | 2.335 |
| fast-lz / lz4 | 0.59× | 0.23× | 1.017 |

- SC-001 compress: MISS (needs 908 MB/s)
- SC-002 decompress: MISS (needs 2,660 MB/s)
- SC-003 ratio: PASS (2.335, better than lz4)

Fuzzing (WSL, nightly): `fuzz_roundtrip` 100k runs clean (358 s);
`fuzz_decompress` 1M runs clean (205 coverage edges; the target prepends a
valid frame header to even-tagged inputs so it reaches block and sequence
decoding — the first version stopped at the magic check, 100k runs in < 1 s).

Next: T023 `/perf goal` tuning.

## Experiment 1 — decoder copy paths (T023), kept

Floor for any decoder that returns a fresh buffer: allocating and filling a
128 MB `Vec` runs at 11.6 GB/s (about 12 ms, first-touch page faults
included; `lz4 -b` reuses its buffer). Sequence mix on the corpus (12.4 M
sequences): 1.5 literals and 9.3 match bytes on average, 61% with no
literals, 87% with < 15 literals, a match of <= 18 bytes and offset >= 16,
6.2% with offset < 16.

Change: each block is decoded into a pre-sized slice with 16 bytes of slack;
literals of <= 16 bytes and matches are copied in fixed 16-byte steps
(8-byte steps for offsets 8..15, pattern repeat for shorter periods); an
out-of-line loop decodes the common sequence shape with one combined check;
the output is reserved from the expected length, capped at 256 x the payload.
Tests first: `tests/decoder_copies.rs` (every offset 1..40 x lengths 4..1000,
back-to-back matches, final literal runs, a match ending at the block end).

Iterations, WSL same session as lz4 (decompress MB/s): v0 1,016; pre-sized
output + 16-byte copies 1,798; + shortcut 2,013; shortcut out of line 2,099
(kept); shortcut widened to long matches / offsets 8..15 with copy loops
1,780 (reverted); unconditional second match step 2,056 (no gain, reverted).

| codec (WSL, best of 7) | compress MB/s | decompress MB/s | ratio |
|---|---|---|---|
| lz4 -1 | 805 | 4,522 | 2.296 |
| fast-lz exp1 | 473 | 2,099 | 2.335 |
| fast-lz / lz4 | 0.59x | 0.46x | 1.017 |

`bench.py ab --ref HEAD --suite fast-lz` (Windows, goal-fastlz-exp1b; an
earlier run's +6.1% on compress/file did not reproduce and the compressor
is unchanged): verdict IMPROVED.

| benchmark | HEAD | exp1 | change |
|---|---|---|---|
| decompress/file | 710 MB/s | 1,761 MB/s | -59.7% time |
| decompress/text-64MB | 488 MB/s | 1,733 MB/s | -71.8% time |
| decompress/random-64MB | 2,628 MB/s | 4,678 MB/s | -43.8% time |
| compress/* | | | noise |

Noise floors (A/A, goal-fastlz-noise2): compress 4.2-5.9%, decompress
1.9-3.3%.

SC-002 still MISS (0.46x of 0.60x). The remaining gap is the decode loop
(about 49 ms vs about 37 ms needed); no sampling profiler is available on
this machine (samply not installed, WPR needs elevation, no perf in WSL).

## Experiment 2 — compressor output path (T023), kept

Change: sequences are written into a pre-sized slice (block size + 16 bytes
of slack) instead of `Vec::push`/`extend_from_slice`; literal runs of
<= 16 bytes are copied in one fixed 16-byte step; a block gives up as soon
as its output would reach the block size (it is stored then, as before).
The match search is unchanged and the output is byte-identical to v0 on the
128 MB corpus (`cmp` of `CODEC_BENCH_DUMP` dumps). Tests first:
`blocks_near_the_stored_threshold` (compressed blocks are always smaller
than their data).

| codec (WSL, best of 7) | compress MB/s | decompress MB/s | ratio |
|---|---|---|---|
| lz4 -1 | 797 | 4,530 | 2.296 |
| fast-lz exp2 | 508 | 2,166 | 2.335 |
| fast-lz / lz4 | 0.64x | 0.48x | 1.017 |

`bench.py ab --ref HEAD --suite fast-lz` (goal-fastlz-exp2): IMPROVED.
compress/file 480 -> 506 MB/s (-5.1% time, CI [-6.4%, -4.0%], floor 4.2%);
the other five benchmarks are noise.

Emission was not the main cost; the match search is. Its miss path is about
20 instructions per probed position. Next: lz4's default table is 4,096
entries (16 KB, fits L1); ours is 16,384 (64 KB).

## Experiment 3 — hash table 2^14 -> 2^12 entries (T023), kept

lz4's default table is 4,096 entries (16 KB, fits the 48 KB L1); ours was
16,384 (64 KB). Screening, WSL same session as lz4 (compress MB/s, ratio):

| table | compress | vs lz4 | ratio |
|---|---|---|---|
| 2^14 (before) | 508 | 0.64x | 2.335 |
| 2^13 | 538 | 0.70x | 2.308 |
| 2^12 (kept) | 588 | 0.75x | 2.256 |
| 2^11 | 598 | 0.76x | 2.173 (below 2.18) |
| 2^10 | 594 | 0.75x | 2.066 |

`bench.py ab --ref HEAD --suite fast-lz` (goal-fastlz-exp3): IMPROVED.

| benchmark | HEAD | exp3 | time | ratio |
|---|---|---|---|---|
| compress/file | 504 MB/s | 609 MB/s | -17.1% | 2.335 -> 2.256 |
| compress/text-64MB | 507 MB/s | 487 MB/s | +4.2% (CI +3.0..+5.4, floor 5.9%: noise) | 1.847 -> 1.792 |
| compress/random-64MB | | | +1.3% (noise) | 1.000 |
| decompress/* | | | noise | |

Watch: the text corpus compresses slightly slower with the smaller table
(CI entirely above zero, under its noise floor), and most of the ratio
margin is used (2.256 vs the 2.18 floor). The ratio alone cannot buy the
remaining compress gap (0.75x of 1.15x).

## Experiment 4 — lz4-style search loop + 6-byte hash (T023), not kept

Change: lz4's search structure (the next position's hash computed before
the current candidate is checked; the position right after a match tried
at once) plus a table hash over 6 bytes instead of 4 (candidates still
confirmed on 4 bytes). Hash width screened on the 2^12 table, WSL same
session as lz4:

| hash | compress MB/s | vs lz4 | decompress MB/s | ratio |
|---|---|---|---|---|
| 4 bytes | 607 | 0.77x | 2,173 | 2.256 |
| 5 bytes | 626 | 0.79x | 2,360 | 2.291 |
| 6 bytes | 694 | 0.87x | 2,550 | 2.231 |
| 7 bytes | 697 | 0.86x | 2,569 | 2.147 (below 2.18) |
| 8 bytes | 740 | 0.94x | 2,535 | 2.027 (below 2.18) |
| 6 bytes, 2^13 table | 613 | 0.77x | 2,574 | 2.318 |

`bench.py ab --ref HEAD --suite fast-lz` (goal-fastlz-exp4): REGRESSION.
compress/file -5.5%, decompress/file -11.0%, decompress/text-64MB -26.0%,
but compress/text-64MB +7.1% (CI +6.1..+8.1%, over the 5% gate).

Split (WSL, codec_bench on the file and on the synthetic text corpus,
compress MB/s): HEAD 607 / 484; search loop only 587 / 492; loop + 6-byte
hash 671 / 473. The loop restructuring does not help in safe Rust; the
gain is the hash. Decision (user): find the cause with the last experiment.

## Experiment 5 — 6-byte hash on the original loop (T023), kept

| codec (WSL, best of 7) | compress MB/s | decompress MB/s | ratio |
|---|---|---|---|
| lz4 -1 | 784 | 4,429 | 2.296 |
| fast-lz exp5 | 721 | 2,519 | 2.230 |
| fast-lz / lz4 | 0.92x | 0.57x | 0.971 |

`bench.py ab --ref HEAD --suite fast-lz` (goal-fastlz-exp5): IMPROVED.

| benchmark | HEAD | exp5 | time | ratio |
|---|---|---|---|---|
| compress/file | 590 MB/s | 670 MB/s | -12.0% | 2.256 -> 2.230 |
| compress/text-64MB | 475 MB/s | 490 MB/s | -3.1% (noise) | 1.792 -> 1.897 |
| decompress/file | 1,701 MB/s | 1,962 MB/s | -13.3% | |
| decompress/text-64MB | 1,631 MB/s | 2,198 MB/s | -25.8% | |
| decompress/random-64MB | 4,577 MB/s | 4,455 MB/s | +2.7% (under gate; stored blocks) | 1.000 |
| compress/random-64MB | | | noise | 1.000 |

## Outcome of T023 (5 experiments, cap reached)

| | v0 | final (exp5) | target | status |
|---|---|---|---|---|
| compress vs lz4 | 0.59x (467 MB/s) | 0.92x (721 MB/s) | >= 1.15x | SC-001 MISS |
| decompress vs lz4 | 0.23x (1,016 MB/s) | 0.57x (2,519 MB/s) | >= 0.60x | SC-002 MISS (close) |
| ratio | 2.335 | 2.230 | >= 2.18 | SC-003 PASS |

Frontier: within the ratio floor the best measured compress speed is 0.92x
lz4 (6-byte hash, 2^12 table); hashing 8 bytes reaches 0.94x at ratio 2.03.
A fresh 128 MB output buffer costs about 12 ms (lz4's benchmark reuses its
buffer), which alone caps decompression near 0.7x of lz4 even with an
lz4-speed decode loop. Going further would need unchecked copies/loads
(`unsafe`) or a different algorithm. Decision (user, 2026-10-29): record the
frontier, keep the best variant, continue with US2; targets unchanged.

## US2 — selection estimate (T033, T034), 2026-10-29

Sampled estimate: evenly spaced samples, about 1/128 of the input, between
4 x 4 KiB and 16 x 16 KiB, compressed with the block compressor and a
per-thread table. First planned at 1/64 (research R6): that measured 2.02%
of compression time on 16 MB of source text (samples cost more per byte
than whole blocks), so the budget was halved. Samples lack the history of a
1 MiB block, so the estimate runs low (text corpus: 20% low with 4 KiB
samples at 1 MB, 9% low with 16 KiB samples): conservative for selection.

Selection report (`cargo test --release -p crush-lz --test selection_report
-- --ignored --nocapture`, Windows, CRUSH_BENCH_CORPUS = the 128 MB tar),
`--level fast` weights:

| input | 1 MB est / chosen / cost | 16 MB | 128 MB |
|---|---|---|---|
| source text | 1.518 / fast-lz / 1.48% | 1.717 / fast-lz / 0.95% | 1.725 / fast-lz / 0.24% |
| log text | 3.143 / fast-lz / 0.80% | 3.315 / fast-lz / 0.88% | 3.307 / fast-lz / 0.22% |
| real file | 1.930 / fast-lz / 0.75% | 2.050 / fast-lz / 0.91% | 2.084 / fast-lz / 0.24% |
| base64 text | 1.000 / deflate / 1.07% | 1.000 / deflate / 0.34% | 1.000 / deflate / 0.11% |
| random | 1.000 / deflate / 1.13% | 1.000 / deflate / 0.35% | 1.000 / deflate / 0.11% |
| deflate output | 1.000 / deflate / 1.06% | 1.000 / deflate / 0.33% | 1.000 / deflate / 0.10% |

SC-004: 18/18 routed as required. SC-005: highest cost 1.48% (limit 2%).
Criterion benches for the estimate and `select_for_input` (1/16/128 MB):
`fast_lz_select` group in `crush-lz/benches/codec.rs`.
