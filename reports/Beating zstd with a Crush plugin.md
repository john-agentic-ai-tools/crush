# Fork zstd's design, not its format

**Build a hybrid.** Create a new Crush-native codec that adopts zstd's proven low-level structure (separated literal and sequence streams, Huffman literals, tANS/FSE sequences, repcodes, cheap table reuse), then run it on a generic independent-block engine extracted from `crush-parallel`. zstd's advantage comes from entropy coding that costs little encoder time, plus a tightly scheduled scalar match loop. Crush's durable advantage is parallel decompression with an index, which already beats the zstd CLI at 4+ cores. A byte-exact port inherits zstd's curve and at best lands 0-10% behind C (an estimate, not a measurement), while an all-new format risks missing 3.70. The premise needs one correction before planning: 3.70 vs 3.503 is exact, but "2.5 to 4 times faster" is not in the repo. Against zstd -3, the level that actually produces 3.70, zstd compresses **1.83x, 2.41x and 1.41x** faster at 1, 4 and all cores. The ship target is therefore **ratio ≥ live zstd -3 on `real`, compress ≥ zstd -3 at 1 and 4 cores, ≥ 1.5x at all cores, and decompress ≥ 2x at 4 cores**. No zstd build was run for this research, so every time share and ablation figure here derives from published numbers. Six measurements must come first, and they could still change the design. If the codec gates fail, the fallback is libzstd inside the block engine, which needs a maintainer decision on C dependencies.

## The real gap is 1.4x to 2.4x at the level that matters

The figures below come from `perf/comparisons/wsl2-ubuntu-20.04-12th-gen-intel-core-i9-12900k-24t.json` (2026-10-30, commit dc33c90, i9-12900K under WSL2, zstd 1.5.7, 128 MiB, 5 reps, median, no ABBA). The Crush row is `parallel-deflate`, which is what the CLI picks for files of 25 MiB or more (`crush-cli/src/algorithm.rs:48-53`). Multiples are computed from the JSON.

| `real` corpus, MB/s | 1 core | 4 cores | all (24) | ratio |
|---|---|---|---|---|
| crush compress / decompress | 130.1 / 774.7 | 456.5 / 1,697.0 | 1,023.8 / 1,999.1 | 3.503 |
| zstd -3 compress / decompress | 238.2 / 1,099.4 | 1,101.9 / 1,236.4 | 1,445.2 / 1,216.2 | 3.70 |
| zstd -1 compress / decompress | 273.9 / 1,153.3 | 1,467.0 / 1,315.1 | 2,641.7 / 1,307.1 | 3.233 |
| **zstd -3 vs crush, compress** | **1.83x faster** | **2.41x faster** | **1.41x faster** | |
| zstd -1 vs crush, compress | 2.11x faster | 3.21x faster | 2.58x faster | |
| **zstd -3 vs crush, decompress** | **zstd 1.42x faster** | **crush 1.37x faster** | **crush 1.64x faster** | |
| zstd -1 vs crush, decompress | zstd 1.49x faster | crush 1.29x faster | crush 1.53x faster | |

A repo-wide search found no "2.5 to 4 times" claim. The phrase pairs speed from one level with ratio from another: multiples of 2.5x and above on `real` come from zstd -1, which compresses *looser* than Crush (3.233 vs 3.503). Multiples up to 5.54x exist only on the `text` corpus, which is base64 of random bytes (`perf/compare.py:41-42`), so every codec ties at about 1.32. Crush's ratio deficit is modest: its output must shrink by 5.3% to reach 3.70. The compare runs also carry real noise, with spreads of 17-23% at all cores on `real` compression, so the 1.41x row is weak evidence on its own.

