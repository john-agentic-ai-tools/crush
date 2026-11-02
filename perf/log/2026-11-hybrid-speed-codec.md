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

2026-11-01, i9-12900K under WSL2, zstd 1.5.7, 128 MiB `real` in tmpfs (the file from the 2026-10-29 comparison). Sizes are exact. Each chunk is compressed alone with `zstd -3` and the sizes summed.

| Block | `-T1` (the gate's flag) | Against 3.700 | `--single-thread` | `-T1`, no literal compression |
|---|---|---|---|---|
| 1 MiB | 3.586 | -3.1% | 3.570 | 3.299 |
| 2 MiB | 3.644 | -1.5% | 3.626 | 3.365 |
| 4 MiB | 3.675 | -0.7% | 3.648 | 3.403 |
| 8 MiB | 3.692 | -0.2% | 3.661 | 3.422 |
| 16 MiB | 3.697 | -0.1% | 3.671 | 3.428 |
| Whole file | 3.700 | | 3.675 | 3.432 |

`--single-thread`, the flag in the original method, gives 3.675 on the whole file. `perf/compare.py` runs `-T<n>`, which gives 3.700 at every thread count, so `-T1` is the like-for-like figure. Why the two modes differ was not investigated.

8 MiB is not a free fallback. SC-008 allows 5 ms for one block, and at the 1.6 GB/s measured below an 8 MiB block takes about 5 ms before any read, while 4 MiB takes about 2.5 ms. It also halves the tasks available to parallel decode.

Sprint can spend more on matching instead. 4 MiB blocks, one pinned core, `zstd -b3 -B4194304`, median of three rounds, in memory:

| Window, 8-byte table, 5-byte table | Ratio | Compress | Decompress |
|---|---|---|---|
| 2^22, 2^17, 2^16 | 3.678 | 379 MB/s | 1,639 MB/s |
| 2^22, 2^17, 2^17 | 3.691 | 359 | 1,649 |
| 2^22, 2^18, 2^16 | 3.699 | 347 | 1,646 |
| 2^22, 2^18, 2^17 | 3.709 | 326 | 1,644 |

A whole-block window alone adds 0.08%. The 8-byte table is the cheap lever: 2^18 entries reach 3.6994 against the live 3.6996, for 8.5% less compression speed, and decompression does not change. Both tables at the larger size clear the gate by 0.3% and cost 14%.

**Decision**: stay at 4 MiB. Start the sprint matcher at 2^18 8-byte entries and 2^16 5-byte entries, and let the prototype decide on 2^17 5-byte entries. These are zstd's numbers: sprint's entropy stage and matcher will land elsewhere.

## Literal compression

One pinned core, whole file, `--single-thread`, ABBA, 22 runs each, median MB/s with spread:

| | Ratio | Compress | Decompress |
|---|---|---|---|
| `zstd -3` | 3.675 | 317.5 (8%) | 1,283.5 (9%) |
| `zstd -3 --no-compress-literals` | 3.422 | 327.9 (10%) | 1,437.6 (16%) |

Huffman literals are worth 7.4% of ratio here (7.8% with `-T1`, 3.700 against 3.432). Both speed differences are inside the spreads: about 3% on compress and 11% on decompress. SC-001 cannot be reached without them, so they stay.

## Prototype

2026-11-01, i9-12900K under WSL2, one pinned core, in memory, 128 MiB `real`, 4 MiB blocks, 128 KiB segments, safe Rust. `crush-sprint/examples/codec_bench.rs`, medians of three to five runs per round, over several rounds. Tables: 2^18 8-byte, 2^16 5-byte.

| Stage | Sprint | zstd 1.5.7 | Multiple |
|---|---|---|---|
| Huffman decode, MB/s of literals | 1,302 | 1,460 | **0.89x** |
| Compress so far (match + Huffman), MB/s of input | 284 | 344 | **0.83x** |
| Match stage alone, MB/s of input | 303 | | |
| Huffman encode, MB/s of input | 4,600 | | |

Sprint figures are from the final build, whose three decode medians ranged 1,266-1,327. The zstd decode figure comes from an earlier round in the same session, and the 1,331 measured then, before the lint changes to `common_length` and the decode loop, may be the same code on a quieter machine: that was not tested. How to read the comparison:

- The decode reference is `zstd -b3 -B131072` on the literal bytes sprint produced. Nearly nothing in them matches (ratio 1.165), so that run is mostly zstd's Huffman decoder, with some framing cost included.
- The compress reference is `zstd -b3 -B4194304 --zstd=wlog=22,hlog=18`, the whole of zstd at the table sizes that reach 3.70. Sprint's figure has **no sequence coding yet**, so the 0.83x will fall when T044 and T048 land.

Estimated ratio: 3.62 at 2^16 5-byte entries, 3.63 at 2^17, for 8% less match speed. The target is 3.70. This is an estimate, not a measurement: sequences have no coder, so their size is the per-segment order-0 entropy of the codes, using coarser match-length classes than the format's. It sets 22.4 MB of sequences beside about 21.5 MB implied for zstd, and at most that gap is real. Actual ratio waits for T044 to T050.

The first run of this bench showed changing the 5-byte table had no effect. The hash constant had 24 trailing zero bits, so the short hash saw only 2 bytes. The fix took the estimate from 3.56 to 3.62, and a test now pins which bytes the hash covers.

### Bounds checks (T042)

Counted in the optimized assembly, panicking checks only:

| Function | Before | After |
|---|---|---|
| `HuffmanDecoder::decode_interleaved` | 0 | 0 |
| `Matcher::find`, with `common_length` | 6, plus 4 in `common_length` | 0 |

`find` traded index expressions for `get` and reverse iterators. Old against new, four alternating rounds: 298-303 MB/s and 276-302 MB/s. Output is identical. The one low reading aside there is no measurable change. Refills and reads still compare a length before each load. These branches do not panic and were not removed.

**T043 go/no-go**: both stages are at or above 0.8x, so sprint continues without an `unsafe` exception. T046 is not triggered.

## First full codec

2026-11-01, same machine and corpus. All of T044 to T056 in place, safe Rust, no `unsafe` exception.

Through the CLI against `zstd -3 -T<n>`, ABBA, 7 reps, 128 MiB in tmpfs, median MB/s. Ratio 3.631 against 3.700, -1.9%:

| | 1 core | 4 cores | all cores |
|---|---|---|---|
| Sprint compress | 221 | 718 | 689 |
| `zstd -3` compress | 202 | 1,059 | 1,633 |
| **Multiple**, gate | **1.09x**, 1.0x | **0.68x**, 1.0x | **0.42x**, 1.5x |
| Sprint decompress | 729 | 2,173 | 3,196 |
| `zstd -3` decompress | 1,039 | 1,196 | 1,205 |
| **Multiple**, gate | **0.70x**, 1.0x | **1.82x**, 2.0x | **2.65x**, 3.0x |

`zstd -3 --single-thread` compressed at 317 MB/s earlier today, against 202 for `-T1`, so the 1-core pass rests on the flag the gate uses. `-T1` also spends a thread on I/O, pinned to the same CPU.

Compression stops scaling after 4 cores: the cause is below, under "All-core compression".

In memory, one core, 4 MiB blocks, medians of three rounds. zstd is `-b3 -B4194304 --zstd=wlog=22,hlog=18`, the tables that reach 3.70:

| | Sprint | zstd | Multiple | SC-004 |
|---|---|---|---|---|
| Compress | 241 MB/s | 354 | 0.68x | 0.85x, **miss** |
| Decompress | 1,031 MB/s | 1,646 | 0.63x | 0.85x, **miss** |

Decode time before the repeat-offset shift, 128 MiB: sequences 102 ms (11.3 ns each, 9.04 M sequences), literals 15 ms, tables 4 ms.

### Decoder changes

Medians over three rounds, MB/s:

| Change | Decompress | Verdict |
|---|---|---|
| First build | 991 | |
| 16-byte array copies, 1 flush fewer | 1,020 | Kept |
| Branchless repeat offsets, word-at-a-time short periods | 1,076 | Kept |
| Codes resolved into table entries, one shifted word per sequence | 1,084 | Kept, no measurable gain |
| Ring of 8 sequences, touching each source early | 1,030 | **Reverted**: slower |
| Match copies through `split_at_mut` | 1,115 | Kept |
| Branching offset pick | 1,010 | **Reverted**: -10%, so mispredicts matter here |
| Repeat-offset shift with no literals | 1,031 | Kept for ratio, -7% |

Simulated branch misses in `run` were about 0.2 per sequence, and the reverted branching pick shows that figure undercounts. `run` executes about 200 instructions per sequence. The cause of the remaining gap to zstd is not established.

### Ratio

| Change | Ratio |
|---|---|
| First build | 3.6173 |
| Repeat-offset shift with no literals (kept) | 3.6307 |
| 2^17 5-byte table | 3.6409, 8% less compress speed |
| 64 KiB segments | 3.6493 |
| 32 KiB segments | 3.6521 |
| 64 KiB segments and 2^17 table | 3.6587 |

Smaller segments are not a free win (6% less compress and 8% less decompress speed at 64 KiB), but the gain suggests table boundaries should follow the content. zstd 1.5.7 may do this with its block pre-splitter at this level; that was not verified. Table descriptions are a second lead: about 240 KB in all, unmeasured against zstd's. Huffman tables use 4 bits per symbol with no entropy coding.

### All-core compression

The 2^18 8-byte table chosen in T028 for ratio is what stops compression scaling. Throughput falls as threads are added. Medians of three, MB/s:

| | 1 P | 8 P | 16 P threads | 8 E | all 24 |
|---|---|---|---|---|---|
| 128 MiB | 214 | 997 | 799 | 1,033 | 671 |
| 1 GiB | 212 | 1,078 | 823 | 1,069 | 664 |

Ruled out, medians of five at all cores unless noted:

- **Too few blocks**: 1 GiB behaves like 128 MiB.
- **Kernel time**: 0.3 s system against 2.7 s user.
- **Encoder rebuilt per rayon split**: a thread-local encoder gave 680 MB/s. Reverted.
- **Allocator**: glibc with no `mmap` and no trimming gave 640 against 645.

Match table sizes at all 24 threads, medians of five, 8-byte and 5-byte logs:

| Tables | Compress | Ratio |
|---|---|---|
| 2^18, 2^16 | 684 | 3.631 |
| 2^17, 2^16 | 935 | 3.609 |
| 2^16, 2^15 | 1,196 | 3.555 |
| 2^14, 2^13 | 1,362 | 3.368 |
| 2^12, 2^11 | 1,339 | 3.095 |

Fewer threads on all CPUs also help: 978 MB/s at 8, 886 at 12. That fits contention for the shared L3 (30 MB): 24 threads each hold about 1.3 MB of randomly accessed tables and a 4 MiB block. WSL has no hardware counters, so the L3 is inferred, not observed. Decompression has no such tables, and scales.

The curve flattens near 1,360 MB/s, below zstd's 1,633 and the 2,450 SC-002 asks for at all cores, so smaller tables alone cannot pass that gate. Table size cannot follow the thread count either, because output must not depend on it.

### Option A: 2^17 tables and content-based segments

The maintainer chose 2^17 / 2^16 tables on 2026-11-01 and asked to recover the ratio elsewhere. In memory, one core, one run each unless noted:

| Change | Ratio | Compress | Decompress |
|---|---|---|---|
| 2^17 tables | 3.609 | 264 | 1,042 |
| Boundaries, distance 1.0, 8 KiB chunks | 3.647 | 255 | 989 |
| Boundaries, distance 0.7, 8 KiB chunks | 3.656 | 246 | 972 |
| Boundaries, distance 0.35, 8 KiB chunks | 3.621 | 229 | 873 |
| Distance 0.7, Huffman lengths tANS coded (kept) | 3.663 | 245 | 932 |

Kept: a segment ends where a chunk's sampled byte distribution differs from the segment so far by more than 0.7 total variation, with no boundary in the first 16 KiB. Tried and dropped, medians of three: tANS table logs one or two steps larger (3.663 in both cases, no change), and favouring repeat mode by 2%, 5% or 10% (3.662, 3.657, 3.646, at most 2% more decode speed). The second also suggests table rebuilds are not why decoding fell, and that cause is open.

Through the CLI against `zstd -3 -T<n>`, ABBA, 7 reps, 128 MiB in tmpfs, median MB/s. Ratio 3.663 against 3.700, -1.0%:

| | 1 core | 4 cores | all cores |
|---|---|---|---|
| Sprint compress | 220 | 736 | 872 |
| `zstd -3` compress | 201 | 1,010 | 1,729 |
| **Multiple**, gate | **1.09x**, 1.0x | **0.73x**, 1.0x | **0.50x**, 1.5x |
| Sprint decompress | 709 | 2,123 | 3,336 |
| `zstd -3` decompress | 1,075 | 1,217 | 1,221 |
| **Multiple**, gate | **0.66x**, 1.0x | **1.74x**, 2.0x | **2.73x**, 3.0x |

Fuzz on this state: `fuzz_tables`, `fuzz_decompress` and `fuzz_roundtrip`, 50,000 runs each, clean. `fuzz_tables` finishes in under a second, so it is probably rejecting most inputs early.

## Gates
