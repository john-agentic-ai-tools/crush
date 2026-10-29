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