**Crush already wins decompression at 4+ cores**, but through architecture, not codec speed. The `compare.py` zstd decompress command has no `-T` flag (`perf/compare.py:95-96`), and the zstd manual describes `-T` for compression only ([zstd manual](https://raw.githubusercontent.com/facebook/zstd/dev/programs/zstd.1.md)). zstd's speed stays flat near 1.2 GB/s, while Crush decodes independent blocks in parallel. That win is not exclusive to Crush's format: pzstd decompresses its own multi-frame files in parallel ([pzstd README](https://git.sharpetronics.com/sharpetronics/zstd/raw/commit/ac14348a283b54603f60353cd3763f9434b73616/contrib/pzstd/README.md)). Compression has a matching soft spot: zstd -3 gains only 31% going from 4 to 24 logical cores in the JSON, and one analysis (version and hardware unstated) found its scaling stops near 5 threads because of buffer availability and in-order flushing ([Easyperf](https://easyperf.net/blog/2024/05/10/Thread-Count-Scaling-Part3)). The 1.41x all-core multiple above reflects that limit.

Two findings from combining the audit data change how those numbers should be read.

The first is that one-core CLI rows understate zstd. Earlier in-memory runs on the same 128 MB corpus, on the same machine family, gave zstd -1 **580 / 1,788 MB/s** (`specs/023-lz4-fast-plugin/research.md:9-14`), against 273.9 / 1,153.3 through the CLI. That is 2.1x and 1.55x higher. zstd -3 also scales *superlinearly* on the CLI, at 4.63x from 1 to 4 cores, and zstd -1 scales 5.36x. *Inference:* a pinned single core is shared with the CLI's I/O work, though no profile proves this. A plugin that passes a CLI gate at 1 core has not necessarily matched zstd's codec. fast-lz shows the size of the effect: in memory it runs at 0.91x / 0.62x of lz4, yet through the CLI it reaches 1.06x / 1.24x (`perf/log/2026-10-31-lz-zero-fill.md:36-38,75`).

The second is that Crush's decompression scaling is weak. It reaches only 2.19x at 4 cores and 2.58x at 24, while independent-block zstd under lzbench reaches 7.2x at T8 on an EPYC 9555P ([lzbench README](https://raw.githubusercontent.com/inikep/lzbench/master/README.md)). *Inference:* a serial pipeline sets the limit, not DEFLATE. The audit found serial per-block `vec!` copies, a serial outer CRC and fully materialised output (`crush-parallel/src/engine.rs:281-282,332-349,357`, `crush-core/src/decompression.rs:40-43`). No profile of this path exists, so the split between those causes is unknown.

## zstd spends bits cleverly and encoder cycles barely at all

Treat zstd's performance as four separable mechanisms. The Silesia figures below come from lzbench 2.4 on one EPYC 9555P thread with turbo off, and they must not be compared directly with the i9-12900K `real` figures above. The corpora differ (zstd -3 is about 3.2:1 on Silesia and 3.70 on `real`), as do the hardware and harness.

**The match loop is cheap because it is scheduled, not because it is vectorised.** `ZSTD_fast` is a single-entry hash table worked by a four-cursor software pipeline ([zstd_fast.c](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_fast.c), [PR #2749](https://github.com/facebook/zstd/pull/2749)). Level 3's `ZSTD_dfast` adds a second table: 8-byte hashes in a 512 KiB long table, 5-byte hashes in a 256 KiB short table, checked at the current position and ip+1 ([zstd_double_fast.c](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_double_fast.c), [clevels.h](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/clevels.h)). Neither uses vector intrinsics, and `lib/compress` has no assembly. `ZSTD_count` is an 8-byte XOR loop and copies are 16-byte SSE2 ([zstd_compress_internal.h](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_compress_internal.h#L879), [zstd_internal.h](https://github.com/facebook/zstd/blob/49cf5179/lib/common/zstd_internal.h#L183)). Branch placement is deliberate: cmov variants are chosen at runtime by `windowLog < 19`, and they lifted silesia.tar in 32 KB blocks from 280 to 343 MB/s on gcc-9 ([PR #4165](https://github.com/facebook/zstd/pull/4165)).

**Entropy coding buys most of the ratio edge over LZ4, and low levels avoid rebuilding what they can reuse.** Match finding writes only to a per-block store of sequences, and entropy coding runs afterwards in one batch ([zstd_compress_internal.h](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_compress_internal.h#L85)). Below `ZSTD_lazy`, zstd selects tables with heuristics rather than cost functions: repeat when valid and under 1,000 sequences, predefined when few ([zstd_compress_sequences.c](https://raw.githubusercontent.com/facebook/zstd/dev/lib/compress/zstd_compress_sequences.c)). Literals below 64 bytes skip Huffman, or below 6 bytes when a repeat table is valid ([zstd_compress_literals.c](https://raw.githubusercontent.com/facebook/zstd/dev/lib/compress/zstd_compress_literals.c)). Sampled head and tail histograms abandon incompressible literals early ([huf_compress.c](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/huf_compress.c#L1370)). *Inference, derived from published speeds and not measured:* the Huffman stage costs roughly 6-9% of level 1 time, from `--fast=1` vs `-1` at 545 vs 510 MB/s on an i7-9700K ([zstd README](https://raw.githubusercontent.com/facebook/zstd/dev/README.md)) and 453 vs 411 MB/s on EPYC ([lzbench table](https://raw.githubusercontent.com/inikep/lzbench/master/doc/lzbench24_sorted.md)). Those two settings differ in parameters as well, so this is a rough bound.

**The two entropy coders fit different data.** FSE beats Huffman where symbol probabilities exceed 0.5, which describes sequence codes. On flat data it gains almost nothing and decodes much slower: at Proba14, FSE gave 1.91 at 460 MB/s and Huff0 gave 1.90 at 860 MB/s ([FiniteStateEntropy README](https://raw.githubusercontent.com/Cyan4973/FiniteStateEntropy/dev/README.md), i7-5600U, dated). Huff0's four independent backward bitstreams let one core run four decode chains, 20 symbols per loop iteration ([huf_decompress_amd64.S](https://raw.githubusercontent.com/facebook/zstd/dev/lib/decompress/huf_decompress_amd64.S)). Good C has nearly closed the gap to assembly: fast C 492 vs asm 501 MB/s ([PR #3827](https://github.com/facebook/zstd/pull/3827)). *Inference:* a Rust decoder does not need `.S` files, though Rust codegen for this loop is unmeasured.

**Decode cost follows sequence and literal counts, not encoder effort.** The fast path is one predictable loop of table loads and over-copies, which keeps decompression roughly level-independent ([zstd_decompress_block.c](https://raw.githubusercontent.com/facebook/zstd/dev/lib/decompress/zstd_decompress_block.c)).

The derived decomposition below shows where the bits and decode time go. All rows are *inferences from published figures* and are not like-for-like, because the steps also change parameters, repcodes and match structure. The direct `--no-compress-literals` measurement was blocked by a permission check when an attempted local zstd build was stopped.

| Step (Silesia, i7-9700K unless noted) | Ratio | Output change | Decode change |
|---|---|---|---|
| lz4 → zstd `--fast=1` (FSE sequences, repcodes, no literal Huffman) | 2.101 → 2.439 | -13.9% | 3,850 → 1,850, -52% |
| `--fast=1` → `-1` (mostly literal Huffman) | 2.439 → 2.896 | -15.8% | 1,850 → 1,550, -16% |
| lizard -10 → -30 (Huffman on LZ4-style tokens, EPYC 9554) | 48.79% → 40.45% size | -17.1% | 2,161 → 1,185, -45% |

Sources: [zstd README](https://raw.githubusercontent.com/facebook/zstd/dev/README.md), [lizard README](https://raw.githubusercontent.com/inikep/lizard/lizard/README.md).

### Which zstd version each claim describes

The notes disagree, and the evidence settles it: **v1.5.7 (2025-02-19) is the latest tagged release, and v1.6.0 is an untagged entry on `dev`.** One note read only the CHANGELOG. The other checked releases and tags, and found that the CHANGELOG entry and `zstd.h` macro say 1.6.0 with no matching tag or release ([releases](https://github.com/facebook/zstd/releases), [CHANGELOG](https://github.com/facebook/zstd/blob/49cf5179/CHANGELOG)). A fresh `gh release list` for this report still shows v1.5.7 as latest.

| Claim set | Applies to |
|---|---|
| All measured speeds and ratios (Crush JSON, lzbench, zstd README) | 1.5.7 |
| Lizard comparison row | 1.5.6 |
| `zstd_fast.c`, `zstd_double_fast.c`, `clevels.h`, `zstd_preSplit.c` | 1.5.7 and `dev` 49cf5179 (zero diff) |
| Huffman, FSE and decoder internals | `dev` as fetched 2026-10-31, not diffed against 1.5.7 |
| Kernel row-finder threshold, RVV/SVE2/NEON paths | post-1.5.7 `dev` only |

## A strict port locks in the curve Crush wants to beat

No surveyed codec beats zstd -1 on compression speed and ratio together in the 2026 Silesia table ([lzbench table](https://raw.githubusercontent.com/inikep/lzbench/master/doc/lzbench24_sorted.md)). Rivals win by moving along another axis. LZAV -1 (451 / 2,527 MB/s, 39.91%) beats `--fast=1` (453 / 1,707, 41.01%) on ratio and decode, with compression tied. ZXC, misa77 and LZSSE get large decode gains by giving up encoder speed. brotli -2 trails zstd -2 badly on both speeds. Oodle has the strongest ratio-plus-decode evidence, but it is vendor-published, old, and silent on fast encoder levels ([Charles Bloom](https://cbloomrants.blogspot.com/2016/07/introducing-oodle-mermaid-and-selkie.html)). OpenZL wins on structured data and falls back to zstd on text ([InfoQ](https://www.infoq.com/news/2025/10/openzl-structured-compression)).

A byte-compatible port has one real strength: it reaches zstd's ratio by construction, with a written format ([RFC 8878](https://www.rfc-editor.org/rfc/rfc8878.txt)). Its weakness is that it can only match the curve. *Inference from analogues:* a port lands 0-10% behind C with unsafe code, SIMD and PGO, and 15-25% behind in safe Rust. The analogues are lz4_flex's safe-build penalty of 21-25% on compression and 15-18% on decode ([lz4_flex](https://github.com/PSeitz/lz4_flex)), and zlib-rs staying 3-6% behind at levels 1-4 as of Feb 2025 ([Trifecta](https://trifectatech.org/blog/zlib-rs-is-faster-than-c/)). Existing Rust work does not close this: ruzstd decodes about 3.5x slower than C on enwik9 ([ruzstd](https://github.com/KillingSpark/zstd-rs)), and structured-zstd shows matching ratios but no usable x86_64 throughput in what was read ([benchmark-delta.md](https://structured-world.github.io/structured-zstd/dev/bench/benchmark-delta.md)). Interoperability is also worth less here than it looks. *Inference:* Crush wraps every payload in its own header (`crush-core/src/plugin/metadata.rs:31-46`), so a `.crush` file is not readable by `zstd` anyway.

An all-new format fails in the other direction. The most tempting idea in the notes is byte-aligned sequences plus Huffman literals, on the theory that sequence decoding causes most of zstd's decode gap. Lizard is the available test of that idea, and it does not support it. Lizard -30 and -40 reach 40.45% and 38.14% at 1,185 and 1,141 MB/s decode, against zstd 1.5.6 -1 at 34.64% and 1,342 MB/s on the same machine ([lizard README](https://raw.githubusercontent.com/inikep/lizard/lizard/README.md)). That is 10-17% larger output *and* 12-15% slower decode, and its authors call it effectively obsolete. Implementation age explains some of this, but the data gives no reason to expect byte-aligned tokens to reach 3.70. *Inference:* reaching zstd -3's ratio needs sequence entropy coding. The byte-oriented codecs near level-1 ratio compress below 45 MB/s (LZAV -2 is 42.8, and ZXC -6, LZSSE and lz4hc -12 are under 15).

The C binding fails the feature's stated goal, since binding libzstd cannot beat libzstd. It is, however, the correct yardstick and the correct fallback. Independent 4 MB blocks cost only about +0.14% size at zstd -1 and +0.54% at -3 (a cross-source approximation), with 7.2x decode scaling at T8 ([lzbench README](https://raw.githubusercontent.com/inikep/lzbench/master/README.md)).

## The hybrid: zstd's stream model on Crush's block engine

The design has two deliverables under feature `026` (the next number; `specs/` undercounts because branches run to 025). The engine comes first because the decompression targets depend on it. Extract a codec-agnostic block engine from `crush-parallel`, which today calls libdeflater directly and has no codec field (`crush-parallel/src/block.rs:6`, `engine.rs:79-85`). A small trait of `compress_block` and `decompress_block_into(&mut [u8])` lets any codec inherit pre-partitioned output, per-worker state, the index, CRCs and stored blocks. The same work removes the phase-1 copy, parallelises or defers the outer CRC, overrides `decompress_to`, and makes `--block` random access a capability rather than hard-wired to one plugin (`crush-cli/src/commands/decompress.rs:51`). parallel-deflate should improve as a side effect, which gives an early, independent payoff.

The codec is a new crate at plugin id 0x05 (0x01 is also unused in production). Its name is left open. The plugin contract already allows it: `compress` receives the whole input and the payload is opaque (`crush-core/src/plugin/contract.rs:93,100`).

| Layer | Take from zstd | Crush-specific choice | Evidence status |
|---|---|---|---|
| Independence unit | Nothing (ZSTDMT overlaps jobs) | 4 MiB independent blocks, no shared window | Published on Silesia; unmeasured on `real` |
| Entropy unit | 128 KiB sub-blocks, border-sampling splitter | Tables and repcodes persist across sub-blocks, reset at each 4 MiB boundary | Splitter -0.3% to -0.5% on silesia.tar; persistence unmeasured |
| Match finder | dfast-class two-table search, cmov selection, complementary insertion | Baseline for parity; test a tag-filtered 2-way or 4-way bucket as an experiment | Bucket is a hypothesis |
| Repcodes | Rep1 probe, rep2 after matches | Same | Contribution unpublished; measure |
| Literals | 4-stream Huffman, 11-bit cap, X1/X2 choice, repeat table, sampling | New container, no RFC constraints | Well evidenced |
| Sequences | tANS/FSE with predefined and repeat modes | Keep for MVP; test separate per-field bit readers later | FSE evidenced; split unproven |
| Incompressible | Sampling and raw fallback | Reuse Crush's probe and borrowed stored blocks | Crush: -87.1% on `compress_incompressible/threads=1` |

Why 4 MiB rather than Crush's current 1 MiB: level 3's window is 2 MiB, so a 1 MiB block would truncate the search range zstd uses, and published data covers 4 MB only. Offsets can span the full block, because independence removes any reason for a smaller window. *Inference:* per-block random access gets about 4x slower than today's 1.14 ms (`README.md:138-151`), which is an acceptable trade if the sweep below confirms the ratio. The 32 MB-per-thread limit (`.specify/memory/constitution.md:320-323`) is comfortably met, since level-3-class tables are 768 KiB and block buffers are small multiples of 4 MiB.

Ship one fixed L3-class profile first. The contract has no level parameter, and crush-gpu's process-global `OnceLock` workaround (`crush-gpu/src/lib.rs:62-72`) should not spread. Keep the codec behind `--plugin` until the gates pass, then change the 25 MiB routing in `crush-cli/src/algorithm.rs`. Registration still needs care: hand arithmetic suggests that adding any non-content-aware plugin shifts min-max scoring built on stale metadata constants (declared 0.65 vs a measured 0.285 for parallel-deflate). `select()` output should be tested before and after registering. That arithmetic has not been run through the real selector.

Several attractive ideas should stay out of the MVP. rANS needs no adoption at all: AVX2 rANS reached about 2x a scalar 4-way decoder, roughly Huff0 parity across different hardware ([rans_static](https://raw.githubusercontent.com/jkbonfield/rans_static/master/README.md)), and post-Downfall gather penalties can reach 2-3x ([htscodecs](https://codegraph.jelmer.uk/htscodecs/1.6.6-1/README.md)). Eight-stream Huffman has no primary benchmark. AVX-512 proved slower in structured-zstd's work ([docs.rs](https://docs.rs/crate/structured-zstd/latest)). Long-distance matching stays off at these levels even in zstd. The GPU path is not an asset here, with a best published decompress of 560 MiB/s (`README.md:121-127`), though *inference:* independent blocks leave a later GPU route open.

## Targets that only pass against a live zstd run

All gates run in one session against live zstd, using `perf/compare.py --tools crush-<new>,zstd-3,zstd-1 --data real --abba --reps 10` at each core set. Reference values are the 2026-10-30 JSON, on the WSL2 i9-12900K only, and the multiple governs rather than the absolute number. No Silesia or EPYC figure is a gate.

| Metric on `real` | 1 core | 4 cores | all cores |
|---|---|---|---|
| Ratio vs zstd -3 (ref 3.70) | ≥ 1.00x | ≥ 1.00x | ≥ 1.00x |
| CLI compress vs zstd -3 | ≥ 1.0x (238) | ≥ 1.0x (1,102) | ≥ 1.5x (2,168) |
| CLI decompress vs zstd -3 | ≥ 1.0x (1,099) | ≥ 2.0x (2,473) | ≥ 3.0x (3,649) |
| In-memory codec vs `zstd -b3` | ≥ 0.85x both ways | n/a | n/a |

Secondary gates are `text` compress ≥ 1.0x zstd -3 at 1 core at ratio ≥ 1.32, memory under 32 MB per thread, `--block` random access, and `compare.py` round-trip checks. The stretch goal is one unclaimed Silesia point from a same-run lzbench race: over 300 MB/s compress, under 34.5%, over 2,500 MB/s decode. Nothing surveyed occupies it, which may equally mean it is out of reach.

The gates vary in difficulty. *Inference, from existing scaling:* **4-core compress is the hardest gate.** Crush's engine scales 3.51x at 4 cores, so 1,102 MB/s needs about 314 MB/s per core at ratio 3.70. That is 4.2 ns/byte at 238 MB/s and 3.2 ns/byte at 314, against 1.6 for fast-lz, so the entropy stage and stronger search have about 1.6-2.6 ns/byte of extra budget. 4-core decompress is close already: a 1,100 MB/s codec at today's 2.19x scaling gives 2,409. The all-core decompress gate requires the engine fix, needing 3.3x scaling against today's 2.58x. The 0.85x in-memory floor exists so a CLI pass cannot hide a weak codec, and the safe-Rust evidence suggests it will be tight without unsafe kernels. A true codec-level win over zstd at one core is not a target, because nothing in the evidence supports promising it.

Sub-10% results are not claims. zstd's own source records 10% decode swings from loop alignment ([zstd_decompress_block.c](https://raw.githubusercontent.com/facebook/zstd/dev/lib/decompress/zstd_decompress_block.c)), and Crush saw +30.7% on `estimate/1MB` from a benchmarks-only commit with identical disassembly (`perf/log/2026-10-31-lz-zero-fill.md:215-234`). So the 1.0x gates need several builds, not one binary. Add a text-like or log corpus too, since `real` is one machine-specific tar head and `text` only tests entropy. The constitution's "criterion" wording (`constitution.md:327`) does not cover external head-to-heads, so the plan must say so.

## Measure these six things before committing the format

Every item below is unmeasured on Crush hardware. The order matters: each result can change or cancel what follows. The ablation builds need a maintainer-approved way to run a downloaded zstd build, since that step is what the permission check blocked.

| # | Measurement | Decides | Stop or redirect if |
|---|---|---|---|
| 1 | `zstd -b1`/`-b3` in memory on `real`, with `perf` on levels 1-3, same session as the CLI runs | Real time shares; reconciles 274 vs 580 MB/s | Entropy share is far above the 6-9% estimate: revisit budget |
| 2 | zstd ablations on `real`: no literal Huffman, no repcodes, predefined FSE only | Which features pay for themselves | A feature gives under 1%: drop it from the MVP |
| 3 | Block-size sweep of zstd -3 at 1, 2, 4, 8 MiB on `real` | Block size, ratio cost | 4 MiB falls clearly below 3.70: use 8 MiB or tune the parser |
| 4 | `parallel-deflate` compress and decompress profile, `CRUSH_TIMINGS=1` | Size of engine serial loss | Serial cost is elsewhere: rescope the engine work |
| 5 | Prototype Rust 4-stream Huffman decode and a dfast loop, safe vs small-unsafe, against zstd on real literals | Unsafe decision, cmov behaviour, codegen | Under 0.8x with unsafe allowed: take the fallback |
| 6 | libzstd behind the extracted engine, as a dev-only harness | The ceiling; proves container value | Fails the decompress gates: the engine, not the codec, is the problem |

A good fallback exists at each exit. If item 5 or later gates fail, libzstd in the block engine still delivers ratio 3.70-class output with parallel decode and an index, subject to the C-dependency decision. Throughput for structured-zstd and mzstd is also worth fetching, because a good pure-Rust encoder would change the build-vs-reuse calculation (mzstd's claimed byte-identity with zstd 1.5.7 is unverified).

## Five risks and six maintainer decisions remain open

The largest risk is **safe Rust missing the budget**. Feature 023 reached its frontier and named unchecked copies or a different algorithm as the next step (`specs/023-lz4-fast-plugin/perf-log.md:188-189`). The second is **ratio shortfall**, because all published block penalties are from Silesia and could differ on `real`. The third is **engine scaling not arriving**: the serial-pipeline diagnosis is an inference from a single table. The fourth is **decoder security**, since table-driven entropy decoding is a larger attack surface than fast-lz, and htscodecs needed a fix for buffer read overruns in its AVX2 rANS path ([htscodecs changes](https://src.opensuse.org/pool/htscodecs/src/branch/factory/htscodecs.changes)). The 100k-iteration fuzz gate and bounded allocation from forged headers apply from day one. The fifth is **format lock-in**: once files exist, the format becomes a compatibility promise, so sequence-layout experiments must finish before the format is frozen.

| Decision for the maintainer | Facts | Recommendation |
|---|---|---|
| C dependency position | No file states one. The constitution never mentions FFI; `libdeflater` ships unlisted (`Cargo.toml:33`); 007 said zstd needs an amendment (`specs/007-parallel-gzip-engine/research.md:383-385`) | Allow libzstd as a dev-only harness now; decide on shipping only if the fallback triggers |
| Unsafe policy for hot paths | Constitution allows it only if "absolutely necessary" (`constitution.md:39-44`); 023 rejected pointer arithmetic (`research.md:56`) | Let item 5 price it, then decide. If needed: one audited kernels module, denied elsewhere, with fuzz and miri |
| Constitution conflicts | Allowed list is stale; `pool.rs` and pooling don't exist despite line 29; `CLAUDE.md` says 1.0.0, live is 1.6.0; targets name gzip and pigz, not zstd; "criterion" wording | One cleanup amendment (7-day feedback, `constitution.md:462-466`) alongside this feature |
| rANS patent | US11234023B2 runs to about 2039 and describes per-fragment model switching, status per Google Patents' own assumptions; **claims were not read** ([Google Patents](https://patents.google.com/patent/US11234023B2/en)) | Stay with Huffman and tANS, which lose nothing measured. Take counsel before any rANS work. tANS predates the filing, but that is a judgement, not a clearance, and this is not legal advice |
| License handling | zstd is BSD or GPLv2 ([zstd README](https://github.com/facebook/zstd)) | Treat any translated code as derivative and retain notices. No review of RAD, Meta or Microsoft patents was done, so do not use reverse-engineered Oodle code |
| What counts as "beating zstd" | CLI and codec results diverge by about 2x at one core | Adopt the table above and publish both numbers in any claim |

Other questions stay unanswered: the origin of the "2.5 to 4" phrase, the file mix of `real`, the ratio penalty below 4 MB blocks, how much repcodes contribute at levels 1-3, whether rustc preserves zstd's cmov and load scheduling, and the Huffman vs sequence vs copy split of decode time.

## Conclusion

The question began as a speed-ratio race against a codec that compresses 2.5-4x faster. The audit turns it into something narrower: match zstd -3's ratio, close a 1.4-2.4x compress gap, and extend a decode lead that Crush already holds but has not fully exploited. Crush's weakest number may be in its own engine rather than in DEFLATE, so the first deliverable pays off even if the codec never ships.

The strategic asymmetry is that zstd's stock CLI gives up parallel decode and scales poorly past a few threads, while the low-level ratio cost of avoiding a shared window is close to zero. Crush can therefore win on the machines people actually use without out-engineering a decade of scalar tuning at one core. The evidence does not support the stronger claim, that a new Rust codec will beat zstd core-for-core, and the plan should not promise it. The six measurements above are cheap, and they decide whether this is a new codec or a better home for an existing one.
