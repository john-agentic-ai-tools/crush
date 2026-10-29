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
