# Alternatives to zstd at low levels, and the state of zstd in Rust

Notes as of 2026-10-31. Derived ratios and multiples are my arithmetic on the cited rows, and they appear under Inferences. Compression ratios below are compressed size as a percentage of original (lower is better) unless written as N:1.

## Which codecs land on or beyond zstd's speed/ratio curve at levels 1-3?

### Takeaway
In the one large same-machine table (lzbench 2.4, Silesia, 2026), no listed codec beats zstd -1 on compression speed and ratio together. Codecs do win in three other ways: LZAV -1 beats zstd's negative levels outright, ZXC/misa77/LZSSE/LZ4HC trade a 40-60x slower encoder for 2-4x faster decode at a similar ratio, and OpenZL wins on structured data only. Oodle is the only family with credible evidence of beating zstd on ratio plus decode speed without giving up much ratio, but that evidence is old, vendor-published, and says nothing about fast-encoder levels.

### Cited Findings

**Reference table.** lzbench 2.4, gcc 15.2.0, AMD EPYC 9555P @ 3.20 GHz, one thread, `performance` governor, turbo off, Ubuntu 26.04, silesia.tar; page is (c) 2026 with no run date. memcpy on this machine is about 24,000 MB/s. All rows from [lzbench sorted table](https://raw.githubusercontent.com/inikep/lzbench/master/doc/lzbench24_sorted.md); setup from [lzbench README](https://github.com/inikep/lzbench).

| Codec | Compress MB/s | Decompress MB/s | Ratio % |
|---|---|---|---|
| zstd 1.5.7 -1 | 411 | 1351 | 34.53 |
| zstd 1.5.7 -2 | 352 | 1257 | 32.70 |
| zstd 1.5.7 -5 | 126 | 1172 | 29.60 |
| zstd --fast=1 | 453 | 1707 | 41.01 |
| zstd --fast=3 | 509 | 1803 | 44.63 |
| zstd --fast=5 | 563 | 1949 | 48.61 |
| lz4 1.10.0 | 550 | 3832 | 47.60 |
| lz4 --fast -17 | 924 | 3977 | 62.15 |
| lz4hc -1 | 259 | 3382 | 42.06 |
| lz4hc -4 | 71.0 | 3587 | 37.65 |
| lz4hc -9 | 31.9 | 3703 | 36.75 |
| lz4hc -12 | 10.4 | 3813 | 36.45 |
| lzav 5.17 -1 | 451 | 2527 | 39.91 |
| lzav 5.17 -2 | 42.8 | 2454 | 34.85 |
| zxc 0.14.1 -1 | 614 | 8151 | 61.76 |
| zxc 0.14.1 -3 | 168 | 4716 | 46.09 |
| zxc 0.14.1 -6 | 9.04 | 3938 | 36.28 |
| misa77 0.6.0 -0 | 250 | 6821 | 44.60 |
| misa77 0.6.0 -1 | 73.8 | 7813 | 42.65 |
| misa77 0.6.0 -3 | 14.4 | 6470 | 37.76 |
| misa77 0.6.0 -4 | 10.1 | 5177 | 35.51 |
| lzsse8 -1 | 17.6 | 4620 | 38.63 |
| lzsse8 -6 | 9.02 | 5007 | 35.61 |
| lzsse4fast | 266 | 3673 | 45.26 |
| lizard 2.1 -10 | 464 | 1911 | 48.79 |
| lizard 2.1 -12 | 165 | 1772 | 40.69 |
| lizard 2.1 -30 | 356 | 1154 | 40.45 |
| lizard 2.1 -42 | 137 | 1067 | 34.61 |
| lizard 2.1 -45 | 22.5 | 1147 | 31.46 |
| snappy 1.3.1 | 457 | 976 | 47.85 |
| brotli 1.2.0 -0 | 341 | 322 | 37.01 |
| brotli 1.2.0 -2 | 139 | 378 | 32.12 |
| brotli 1.2.0 -5 | 53.3 | 419 | 28.10 |
| libdeflate 1.26 -1 | 202 | 783 | 34.68 |
| libdeflate 1.26 -6 | 82.1 | 820 | 31.85 |
| slz_gzip -1 | 316 | 329 | 46.97 |
| zlib-ng 2.3.3 -1 | 196 | 466 | 44.41 |
| zlib 1.3.2 -1 | 89.0 | 297 | 36.45 |
| bsc 3.3.12 -m0 -e1 | 19.4 | 25.0 | 23.26 |
| density 0.16.6 -1 | 1630 | 1617 | 62.77 |
| density 0.16.6 -3 | 442 | 578 | 43.56 |
| brieflz 1.3.0 -1 | 192 | 293 | 38.28 |
| brieflz 1.3.0 -6 | 22.8 | 328 | 31.71 |
| kanzi 2.5.3 -1 | 196 | 1112 | 37.44 |
| memlz 0.5 beta | 4452 | 2039 | 59.90 |
| skim 0.1.0 | 2313 | 3500 | 64.53 |

The table has no zstd -3 or -4 row. The nearest zstd -3 figures are below.

**zstd -3 and ZXC 0.15.0 across machines.** Project-published (self-reported), lzbench 2.4.1, `-march=native`, Silesia. Each cell is compress / decompress MB/s — [ZXC README](https://github.com/hellobertrand/zxc).

| Codec | Apple M2 | Axion (Neoverse-V2) | EPYC 9B45 (Zen 5) | EPYC 7B13 (Zen 3) | Ratio % |
|---|---|---|---|---|---|
| zstd 1.5.7 -1 | 645/1803 | 523/1642 | 606/1898 | 444/1339 | 34.53 |
| zstd 1.5.7 -3 | 375/1680 | 329/1525 | 369/1733 | 232/1178 | 31.20 |
| lz4 1.10.0 | 814/4774 | 730/4258 | 766/5052 | 638/3883 | 47.60 |
| zxc 0.15.0 -1 | 886/13513 | 885/9564 | 868/11609 | 724/8155 | 61.76 |
| zxc 0.15.0 -3 | 267/9707 | 256/6571 | 253/8335 | 214/5252 | 46.65 |
| zxc 0.15.0 -5 | 114/8118 | 109/5683 | 110/7014 | 93.3/4779 | 40.83 |
| zxc 0.15.0 -6 | 14.5/6884 | 13.5/4950 | 15.3/6045 | 11.9/4290 | 36.28 |
| zxc 0.15.0 -7 | 9.71/5054 | 8.82/3563 | 10.2/4724 | 7.97/3188 | 33.09 |

**Oodle.** Vendor-published in all cases; no independent Silesia table found.
- Oodle 2.3.0 -z6, Win64, Core i7-3770 @ 3.4 GHz, Silesia as a per-file sum (2016): Kraken 4.082:1 at 999 MB/s decode; Mermaid 3.571:1 at 2022; Selkie 3.053:1 at 2930; zstdmax 4.013:1 at 468; zlib9 3.128:1 at 359; lz4hc 2.723:1 at 2267 — [Charles Bloom](https://cbloomrants.blogspot.com/2016/07/introducing-oodle-mermaid-and-selkie.html). These are high-effort encodes; the zstd 2016 version is unstated, and Bloom notes zstd runs about 25% faster on recent GCC -O3 than MSVC.
- Oodle 2.6.0 Kraken Optimal4: 4.24:1 at 985.4 MB/s decode on Silesia — [RAD release notes](https://radgametools.com/oodlewhatsnew.htm).
- Unreal docs: Selkie about 1.86:1 at 4233 MB/s, Mermaid 2.21:1 at 2649, Kraken 2.44:1 at 1467, Leviathan 2.62:1 at 962; corpus not named. Encode speed is set by the compression level, not the compressor — [Epic](https://dev.epicgames.com/documentation/unreal-engine/oodle-data?lang=en-US).
- RAD claims Leviathan reaches roughly LZMA ratios with 10x faster decode — [RAD](https://www.radgametools.com/oodlecompressors.htm).
- Independent, non-Silesia float data (2023): Aras Pranckevicius found Mermaid and Kraken ahead of zstd and brotli on the Pareto front — [Aras](https://aras-p.info/blog/2023/01/31/Float-Compression-2-Oodleflate/).

**Other codecs.**
- zstd's own table, i7-9700K @ 4.9 GHz, gcc 14.2, lzbench, Silesia: zstd -1 2.896:1, 510/1550; brotli -1 2.883:1, 290/425; zlib -1 2.743:1, 105/390; lz4 2.101:1, 675/3850; snappy 2.089:1, 520/1500 — [zstd README](https://github.com/facebook/zstd). This fills the brotli -1 hole in lzbench.
- LZAV 5.20 on Ryzen 3700X @ 4.2 GHz, clang-cl 18.1.8: 565/3270 at 39.91%; HI mode 125/3100 at 34.85%. The author's zstd 1.5.5 -1 run on that CPU (TurboBench) gives 436/1400 at 34.6%. The README header claims a better ratio than zstd -1, which its own Silesia numbers contradict — [LZAV README](https://github.com/avaneev/lzav). On per-dataset results LZAV beats LZ4, Snappy and LZF everywhere listed.
- LZSA is designed for 8-bit micros (Z80, 6502). Its Silesia ratios are LZSA1 34.78% and LZSA2 32.52%, against LZ4_HC 36.47%; no x86 speed figures, and the compressor is about 2x slower than LZ4_HC — [LZSA README](https://github.com/emmanuel-marty/lzsa).
- S2 on a 4.01 GB webdevdata subset, single goroutine, amd64 (project-published): S2 936.73 MB/s, 73.60% reduction; S2 Better 451.10, 76.05%; Snappy 790.15, 71.89%; LZ4 452.02, 73.50%; Go zstd Fastest 236.04, 80.21%; Go zstd Best 35.63, 82.45%. The zstd rows are the Go port, not C — [S2 README](https://github.com/klauspost/compress/tree/master/s2).
- OpenZL launch, M1 Mac, Silesia SAO file only: ratio 2.06 at about 340 MB/s vs zstd -3 1.31 at 220 MB/s vs xz -9 1.64 at 3.5 MB/s — [Meta (Oct 2025)](https://engineering.fb.com/2025/10/06/developer-tools/openzl-open-source-format-aware-compression-framework).
- OpenZL in production at Meta: Nimble columnar about 10% smaller than zstd without training and about 15% with; a Thrift feature cache 18% smaller — [OpenZL paper (2026)](https://arxiv.org/pdf/2605.09928). It falls back to zstd on data with little structure, such as text — [InfoQ](https://www.infoq.com/news/2025/10/openzl-structured-compression).
- OpenZL 0.3 (reported 2026-09-29) claims decompression "around 144% faster than Zstandard" at equivalent settings, and its transformer about 35% better than zstd -19; the article states no zstd version, hardware or data — [Phoronix](https://www.phoronix.com/news/OpenZL-0.3-Released). Vendor claim, unverified.
- New 2023-2026 entries in lzbench's supported list: aceapex, bzip3, go-brrr, kanzi, mbrotli, memlz, misa77, mzstd, openzl, pulsar, skim, tamp, wlz4/wzip, zlib-turbo, zxc — [lzbench](https://github.com/inikep/lzbench). Age of each is my reading, not stated by the page.

### Inferences
- **zstd -1 is undominated in the table above** on compress-speed plus ratio. libdeflate -1 comes closest on ratio (34.68%) at half the speed and 58% of the decode speed. brotli -0 is dominated by zstd -1 on all three axes.
- **zstd's negative levels are the soft spot.** LZAV -1 (451/2527/39.91) essentially dominates zstd --fast=1 (453/1707/41.01): 2.7% smaller, 1.48x decode, compress speed a tie. It also dominates --fast=3 and --fast=5 on ratio and decode, though not on compress speed. Against zstd -1 it is 1.10x on compress, 1.87x on decode, and output 15.6% larger. This is the clearest evidence that an entropy-free LZ with a tuned format can win against zstd's weak end.
- **Encoder-asymmetric codecs beat zstd -1 on ratio-plus-decode only by surrendering compression speed.** ZXC -7 on EPYC 9B45 is 4.2% smaller and 2.49x faster to decode but 59x slower to compress. misa77 -0 (250/6821/44.60) matches zstd --fast=3's ratio at 3.8x decode and about half the compress speed, which is the most interesting small-sacrifice point seen here.
- **An "over 300 MB/s, under 34.5%, over 2500 MB/s decode" codec is an empty region of this table.** That is the target a Crush plugin would claim. Nothing surveyed occupies it; Oodle may approach it on decode, but its fast-encoder levels have no public numbers.
- Brotli -2 is 1.8% smaller than zstd -2 but 2.5x slower to compress and 3.3x slower to decode, so it is off the curve at low levels.
- LZ4 vs zstd -1 is the classic trade: 1.34x compress, 2.84x decode, 37.8% larger output.
- Lizard is effectively matched or beaten by zstd and LZAV at comparable settings. BSC, Pulsar and brieflz are not contenders at these speeds: BSC wins ratio by a wide margin but decodes 50x slower than zstd.
- memlz (4452 MB/s) and skim (2313 MB/s) extend the far-right speed end but compress worse than zstd's fastest levels by a wide margin.
- OpenZL is a different category. It is not a general-purpose competitor on Silesia as a whole, and for a generic plugin the relevant idea is format-aware transforms, not the framework.
- Name collision: lzbench already lists a codec called "crush 1.0", unrelated to this project.

### Gaps
- No Oodle numbers at fast encoder levels (HyperFast, SuperFast, VeryFast), and no same-machine Oodle vs zstd 1.5.x table. The SDK needs a licence.
- lzbench lists openzl, mzstd, go-brrr, wlz4, and nvcomp but publishes no rows for them in the pages read.
- misa77, skim, and memlz designs were not investigated.
- No independent verification of OpenZL 0.3 claims; the Phoronix chart's values were not extractable.
- LZSSE's own numbers are in blog posts that were not read; the lzbench rows stand in.

## What are the specific architectural reasons for wins?

### Takeaway
Fast-decode wins come from removing or restructuring entropy coding and from branchless, SIMD or multi-stream decoding. The encoder then spends its time choosing tokens the decoder can run cheaply. High-ratio low-level wins come from richer format features rather than better hashing alone.

### Cited Findings
- Kraken, Mermaid and Selkie make space-speed decisions in the encoder, trading compressed size against decode time; Kraken favours ratio, Selkie speed, Mermaid between. Scratch memory beyond in/out buffers is 256 KB — [Charles Bloom](https://cbloomrants.blogspot.com/2016/07/introducing-oodle-mermaid-and-selkie.html).
- Community reimplementation of Oodle's Huffman decode reads three interleaved bitstreams: two forward from start and middle, one backward from the end, with an 11-bit lookup table — [oozextract source](https://docs.rs/crate/oozextract/latest/source/src/decoder/huffman.rs). This is reverse-engineered, not RAD documentation.
- LZSSE is LZSS "designed for a branchless SSE decompression implementation", with an optimal parser pairing a strong match finder and Storer-Szymanski-style parse. LZSSE4 and LZSSE8 also have hash-table greedy "fast" compressors; LZSSE8 is the recommended general choice — [LZSSE](https://github.com/ConorStokes/LZSSE). The parser's cost shows in lzbench at about 9 MB/s, with lzsse4fast at 266 MB/s — [lzbench table](https://raw.githubusercontent.com/inikep/lzbench/master/doc/lzbench24_sorted.md).
- LZAV is scalar, portable, header-only C99, using hash-table match finding, a dynamic 2 MiB-512 MiB window, and a stream format more complex than LZ4's, which the README gives as the reason for lower peak decode speed. It does bounds-checked decoding of malformed input. The README does not specify the hash function — [LZAV README](https://github.com/avaneev/lzav).
- ZXC targets write-once, read-many workloads: the encoder does the expensive work so the decoder can be very fast. It also offers seekable archives, in-place decode, and trained dictionaries — [ZXC README](https://github.com/hellobertrand/zxc).
- S2 extends Snappy with framed blocks up to 4 MB (from 64 KB), a repeat-offset code, and AMD64/ARM64 assembly for both encode and decode. It claims 5-20% better compression and 25-40% higher single-threaded compression throughput. Without assembly, decode still reached 2289.8 MB/s on consensus.db.10gb — [S2 README](https://github.com/klauspost/compress/tree/master/s2).
- OpenZL represents compression as a graph of transforms, with users supplying the data shape via a preset or format description and a trainer generating plans — [InfoQ](https://www.infoq.com/news/2025/10/openzl-structured-compression); [OpenZL paper](https://arxiv.org/pdf/2605.09928).

### Inferences
- The LZAV result suggests a byte-oriented format with smarter token layout can capture most of the gap between LZ4 (47.6%) and zstd --fast (41%) with no entropy stage and no SIMD. That makes it a good fit for safe Rust.
- Reaching about 34% at encode speeds above 300 MB/s probably needs some entropy coding on literals, because the byte-oriented codecs near that ratio (ZXC -6, LZSSE, lz4hc -12, misa77 -4) all compress at under 15 MB/s, and LZAV -2 manages only 42.8 MB/s. Oodle's published design is the known answer: entropy-code selectively, and let the encoder price decode time.
- Rate-distortion on decode time is the transferable idea, and it costs encoder CPU, which is the one budget low levels do not have. A cheap heuristic version would be required, such as per-block "raw vs Huffman literals" decisions.
- S2's lesson is that assembly matters less than format changes; its no-assembly decode remains fast.

### Gaps
- I could not reach Bloom's Kraken design post (404 at the guessed URL), so the "two-phase decode" and side-by-side-stream details commonly attributed to Kraken are unverified here.
- From background knowledge only, not verified this session: zstd's four-stream Huffman literals and FSE sequences (RFC 8878), and BMI2 Huffman loops, are the main decode differences from LZ4-class formats.
- LZAV's hash function and hash table layout are not documented in the README; reading the header would settle it.
- The two LZSSE blog posts explaining the decode mechanics were not read.

## Parallel designs: independent blocks vs shared windows

### Takeaway
At low levels, independent 4 MB blocks cost almost nothing in ratio (about 0.1-0.5%) and give near-linear parallel decompression. The penalty only becomes real at high levels. This strongly favours independent blocks for a low-level Crush plugin.

### Cited Findings

**Independent 4 MB blocks.** lzbench 2.4, `-eMAINSTREAM -b4096 -T# -t8,8 -o4`, EPYC 9555P, pinned with taskset, silesia.tar (51 blocks, so T32 is capped near 25x) — [lzbench README](https://raw.githubusercontent.com/inikep/lzbench/master/README.md); MB/s:

| Codec | Ratio % | Comp T1 | Comp T8 | Comp T32 | Dec T1 | Dec T8 | Dec T32 |
|---|---|---|---|---|---|---|---|
| lz4 1.10.0 | 47.62 | 537 | 3915 | 11671 | 3390 | 26212 | 77376 |
| zstd --fast=1 | 41.05 | 427 | 2829 | 9080 | 1613 | 12298 | 37663 |
| zstd -1 | 34.58 | 400 | 3039 | 8672 | 1303 | 9367 | 31226 |
| zstd -3 | 31.37 | 240 | 1524 | 4627 | 1151 | 8364 | 23142 |
| zstd -7 | 28.75 | 80.1 | 472 | 1645 | 1222 | 8376 | 22130 |
| zstd -22 | 25.88 | 3.01 | 18.2 | 59.6 | 1103 | 7300 | 19522 |

- Single-threaded whole-file ratios for comparison: zstd -1 34.53, lz4 47.60, zstd --fast=1 41.01, zstd -22 24.67 — [lzbench table](https://raw.githubusercontent.com/inikep/lzbench/master/doc/lzbench24_sorted.md); zstd -3 31.20 — [ZXC README](https://github.com/hellobertrand/zxc).
- pzstd splits input into equal chunks, compresses each into its own zstd frame, and writes a 12-byte skippable-frame header giving the next frame's size. It decompresses in parallel only for files it produced; other zstd files get one I/O and one decode thread. Dictionary mode is unsupported — [pzstd README](https://git.sharpetronics.com/sharpetronics/zstd/raw/commit/ac14348a283b54603f60353cd3763f9434b73616/contrib/pzstd/README.md). The same README says pigz cannot decompress in parallel and only separates reading, decompression, and writing into threads.
- `zstd -T` splits work into jobs of generally 4x windowSize (minimum 512 KB). overlapLog reloads up to one windowSize from the previous job, 6-9 automatic by strategy: "Reloading more data improves compression ratio, but decreases speed". The manual describes -T only for compression — [zstd manual](https://raw.githubusercontent.com/facebook/zstd/dev/programs/zstd.1.md).
- S2 streams split across all cores. `DecodeConcurrent` on a Ryzen 3950X (16 cores): enwik10.s2 1303.5 MB/s at 1 CPU, 4847.9 at 4, 9592.1 at 16. Compression on a 16-core/32-thread machine reached 6520 MB/s on silesia.tar and 8440 MB/s on enwik9. An index in skippable chunks gives random seek — [S2 README](https://github.com/klauspost/compress/tree/master/s2).
- rapidgzip decodes almost any gzip file in parallel. Ryzen 3900X, Silesia: gzip 293 MB/s; rapidgzip at 24 threads 5750 MB/s with index, 2040 from scratch. On 2x EPYC 7702 (128 cores): about 24 GB/s with index and 12 without on Silesia, bgzip input about 22 GB/s, pigz output about 6 GB/s. It reports 24.3 GB/s vs 9.5 GB/s for pzstd, and says standard zstd files decompress at about 800 MB/s and cannot be parallelized — [rapidgzip](https://github.com/mxmlnkn/rapidgzip). Project-published.

### Inferences
- Size penalty of independent 4 MB blocks: zstd -1 +0.14%, lz4 +0.04%, zstd --fast=1 +0.10%, zstd -3 +0.54% (cross-source, so approximate), zstd -22 +4.9%. Low-level match searches evidently rarely use distant data, so the window is not the budget to spend there.
- Scaling from these rows: zstd -1 7.6x compress and 7.2x decompress at T8, 21.7x and 24.0x at T32, against a 25x cap. Block-independent designs scale close to linearly on both sides.
- Crush's existing engine targets pigz with parallel blocks. Block independence plus an index gives parallel decode and seek at effectively zero ratio cost at low levels, which pigz's format cannot.
- Shared-window designs (`zstd -T`, pigz) buy ratio that matters only at high levels, and they give up parallel decode. For a low-level plugin the shared window is a poor trade.
- Below about 1 MB blocks, the trade likely changes quickly, and it should be measured. The lzbench data only cover 4 MB.
- rapidgzip shows that a format can be retrofitted for parallel decode, but with a 2.8x penalty without an index. Designing the index in is clearly better.

### Gaps
- BGZF spec could not be read (the SAM PDF was unparseable); its 64 KiB block limit and virtual offsets are from background knowledge only. No measured BGZF ratio penalty was found.
- pigz's block size and dictionary sharing (128 KiB blocks with a 32 KiB preceding dictionary, from background knowledge) are unverified; its README does not cover them.
- No direct measurement of the ratio penalty below 4 MB per block at zstd -1..-3 was found, and no direct numbers for `zstd -T` overlap vs independent jobs.

## GPU and heterogeneous compression

### Takeaway
GPU decode can reach tens to over 100 GB/s, but the striking zstd figure comes from dedicated decompression hardware on Blackwell, not compute shaders. Public, comparable numbers for nvCOMP zstd on general GPU paths were not recoverable. GPU work is a data-centre and game-asset niche, not a reason to change a CPU codec's design.

### Cited Findings
- NVIDIA Blackwell B200 hardware Decompression Engine via nvCOMP: Zstandard 77.5 GB/s input, 154.9 GB/s output — [arXiv 2512.02189 (Dec 2025)](https://arxiv.org/pdf/2512.02189).
- Older NVIDIA figure: LZ4 decompression 69.46 GB/s on a mortgage float column — [NVIDIA slides](https://reprises.in2p3.fr/ressource/Presentation/CWE41740_Slide_Deck_1648076975326001TKn2.pdf). The nvcomp repo cites LZ4 "up to 60 GB/s" on an earlier release — [nvcomp](https://github.com/nv-dlasalle/nvcomp). Vendor figures on favourable columnar data.
- GSST (TU Delft, 2025) reports 191 GB/s decompress at 2.74 ratio on A100 for strings. It says nvCOMP zstd reaches higher compression with significantly lower decompression throughput, and that GSST dominates nvCOMP's LZ4, Snappy, and GDeflate — [TU Delft](https://repository.tudelft.nl/file/File_627b50ef-4c9a-4367-bd9c-b640c978edff). Research claim on string data; figures in the paper's chart were not extractable.
- nvCOMP release notes mention up to 28% zstd decompression speedup on a newer GPU with large datasets, and GDeflate high-compression mode up to 2x faster — [nvCOMP release notes](https://docs.nvidia.com/cuda/nvcomp/release_notes.html).
- A secondary blog says Microsoft added zstd alongside GDeflate to DirectStorage, with the GPU shader aimed at content chunked to 256 KB or smaller — [Borecraft](https://borecraft.com/findings/helix-zstd-directstorage-findings.html). Not verified against Microsoft.
- lzbench 2.4 lists nvcomp 2.2.0 and gpucompact 1.1 as supported compressors — [lzbench](https://github.com/inikep/lzbench).

### Inferences
- GPU decode favours small, independently decodable chunks. This aligns with the block-independent design above, so a block format would not close off a later GPU path.
- Transfer overheads and small-chunk requirements mean headline GB/s do not translate to end-to-end file throughput. No source here measured that directly.

### Gaps
- NVIDIA's benchmark page exposes charts only, with no extractable numbers — [nvCOMP benchmarks](https://docs.nvidia.com/cuda/nvcomp/benchmarks.html). No clean nvCOMP zstd software-path number was found, nor a GDeflate vs zstd head-to-head.
- No GPU compression (as opposed to decompression) throughput at zstd-like ratios was found.

## Rust: state of the implementations, and what a port would achieve

### Takeaway
Rust has reached C parity or better on simple formats (LZ4, Snappy, DEFLATE decode) but required `unsafe`, SIMD, and runtime dispatch to get there. For zstd, structured-zstd now covers all levels and matches C ratio on the rows seen, but I found no usable x86_64 throughput numbers, and ruzstd is still well behind on decode. A careful port should land within roughly 0-10% of C, while strictly safe Rust would likely cost 15-25%.

### Cited Findings
- **zstd (crate):** bindings to C zstd, with `zstd-safe` and `zstd-sys` layers, `zstdmt`, and vendored or system builds. BSD-3-Clause — [zstd-rs](https://github.com/gyscos/zstd-rs).
- **ruzstd:** Uncompressed plus Fastest (about 1), Default (about 3), Better (about 7), Best (about 11). The README says the compressor is usable but does not yet match C on speed, ratio, or configurability. Decoder is about 3.5x slower than C on enwik9 and about 1.4x on an Ubuntu ISO — [ruzstd](https://github.com/KillingSpark/zstd-rs).
- **structured-zstd:** 0.2.6 dated 2026-10-26, Apache-2.0, forked from ruzstd (MIT). Covers every C level (-131072..=22) and calls its RFC 8878 decoder "production-grade". SIMD is runtime-dispatched (SSE2/SSE4.2, BMI2, AVX2, NEON); AVX-512 is opt-in because it measured slower on this workload — [docs.rs](https://docs.rs/crate/structured-zstd/latest). An earlier search result showed 0.0.49 as "latest", so versions moved quickly or the index was stale.
- structured-zstd ratios on `decodecorpus-z000033`, i686-gnu target, Rust vs C (compressed fraction): --fast 1 0.582341 vs 0.582618; level 1 0.558811 vs 0.559203; level 2 0.538778 vs 0.539032; level 3 0.484755 vs 0.488155 — [benchmark-delta.md](https://structured-world.github.io/structured-zstd/dev/bench/benchmark-delta.md). Project CI, synthetic data, 32-bit target.
- Its presets map to C levels 1/3/7/13, and cross-validation only bounds compression time at 1.5x the reference CLI, a pass/fail limit rather than a measured gap — [BENCHMARKS.md](https://github.com/structured-world/structured-zstd/blob/main/BENCHMARKS.md).
- **lz4_flex** v0.11.0 (2023), Ryzen 7 5900HX @ 3 GHz, rustc 1.69, vs lz4 1.9.3 in MiB/s (compress/decompress): 66 KB JSON safe 1272/4540, unsafe 1615/5512, unsafe+unchecked 1615/5973, C 1469/5313. 10 MB dickens safe 259/2338, unsafe 347/2734, unsafe+unchecked 347/3168, C 324/2759 — [lz4_flex](https://github.com/PSeitz/lz4_flex). Project-published.
- **snap:** on i7-6900K vs Snappy C++ 1.1.8, compression within a few percent (1016.2 vs 1033.1 MB/s on html) and some decompress cases slightly faster (930.2 vs 857.2 MB/s on txt4). Compression is byte-for-byte identical to C++ — [rust-snappy](https://github.com/burntsushi/rust-snappy).
- **miniz_oxide:** pure Rust with no unsafe code, and a flate2 backend. Versions before 0.8.4 had a serious compression regression on Rust 1.81+ from an upstream issue — [miniz_oxide](https://github.com/Frommi/miniz_oxide).
- **zlib-rs** in Aug 2024, x86_64 AVX2, silesia-small.tar: slower than zlib-ng by +20.2% at level 1, +5.7% at level 6, +2.8% at level 9, +4.8% decode. Higher instruction counts at every level (642M vs 468M at level 1), attributed largely to well-predicted bounds checks — [Trifecta](https://trifectatech.org/blog/current-zlib-rs-performance/).
- **zlib-rs 0.4.2** in Feb 2025: decompress vs zlib-ng -12.95% at 1 KB chunks, -6.94% at 65 KB, +1.40% at 16 B. Compress -6.19% at level 6, -13.64% at level 9, but +3.33% to +6.18% slower at levels 1-4 — [Trifecta](https://trifectatech.org/blog/zlib-rs-is-faster-than-c/). The workplan describes being within a couple of percent of matching zlib-ng across the board on x86_64 — [workplan](https://trifectatech.org/initiatives/workplans/data-compression).
- zlib-rs lessons: multiversioning "is not natively supported in rust" and manual runtime dispatch requires unsafe. Rust's lack of switch fallthrough hurt small-chunk DFA decode, and `-Cllvm-args=-enable-dfa-jump-thread` recovered most of it, though LLVM does not enable that by default — [Trifecta](https://trifectatech.org/blog/zlib-rs-is-faster-than-c/). Its earlier benchmark used `target-cpu=native`, an unfair comparison with zlib-ng's runtime checks. uv later found compile-time-only feature detection noticeably slower on x86 and switched to runtime detection — [uv-dev](https://git.joshthomas.dev/mirrors/uv/src/branch/zb/snapshot/crates/uv-dev).
- misa77 "safe" variants in lzbench decode about 1-1.5% slower than the default — [lzbench table](https://raw.githubusercontent.com/inikep/lzbench/master/doc/lzbench24_sorted.md). What "safe" means there was not verified.

### Inferences
- **Safe vs unsafe cost:** lz4_flex safe trails its unsafe build by 21-25% on compression and 15-18% on decode. That is the best current estimate for byte-copy-heavy hot loops. zlib-rs's bounds-check cost looked smaller in wall time than in instruction count, since those branches predict well.
- **Rust vs C:** with unsafe, Rust ranges from a few percent slower to about 10% faster (lz4_flex +10% compress on JSON, snap about parity). A Rust port should not expect a language-level win; where Rust beat C, it had usually gone through more tuning passes or the target had easier conditions.
- **zlib-rs's weak spot is exactly Crush's target:** levels 1-4 stayed 3-6% behind zlib-ng in the Feb 2025 numbers while higher levels went ahead. Low-level encoders are the most tightly optimized code path, so expect them to be the hardest to match.
- **Porting zstd -1:** a realistic estimate is 0-10% behind C with unsafe, SIMD, and PGO, and about 15-25% behind in safe-only Rust. This is my estimate from the analogues above, not a measurement.
- **Crush constitution tension:** "100% memory safe Rust" and the 500 MB/s target pull in opposite directions in hot paths, since std::arch loads and stores still need unsafe. A small, audited unsafe core plus fuzzing seems the practical resolution. The `zstd` binding would also bring in a C dependency outside the constitution's allowed list.
- **Lower-risk option:** a new LZAV-class format in safe Rust avoids both the bit-reader cost and zstd's format constraints, and it targets the soft spot found above. A direct zstd port cannot beat zstd's curve by much, since it inherits the format; it can only match it.

### Gaps
- structured-zstd has no usable x86_64 throughput rows in what was read. The delta file is 923 KB and only the first 100 KB was read, the speed tables seen were cut off, and the dashboard needs JavaScript. Fetching `benchmark-report.md` or `data.js` could fill this. So whether a pure-Rust zstd encoder is speed-competitive by level remains unanswered.
- No ruzstd encoder speed or ratio by level was found, and the README gives no unsafe policy.
- The lz4_flex numbers date from 2023 and rustc 1.69.

## Does Rust offer compile-time SIMD/PGO advantages? std::arch and portable SIMD status

### Takeaway
No real advantage over C, and rough parity at best. As of late 2026, `std::simd` is still nightly-only; stable work uses `std::arch` with `#[target_feature]`, plus crates for dispatch. PGO is available through LLVM as in Clang.

### Cited Findings
- As of 2026-09-25, `std::simd` is nightly-only with "infrequent breaking API changes", and it sits directly on LLVM, so unsupported operations can get no SIMD — [State of SIMD in Rust 2026](https://shnatsel.github.io/state-of-simd-rust-2026/).
- Rust 1.87+ allows safe calls to platform intrinsics inside `#[target_feature]` functions. Loads and stores still need unsafe because they take raw pointers, and calling a feature-gated function from an unannotated function still needs unsafe — [same article](https://shnatsel.github.io/state-of-simd-rust-2026/).
- Multiversioning crates: `multiversion` (for autovectorized code, with call overhead), `fearless_simd`, `pulp`, `macerator`; `wide` is incompatible with multiversioning. It recommends `-C target-cpu=x86-64-v3` only when you control the hardware — [same article](https://shnatsel.github.io/state-of-simd-rust-2026/). A commenter reports a recent fearless_simd 1.0 release — [HN](https://news.ycombinator.com/item?id=49844629).
- rustc PGO is LLVM-based, equivalent to Clang's `-fprofile-generate`/`-fprofile-use`, and uses `-Cprofile-generate`, `llvm-profdata merge`, then `-Cprofile-use`; `cargo-pgo` automates this — [rustc book](https://doc.rust-lang.org/rustc/profile-guided-optimization.html). The page has no nightly-only notice but does not state stability explicitly.

### Inferences
- PGO should help Rust and C about equally, given the shared LLVM backend, and it applies to shipped binaries only, so library consumers do not benefit by default.
- Target_feature with a feature-token pattern is the practical design for Crush: safe intrinsics in the kernels, with unsafe limited to dispatch and pointer loads.
- structured-zstd's finding that AVX-512 was slower argues against assuming wider vectors help; AVX2 and BMI2 tiers are the priority.

### Gaps
- No stable release date for `std::simd` was found, and no official Rust project status statement was read. The AVX-512 intrinsic stabilization version was not found in sources read.
- No published measurement isolates PGO gain on a Rust compression codec.

## License facts and implications of a direct port

### Takeaway
A zstd port is legally straightforward under its BSD option. Oodle cannot be ported, and its format is proprietary, so only the published ideas are safely reusable.

### Cited Findings
- "Zstandard is dual-licensed under BSD OR GPLv2" — [zstd README](https://github.com/facebook/zstd). The `zstd` Rust binding is BSD-3-Clause, and the C legacy decoders also include BSD-2-Clause code — [zstd-rs](https://github.com/gyscos/zstd-rs).
- structured-zstd is Apache-2.0 and keeps ruzstd's MIT notice in a NOTICE file — [docs.rs](https://docs.rs/crate/structured-zstd/latest).
- Permissive alternatives: LZAV MIT — [LZAV](https://github.com/avaneev/lzav); ZXC BSD-3-Clause — [ZXC](https://github.com/hellobertrand/zxc); rapidgzip Apache-2.0 and MIT — [rapidgzip](https://github.com/mxmlnkn/rapidgzip).
- Oodle is a proprietary commercial SDK from RAD/Epic — [RAD](https://www.radgametools.com/oodlecompressors.htm). Open-source decoders such as oozextract are reimplementations rather than RAD code — [oozextract](https://docs.rs/crate/oozextract/latest/source/src/decoder/huffman.rs).

### Inferences
- A Rust translation of zstd is a derivative work, so taking the BSD option means retaining the copyright notice and conditions. This is not legal advice.
- Fixed-format zstd compatibility gives Crush the ecosystem but also fixes the curve. A new format has the freedom to beat it but no interoperability.
- Oodle-derived reverse-engineered code carries clear IP risk and should not be used as a basis. Implementing independently from published concepts, such as encoder space-speed decisions and multi-stream literals, is a different and lower-risk matter, though patents were not checked.

### Gaps
- No patent review was done for RAD, Meta, or Microsoft work. No official statement on Oodle's format licensing was read.
- OpenZL's licence was not checked.

## Benchmark sources and methodology pitfalls

### Takeaway
lzbench on Silesia is the right common yardstick, but results shift by 30% or more across CPUs and builds. Any Crush claim must run all codecs in one harness on one machine, in the same mode.

### Cited Findings
- lzbench single-thread runs use `-eALL -t8,8 -o1c4` with `performance` governor and turbo off. Multi-threaded runs use 4 MB blocks, pinned with taskset, and 51 blocks limit T32 to about 25x. A memcpy row gives the bandwidth ceiling — [lzbench](https://github.com/inikep/lzbench).
- lzbench's own pages disagree on versions: tables use lzav 5.17, zxc 0.14.1, mbrotli 0.5.2, while the list shows lzav 5.20, zxc 0.15.0, mbrotli 0.5.15 — [lzbench](https://github.com/inikep/lzbench). ZXC -3 is 46.09% at 0.14.1 and 46.65% at 0.15.0 — [table](https://raw.githubusercontent.com/inikep/lzbench/master/doc/lzbench24_sorted.md); [ZXC](https://github.com/hellobertrand/zxc).
- Same codec, different setups: zstd -1 decode is 1351 MB/s on EPYC 9555P with turbo off — [lzbench table](https://raw.githubusercontent.com/inikep/lzbench/master/doc/lzbench24_sorted.md); 1550 on i7-9700K @ 4.9 GHz — [zstd](https://github.com/facebook/zstd); 1898 on EPYC 9B45 — [ZXC](https://github.com/hellobertrand/zxc). Snappy decode is 976 MB/s, 1500 MB/s, and 2440-3360 MB/s across [lzbench](https://raw.githubusercontent.com/inikep/lzbench/master/doc/lzbench24_sorted.md), [zstd](https://github.com/facebook/zstd), and [LZAV](https://github.com/avaneev/lzav) tables.
- Bloom's Silesia totals sum per-file time and size rather than using a tar, and the compiler moves zstd about 25% — [Charles Bloom](https://cbloomrants.blogspot.com/2016/07/introducing-oodle-mermaid-and-selkie.html).
- Native vs dispatched builds are not equivalent: zlib-rs's `target-cpu=native` comparison was admitted to be unfair — [Trifecta](https://trifectatech.org/blog/zlib-rs-is-faster-than-c/). ZXC's table is built with `-march=native` — [ZXC](https://github.com/hellobertrand/zxc).
- Authors choose baselines: LZAV excludes zstd from its main tables and yet claims to beat zstd -1 on ratio — [LZAV](https://github.com/avaneev/lzav).
- Benchmarks should call the release binary directly, since `cargo run` adds significant overhead — [Trifecta](https://trifectatech.org/blog/current-zlib-rs-performance/).

### Inferences
- A fair race for Crush: lzbench-style in-memory, single-threaded, one pinned core, fixed frequency, identical toolchain and flags for every codec, plus a separate 4 MB-block multi-thread table. Report memcpy, build flags, and exact versions.
- Include enwik, a binary corpus, and a small-block workload in addition to Silesia, since LZAV's per-dataset results and OpenZL show a single corpus can flatter one design.
- Report runtime-dispatched builds, because `-march=native` flatters results compared with what users receive.
- Compare with zstd -1 and -3 from the same run. Note lzbench's tables omit -3 in single-threaded mode.

### Gaps
- TurboBench and Squash were not read directly. TurboBench appears only as the tool behind LZAV's zstd row, and Squash's currency in 2026 is unknown.
- Cache effects of in-memory vs I/O-backed benchmarks: no source here quantified them.
- encode.su threads were not read.
