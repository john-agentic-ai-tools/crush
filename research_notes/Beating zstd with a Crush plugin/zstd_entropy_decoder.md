# zstd entropy stage and decoder design: what to port or beat in Rust

Scope note: code citations are to the `dev` branch of facebook/zstd as fetched 2026-10-31; the [CHANGELOG](https://raw.githubusercontent.com/facebook/zstd/dev/CHANGELOG) lists v1.6.0 (Dec 2025) as latest. Fetches were summarised by a reader model over the raw files, so treat quoted identifiers as reliable and re-check any exact line before porting.
No first-party measurement was made. A zstd build plus Silesia run in WSL (to get `--no-compress-literals` numbers and a level 1-3 perf profile) was blocked by the permission system, because it would have run externally downloaded code. All ratio and time-share evidence below is therefore published or derived from published figures.

## Q1. How Huff0 works (literals section, 4 streams, X1/X2, BMI2, assembly)

### Takeaway
Huff0 gets speed from data layout: four independent backward bitstreams let one core run four decode chains at once, with 20 symbols per loop iteration. The assembly gain has largely been captured in C since 2023, so a Rust port does not need `.S` files to match it.

### Cited Findings
- Literals types (2-bit field): Raw 0, RLE 1, Compressed 2 (Huffman with tree description), Treeless 3 (reuses the previous tree or a dictionary's) — [RFC 8878](https://www.rfc-editor.org/rfc/rfc8878.txt)
- Compressed/Treeless Size_Format: 00 = one stream, 10-bit sizes, 3-byte header; 01 = four streams, 10-bit, 3 bytes; 10 = four streams, 14-bit, 4 bytes; 11 = four streams, 18-bit, 5 bytes — [RFC 8878](https://www.rfc-editor.org/rfc/rfc8878.txt)
- Jump table: 6 bytes, three LE 16-bit compressed sizes for streams 1-3; stream 4 is the remainder. Each stream regenerates (Regenerated_Size+3)/4 bytes, the last up to 3 fewer — [RFC 8878](https://www.rfc-editor.org/rfc/rfc8878.txt)
- Max Huffman code length is 11 bits. Tree header byte <128 means FSE-compressed weights (max accuracy log 6, two interleaved states); >=128 means direct 4-bit weights. Code length = Max_Number_of_Bits + 1 - Weight; the last weight is inferred. Streams are read backward from the end; a final-bit flag sits in the last byte — [RFC 8878](https://www.rfc-editor.org/rfc/rfc8878.txt)
- `HUF_TABLELOG_MAX` is 12 ("max runtime value of tableLog"), `HUF_TABLELOG_DEFAULT` 11, `HUF_SYMBOLVALUE_MAX` 255, `HUF_BLOCKSIZE_MAX` 128 KB — [huf.h](https://raw.githubusercontent.com/facebook/zstd/dev/lib/common/huf.h)
- X1: `HUF_DEltX1 {nbBits, byte}`, one symbol per lookup. X2: 4-byte `HUF_DEltX2 {U16 sequence; nbBits; length}`, up to two symbols per lookup — [huf_decompress.c](https://raw.githubusercontent.com/facebook/zstd/dev/lib/decompress/huf_decompress.c)
- `HUF_selectDecoder` quantises the ratio into Q (0-15) and estimates times from the `algoTime` table, e.g. `{{ 150,216}, { 381,119}}` for Q==2 (12-18%), `{{ 359,188}, {1167,109}}` for Q==8 (50-56%), `{{1412,185}, {1695,202}}` for Q==15 (93-99%). X2's estimate is bumped by `DTime1 += DTime1 >> 5`, "small advantage to algorithm using less memory, to reduce cache eviction" — [huf_decompress.c](https://raw.githubusercontent.com/facebook/zstd/dev/lib/decompress/huf_decompress.c)
- `HUF_readDTableX1_wksp` caps the table at `HUF_DECODER_FAST_TABLELOG` (11), writes four entries at a time through one U64 (multiply by `0x0001000100010001`), and switches on run length (1, 2, 4, 8, default) to a specialised loop — [huf_decompress.c](https://raw.githubusercontent.com/facebook/zstd/dev/lib/decompress/huf_decompress.c)
- Fast loop state is `HUF_DecompressFastArgs` (`ip[4]`, `op[4]`, `bits[4]`, `dt`, `ilowest`, `oend`, `iend[4]`). It needs little-endian, `dtLog == 11`, and each stream >= 8 bytes, otherwise it returns 0 and the fallback runs. The bound comment: "Each iteration consumes up to 11 bits * 5 = 55 bits < 7 bytes per stream." — [huf_decompress.c](https://raw.githubusercontent.com/facebook/zstd/dev/lib/decompress/huf_decompress.c)
- Assembly: `HUF_decompress4X1_usingDTable_internal_fast_asm_loop` and the 4X2 twin "Decode 5 symbols in each of the 4 streams (20 total)", using `shrxq`/`shlxq` (flag-free shifts) plus `bsfq` for refill. The refill is `bits[n] = MEM_readST(ip[n]) | 1`, a sentinel-bit scheme where the bit count is recovered by ctz. Twelve GPRs are live (`op0-3`, `ip0-3`, `bits0-3`) with `dtable`, `olimit`, and spills for `ip1-3`/`olimit` to the stack — [huf_decompress_amd64.S](https://raw.githubusercontent.com/facebook/zstd/dev/lib/decompress/huf_decompress_amd64.S)
- Assembly gains when introduced, C to asm, gcc-11 / clang-12 (units not stated on the page): proba32 +35% / +62%, proba128 +39% / +45%, silesia.tar +7.3% / +10.1%, x-ray +22.8% / +45.5% — [PR #2722](https://github.com/facebook/zstd/pull/2722)
- v1.5.4: Huffman decompression +20% on targets that cannot use x64 assembly; v1.5.1 introduced the x64 assembly decoder — [CHANGELOG](https://raw.githubusercontent.com/facebook/zstd/dev/CHANGELOG)
- After PR #3827 on `zstd -b1e1r github/`: fallback 477, fast C 492, asm 501 MB/s (fast C was 384 before), because the fast loop now runs until safe iterations hit 0. Block-size effect for fast C: +11.2% at 1K down to +0.2% at 128K — [PR #3827](https://github.com/facebook/zstd/pull/3827)
- Kernel note: gcc there was not unrolling the Huffman inner loops, "destroying" decoding performance, and the fix was hand-unrolling the 4-stream loops — [zstd kernel commit](https://fuchsia.googlesource.com/third_party/zstd/+/refs/tags/v1.5.5-kernel%5E%21/)
- Huff0 standalone on i7-5600U, 32 KB blocks: Proba80 600 enc / 1350 dec MB/s, Proba14 595 / 860, Proba02 525 / 555; zlib's Huffman 180-265 enc / 210-300 dec — [FiniteStateEntropy README](https://raw.githubusercontent.com/Cyan4973/FiniteStateEntropy/dev/README.md)

### Inferences
- An X1 table at log 11 is 2^11 entries of 2 bytes, about 4 KB. X2 at 4 bytes per entry is about 8 KB. Both fit L1, which helps explain why table size is the limit on tableLog (derived from the entry sizes above).
- RFC format max is 11 bits; `HUF_TABLELOG_MAX` 12 is an implementation allocation value, so a Crush format need not mirror it. Capping at 11 is what the fast loop assumes.
- For Rust: assembly's remaining margin over good C is about 2% on the github small-block case and appears mainly on compilers that fail to allocate 12+ live registers. `#[target_feature(enable = "bmi2")]` plus runtime dispatch is a plausible match for fast C, not proven; Rust codegen for this loop has not been measured here.
- PR #3827's small-block gains matter directly to Crush's block-oriented design: fast-loop exit behaviour dominates at 1-8 KB.

### Gaps
- Exact X1 vs X2 throughput on Silesia with current zstd: not found.
- `algoTime` units are implicit; no source defines them beyond the code.

## Q2. FSE/tANS in zstd, and why sequences use FSE while literals use Huffman

### Takeaway
FSE wins where a symbol's probability exceeds 0.5 and Huffman's 1-bit floor hurts, which describes sequence codes. Literals are flatter, so FSE gains roughly nothing there while decoding up to 3x slower and building tables more slowly.

### Cited Findings
- FSE "is based on a new theory called ANS", precomputes coding steps into tables, is "as precise as arithmetic coding", and uses "only additions, table lookups, and shifts" — [Meta engineering](https://engineering.fb.com/2016/08/31/core-infra/smaller-and-faster-data-compression-with-zstandard/)
- FSE vs Huff0 (i7-5600U, 32 KB blocks; ratio, enc, dec MB/s): Proba80 FSE 8.84, 325, 440 vs Huff0 6.38, 600, 1350. Proba14 FSE 1.91, 330, 460 vs Huff0 1.90, 595, 860. Proba02 FSE 1.13, 325, 445 vs Huff0 1.13, 525, 555 — [FiniteStateEntropy README](https://raw.githubusercontent.com/Cyan4973/FiniteStateEntropy/dev/README.md)
- Huffman "can't break the '1 bit per symbol' limit"; on proba90.bin FSE gave 15.21 vs zlib Huffman 7.175 — [Collet FSE post](http://fastcompression.blogspot.com/2013/12/finite-state-entropy-new-breed-of.html)
- Table cost: an FSE decoding table costs 1-16 KB by accuracy, a Huffman tree 32-512 bytes; "FSE requires the full table to be in memory". Fast spread is within about 0.01% of optimal without multiply or divide — [Collet FSE post, comments](http://fastcompression.blogspot.com/2013/12/finite-state-entropy-new-breed-of.html)
- "Huffman is definitely faster to build tables" than TANS and faster to decode transmitted code lengths. Bloom measured Huffman decode at 325-415 MB/s by method (ANS-style next-state table 415 was fastest) vs TANS 370 on the same file, excluding table build — [cbloom rants, Huffman Performance](http://cbloomrants.blogspot.com/2015/)
- Compression modes (per LL, OF, ML): 0 Predefined, 1 RLE, 2 FSE_Compressed, 3 Repeat. Max accuracy log 9 / 8 / 9 for LL / OF / ML; predefined logs 6 / 5 / 6 — [RFC 8878](https://www.rfc-editor.org/rfc/rfc8878.txt)
- `ZSTD_buildFSETable_body` has a fast path, when no -1 (low probability) symbols exist, that lays symbols 8 bytes at a time with `MEM_write64`, else a general spread with `highThreshold`. It has `_default` and `_bmi2` builds — [zstd_decompress_block.c](https://raw.githubusercontent.com/facebook/zstd/dev/lib/decompress/zstd_decompress_block.c)
- `ZSTD_buildSeqTable`: `set_rle` builds one entry; `set_basic` returns the default table; `set_repeat` reuses the previous one and prefetches it when `nbSeq > 24`; `set_compressed` reads NCount and builds — [zstd_decompress_block.c](https://raw.githubusercontent.com/facebook/zstd/dev/lib/decompress/zstd_decompress_block.c)
- Encoder `ZSTD_selectEncodingType`: below `ZSTD_lazy`, it picks `set_repeat` when valid and `nbSeq < 1000`, and `set_basic` when `nbSeq < dynamicFse_nbSeq_min` ("28-36 for offset, 56-72 for lengths"). At `ZSTD_lazy` and up it compares exact costs — [zstd_compress_sequences.c](https://raw.githubusercontent.com/facebook/zstd/dev/lib/compress/zstd_compress_sequences.c)

### Inferences
- With alphabets of 36 (LL), 53 (ML), and about 32 (OF) symbols and logs at most 9, sequence tables are 512 entries at most, so the build cost that hurts FSE over 256 literal symbols is much smaller here. Three such tables per block is still real work, which is why low levels lean on predefined and repeat modes.
- Proba14 shows FSE's literal gain at about 0.5% for 1.9x decode loss on flat-ish data. Skewed literals (RLE-like blocks) favour FSE, but zstd covers the extreme case with the RLE type.
- Literal data rate also differs: a block has far more literal bytes than sequences, so literal decode speed is the throughput-critical path while sequence precision is the ratio-critical one.

### Gaps
- Published microbenchmarks of FSE table build in ns per block: not found.
- The FSE README numbers are from a Broadwell-era mobile CPU and have not been refreshed. Note also that Huff0 has gained a lot since then (Q1), so the gap to FSE is likely wider now.

## Q3. Sequence section format

### Takeaway
Three FSE states and raw extra bits share one backward bitstream, with a fixed read order that makes decoding one straight-line, table-driven block per sequence.

### Cited Findings
- Number_of_Sequences takes 1-3 bytes; 0 means no sequences. The modes byte has LL in bits 7-6, OF in 5-4, ML in 3-2, reserved 1-0 — [RFC 8878](https://www.rfc-editor.org/rfc/rfc8878.txt)
- LL codes 0-35 (lengths to 131071), ML codes 0-52 (lengths 3-131074), OF codes 0-N (N>=22 recommended, reference decoder supports 31) — [RFC 8878](https://www.rfc-editor.org/rfc/rfc8878.txt)
- Bitstream read backward. Initial states read LL, OF, ML. Each sequence reads OF bits, ML bits, LL bits. States update LL, ML, OF, and the update is skipped after the last sequence — [RFC 8878](https://www.rfc-editor.org/rfc/rfc8878.txt)
- Repeat offsets start at 1, 4, 8; values 1-3 are repcodes that shift by one when LL is 0, so value 3 then means Repeated_Offset1 - 1 — [RFC 8878](https://www.rfc-editor.org/rfc/rfc8878.txt)
- `ZSTD_encodeSequences_body` works from the last sequence to the first with intentional index underflow, encoding OF, ML, LL symbols then extra bits. On 64-bit it flushes only when `ofBits+mlBits+llBits >= 64-7-(LLFSELog+MLFSELog+OffFSELog)` before literal bits, and when the sum is `> 56` before offset bits — [zstd_compress_sequences.c](https://raw.githubusercontent.com/facebook/zstd/dev/lib/compress/zstd_compress_sequences.c)
- Repcode modeling gives "a single to a double-digit compression improvement" depending on the data — [Meta engineering](https://engineering.fb.com/2016/08/31/core-infra/smaller-and-faster-data-compression-with-zstandard/)

### Inferences
- The states are not "interleaved streams" in the 4-stream Huffman sense, since all share one bit reader. Their parallelism is three independent table loads per sequence, not independent bit reads. The serial bit position limits speedup, so SIMD gives little here.
- Each FSE state directly indexes its table, so the next symbol needs no bit read. Meta describes this as reducing "latency to access the next symbol, as it is immediately accessible from the state value".

### Gaps
- Average bits per sequence and bytes per sequence on Silesia at levels 1-3: not found.

## Q4. How the entropy encoder stays cheap at levels 1-3

### Takeaway
zstd keeps fast levels cheap by not building what it can reuse: predefined or repeat FSE tables, sampling before histograms, and no cost modelling below `ZSTD_lazy`. Entropy's share of time at levels 1-3 is unpublished; the best derived bound puts Huffman at about 6% at level 1.

### Cited Findings
Literals (`ZSTD_compressLiterals`):
- `singleStream = srcSize < 256`, header 3-5 bytes. `disableLiteralCompression` goes straight to raw. Below `ZSTD_minLiteralsToCompress` it stores raw ("if too small, don't even attempt compression (speed opt)"): 6 bytes with a valid repeat table, else `8 << MIN(9-strategy, 3)`, i.e. 64 bytes at strategy 6 or below — [zstd_compress_literals.c](https://raw.githubusercontent.com/facebook/zstd/dev/lib/compress/zstd_compress_literals.c)
- `HUF_flags_preferRepeat` is set when `strategy < ZSTD_lazy && srcSize <= 1024`; `HUF_flags_optimalDepth` when `strategy >= HUF_OPTIMAL_DEPTH_THRESHOLD`. A repeat table yields `set_repeat` (treeless) — [zstd_compress_literals.c](https://raw.githubusercontent.com/facebook/zstd/dev/lib/compress/zstd_compress_literals.c)
- Output is discarded for raw if `cLitSize >= srcSize - minGain`, where `ZSTD_minGain` is `(srcSize >> minlog) + 2` and `minlog` is 6 below `ZSTD_btultra` — [zstd_compress_internal.h](https://raw.githubusercontent.com/facebook/zstd/dev/lib/compress/zstd_compress_internal.h)
- `ZSTD_literalsCompressionIsDisabled` in auto mode returns `(strategy == ZSTD_fast) && (targetLength > 0)` — [zstd_compress_internal.h](https://raw.githubusercontent.com/facebook/zstd/dev/lib/compress/zstd_compress_internal.h)
- Negative levels use row 0 ("baseline for fast mode") and set `cp.targetLength = -level` — [zstd_compress.c](https://raw.githubusercontent.com/facebook/zstd/dev/lib/compress/zstd_compress.c)
- `suspectUncompressible = (numSequences == 0) || (litSize / numSequences >= SUSPECT_UNCOMPRESSIBLE_LITERAL_RATIO)`, ratio 20 — [zstd_compress.c](https://raw.githubusercontent.com/facebook/zstd/dev/lib/compress/zstd_compress.c)

Huffman (`HUF_compress_internal`):
- If suspect and input is at least `SUSPECT_INCOMPRESSIBLE_SAMPLE_SIZE * SUSPECT_INCOMPRESSIBLE_SAMPLE_RATIO`, it histograms a head and tail sample and bails when the max count is `<= ((2 * SAMPLE_SIZE) >> 7)+4` ("probably not compressible enough"). The full-histogram check is `largest <= (srcSize >> 7)+4`; a single symbol gives RLE — [huf_compress.c](https://raw.githubusercontent.com/facebook/zstd/dev/lib/compress/huf_compress.c)
- Repeat: with `preferRepeat` and a valid table, it reuses before sampling. Otherwise, after building, it keeps the old table if `oldSize <= hSize + newSize` or `hSize + 12 >= srcSize` — [huf_compress.c](https://raw.githubusercontent.com/facebook/zstd/dev/lib/compress/huf_compress.c)
- Build: without `optimalDepth`, `HUF_optimalTableLog` uses a "cheap evaluation, based on FSE". `HUF_buildCTable_wksp` does a count-bucket `HUF_sort`, unlimited `HUF_buildTree`, then `HUF_setMaxHeight` clamps depth and repays Kraft excess by lengthening cheap symbols — [huf_compress.c](https://raw.githubusercontent.com/facebook/zstd/dev/lib/compress/huf_compress.c)
- Emit: `HUF_CStream_t` keeps two accumulators; stream 1 fills independently and merges into stream 0 via `HUF_mergeIndex1`, removing data dependencies. Unroll depth depends on tableLog and word size. `HUF_tightCompressBound` skips bounds checks when the output is large enough — [huf_compress.c](https://raw.githubusercontent.com/facebook/zstd/dev/lib/compress/huf_compress.c)

Sequences and blocks:
- Low-strategy table choice avoids cost functions entirely (see Q2). High strategies compute `ZSTD_fseBitCost`, `ZSTD_crossEntropyCost`, and `(NCountCost << 3) + ZSTD_entropyCost` — [zstd_compress_sequences.c](https://raw.githubusercontent.com/facebook/zstd/dev/lib/compress/zstd_compress_sequences.c)
- Block RLE checks use `rleMaxLength = 25`; `ZSTD_maybeRLE` is `nbSeqs < 4 && nbLits < 10`. Blocks whose output reaches `blockSize - ZSTD_minGain(...)` are stored raw. The pre-splitter `ZSTD_optimalBlockSize` uses `splitLevels[] = {0,0,1,2,2,3,3,4,4,4}` by strategy and skips incompressible data (`savings < 3`) — [zstd_compress.c](https://raw.githubusercontent.com/facebook/zstd/dev/lib/compress/zstd_compress.c)
- Whole-codec speeds, i7-9700K, Silesia: zstd 1.5.7 -1 510 MB/s, `--fast=1` 545 MB/s — [zstd README](https://raw.githubusercontent.com/facebook/zstd/dev/README.md)

### Inferences
- Combining the two literal-compression facts: `--fast=N` means Huffman off, FSE on. The difference between `--fast=1` and `-1` is therefore mostly the Huffman stage, though the two use different parameter rows.
- Time bound: 1/510 - 1/545 = 0.126 ns/B of 1.96 ns/B, so about 6.4% of level 1 time. This is a rough estimate of the literal Huffman stage, and the different match-finder parameters mean it could be off in either direction.
- Direct code implication for Crush: the cheap tricks (6-byte repeat threshold, 1 KB prefer-repeat, sampling, literals-per-sequence suspicion) are format-independent and portable, and require no patent or format commitment.
- The prompt's `isIncompressible` is not the identifier; zstd's are `suspectUncompressible` and `HUF_flags_suspectUncompressible`.

### Gaps
- Directly measured entropy vs match-finding share at levels 1-3: no published profile found, and the local measurement was blocked. A `perf record` of `zstd -b1`/`-b3` on Silesia would close this in minutes.
- `SUSPECT_INCOMPRESSIBLE_SAMPLE_SIZE` and `_RATIO` numeric values were not captured.

## Q5. Decoder loop, and why decompression speed is roughly level-independent

### Takeaway
Decode cost scales with sequence and literal counts rather than with encoder effort, so the fast path is one predictable loop of table loads and over-copies. Most of the engineering is in reducing branches and cache misses around it.

### Cited Findings
Loop variants, all in [zstd_decompress_block.c](https://raw.githubusercontent.com/facebook/zstd/dev/lib/decompress/zstd_decompress_block.c):
- `ZSTD_decompressSequences_body`: default loop of `ZSTD_decodeSequence` then `ZSTD_execSequence`
- `ZSTD_decompressSequences_bodySplitLitBuffer`: literals split between dst tail and `litExtraBuffer`, with `ZSTD_execSequenceSplitLitBuffer` until the switch, then `ZSTD_execSequence`
- `ZSTD_decompressSequencesLong_body`: queue of `STORED_SEQS` (8) decoded sequences; `ZSTD_prefetchMatch` issues `PREFETCH_L1` on match and match + cacheline, and execution runs 8 positions behind decode
- Dispatcher: `usePrefetchDecoder` starts as `ddictIsCold`. If `totalHistorySize > (1u << 24)` and `nbSeq > 8`, `ZSTD_getOffsetInfo` estimates the share of long offsets (`nbAdditionalBits > 22`) and enables prefetch at `minShare = MEM_64bits() ? 7 : 20` ("correspond to 2.73% and 7.81%")

Copy paths, same file:
- `ZSTD_execSequence`: `ZSTD_copy16` for literals, wildcopy only when longer. Comment: "Split out litLength <= 16 since it is nearly always true. +1.6% on gcc-9."
- Offsets `>= WILDCOPY_VECLEN` use non-overlap wildcopy. Close offsets use `ZSTD_overlapCopy8` with `dec32table`/`dec64table`. Near the end, `FORCE_NOINLINE ZSTD_execSequenceEnd` uses `ZSTD_safecopy`.
- Literal buffer locations: `ZSTD_in_dst`, `ZSTD_not_in_dst`, `ZSTD_split`. Raw literals are referenced in place in the source when safe, avoiding a copy.
- `ZSTD_decodeSequence` has an `__aarch64__` path keeping offsets in locals with `ZSTD_memcpy` loads (GCC-only), plus a generic path.

Alignment hacks and dispatch:
- `__asm__` with `.p2align` and nops is applied per GCC version. The comment reads: "zstd compiled with gcc-9 on an Intel i9-9900k shows 10% decompression speed swings based on the alignment of the decompression loop." Another states it is "good for gcc-7, gcc-9, and gcc-11" — [zstd_decompress_block.c](https://raw.githubusercontent.com/facebook/zstd/dev/lib/decompress/zstd_decompress_block.c)
- BMI2: under `DYNAMIC_BMI2`, `_bmi2` functions carry `BMI2_TARGET_ATTRIBUTE` and are picked by `ZSTD_USE_BMI2`, otherwise macro-aliased to `_default`; `HUF_DGEN` does the same — [zstd_decompress_block.c](https://raw.githubusercontent.com/facebook/zstd/dev/lib/decompress/zstd_decompress_block.c), [huf_decompress.c](https://raw.githubusercontent.com/facebook/zstd/dev/lib/decompress/huf_decompress.c)
- "Zstandard has been created to be friendly to branchless algorithms"; design uses "few or no data dependencies", "separating data into multiple parallel streams" — [Meta engineering](https://engineering.fb.com/2016/08/31/core-infra/smaller-and-faster-data-compression-with-zstandard/)

Throughput and history:
- i7-9700K, Silesia, gcc 14.2: zstd -1 1550 MB/s, `--fast=1` 1850, `--fast=4` 2050, lz4 3850, snappy 1500, zlib -1 390, brotli -1 425 — [zstd README](https://raw.githubusercontent.com/facebook/zstd/dev/README.md)
- Single core EPYC 9554 @ 3.1 GHz, turbo off: zstd 1.5.6 -1 1342 MB/s, lz4 1.10.0 3728 MB/s — [lizard README](https://raw.githubusercontent.com/inikep/lizard/lizard/README.md)
- 2016: "approximately 550 MB/s for Zstandard and 270 MB/s for zlib" on Silesia, "regardless of compression ratio" — [Meta engineering](https://engineering.fb.com/2016/08/31/core-infra/smaller-and-faster-data-compression-with-zstandard/)
- Decompress gains by version: v1.3.8 +7% large files, +15% cold dictionaries; v1.4.1 ~7%; v1.4.4 >10%; v1.4.5 x64 +10% clang, +5% gcc, ARM +15-50%; v1.4.6 small block 20%+ — [CHANGELOG](https://raw.githubusercontent.com/facebook/zstd/dev/CHANGELOG)

### Inferences
- Level independence: higher levels find longer matches (fewer sequences per byte) while the per-sequence and per-literal costs are unchanged, so throughput does not fall with encoder effort. Large windows are the exception, which is why the Long variant exists at the 16 MB history threshold.
- 2016 550 MB/s to 2025 1550 MB/s spans newer hardware and software alike, so about 2.8x cannot be attributed to code alone.
- zstd's speed gap to LZ4 (2.5x at level 1) is the entropy budget. A Crush design with cheaper literal decode has room mostly there.
- Alignment sensitivity: zstd's 10% swings mirror the existing Crush finding that `fast_lz_select` slowdown is binary placement. Benchmarks comparing a Crush plugin to zstd need several builds or layout randomisation, or a sub-10% result is noise.

### Gaps
- Decompression speed by level from a single current source (level 1 to 19) was not extracted; README charts are images.
- Share of decode time in Huffman vs sequence decode vs copy: no published breakdown found.

## Q6. Newer entropy research: SIMD rANS, Oodle, 4X vs 8X Huffman

### Takeaway
Published data shows AVX2 rANS at about 2x a 4-way scalar decoder and roughly at Huff0's level, not a generational jump, and it depends on gather or a scalar substitute. No credible AVX-512 number or 8X Huffman benchmark was found, so any claim of gains there needs Crush's own measurement.

### Cited Findings
- Giesen's paper describes an SSE 4.1 rANS decoder on an 8-way interleaved stream, entirely free of branches, with performance almost independent of the data — [Interleaved entropy coders](https://arxiv.org/pdf/1402.3392)
- Bonfield `rans_static`, i5-4570 (Haswell), gcc 6.2, q8 data, order 0, enc / dec MB/s: rANS_static4x8 300 / 681; rANS_static4x16 369 / 665; r8x16_sse 352 / 577; r16x16b_avx2 289 / 982; r32x16_avx2 291 / 904; r32x16b_avx2 535 / 1431; scalar r32x16 291 / 380. No AVX-512 rows, since that CPU lacks it — [rans_static README](https://raw.githubusercontent.com/jkbonfield/rans_static/master/README.md)
- Intel's GDS (Downfall) microcode fix "can slow AVX2 decode and AVX512 encode by 2-3x", and emulating gathers now beats hardware gathers; without the patch, simulated gathers cost 10-30% on decode, up to 50% on AVX-512 encode — [htscodecs README](https://codegraph.jelmer.uk/htscodecs/1.6.6-1/README.md)
- htscodecs 1.6.0 fixed bounds checking in `rans_uncompress_O0_32x16_avx2` (buffer read overruns) — [htscodecs changes](https://src.opensuse.org/pool/htscodecs/src/branch/factory/htscodecs.changes)
- Two interleaved rANS streams gave about 1.4x; rANS is a near drop-in for multi-symbol arithmetic coders, with a "division-free" decoder; it suits fast-changing probabilities, where Huffman and tANS need rebuilding. LZNA uses a 63-bit state with 32-bit renorm; BitKnit a 32-bit state with 16-bit renorm and 14-bit precision — [Giesen, rANS in practice](https://fgiesen.wordpress.com/2015/12/21/rans-in-practice/)
- Oodle benchmarks by the author: Kraken 4.05:1 at 919.8 MB/s decode vs zlib 306.9, lzma 78.8, on a single-threaded Core i7-3770 — [Release the Kraken](http://cbloomrants.blogspot.com/2016/04/release-kraken.html). On one 2020 texture set, roughly 1000 (Leviathan), 1500-1700 (Kraken), 3300 (Mermaid), 7000-8000 MB/s (Selkie) — [cbloom 2020](http://cbloomrants.blogspot.com/2020/)
- Interleaved ANS conflicts with minimal-flush, which needs an end marker per state, "a pain in the butt in practice" — [cbloom 2015](http://cbloomrants.blogspot.com/2015/)
- libdeflate (second-hand, from ports): 11-bit first-level litlen table, 8-bit distance, 7-bit precode, entries packed in u32, cold subtable path, preload-before-copy, `_bzhi_u64` for bit extraction — [linflate DESIGN](https://docs.rs/crate/linflate/0.1.0/source/DESIGN.md)

### Inferences
- r32x16b_avx2 at 1431 vs rANS_static4x8 at 681 is about 2.1x for decode on Haswell, the only rigorous "AVX2 vs scalar" pair found. Cross-CPU comparison with Huff0 (860-1350 MB/s on 2015 hardware, different data) suggests parity, with rANS adding sub-bit precision.
- Encoder gain is about 1.8x against the 4x8 coder (535 vs 300) and 1.45x against the 4x16 coder (535 vs 369).
- Giesen's 1.4x from 2 streams and Bonfield's flat 16-way to 32-way result for the plain variants (896 vs 904 MB/s) imply diminishing returns limited by refill and data movement more than by stream count. This is reasoning, not a measurement.
- 8X Huffman has per-stream costs: more jump-table bytes, a worse tail, and per-stream minimum block size. It competes with zstd's small-block fast-loop exit, which PR #3827 showed is worth +11% at 1 KB. Practical gains should favour blocks of at least 16 KB. With AVX-512 VBMI-style shifts this may be worthwhile, but it is unproven.
- Any Crush AVX2 path must avoid `vpgatherdd` on Intel, since post-Downfall performance is the one that users see.

### Gaps
- Numbers from Giesen's paper: the PDF text could not be extracted (no poppler here), so its measured figures are not reported.
- AVX-512 rANS throughput and 8X Huffman throughput: no primary figure found. `ryg_rans` README returned 404 at the tried path.
- Current Oodle literal coding internals are proprietary. Posts retrieved did not describe them, and the numbers are the author's own.
- Lemire's SIMD work was not found to contain anything specific to Huffman or ANS in these searches.

## Q7. Ratio gain from entropy coding on top of byte-oriented LZ

### Takeaway
On Silesia, entropy coding is worth about 14-17% smaller output at level-1-class parsing, and the full lz4-to-zstd -1 gap is about 27%. Lizard, the cleanest natural experiment, shows Huffman alone reduces output about 17% at a 32-45% decode speed cost.

### Cited Findings
- zstd README, i7-9700K, Silesia (ratio, comp, decomp MB/s): zstd 1.5.7 -1 2.896, 510, 1550; `--fast=1` 2.439, 545, 1850; `--fast=4` 2.146, 665, 2050; lz4 1.10.0 2.101, 675, 3850; snappy 2.089, 520, 1500; zlib -1 2.743, 105, 390 — [zstd README](https://raw.githubusercontent.com/facebook/zstd/dev/README.md)
- Lizard, EPYC 9554, silesia.tar (comp, decomp MB/s, compressed %): lz4 579, 3728, 47.60; lizard -10 484, 2161, 48.79; -20 388, 1674, 45.73; -30 363, 1185, 40.45; -40 297, 1141, 38.14; zstd 1.5.6 -1 421, 1342, 34.64. Levels: -10s fastLZ4, -20s LIZv1, -30s fastLZ4 + Huffman, -40s LIZv1 + Huffman — [lizard README](https://raw.githubusercontent.com/inikep/lizard/lizard/README.md)
- That README also calls Lizard "outdated and effectively obsolete" given 2025 benchmarks — [lizard README](https://raw.githubusercontent.com/inikep/lizard/lizard/README.md)
- A zstd commit documents a mode forcing literals-only compression to measure Huffman, with ratio 1.49 on enwik7 at level 1 — [huf.h commit history](https://git.sharpetronics.com/sharpetronics/zstd/commits/commit/cc3e3acd3415cfb14aad5fbe4e819fcf20760e8f/lib/common/huf.h)
- Repcodes contribute "a single to a double-digit compression improvement" — [Meta engineering](https://engineering.fb.com/2016/08/31/core-infra/smaller-and-faster-data-compression-with-zstandard/)
- `--fast` disables literal Huffman by default: see Q4 for `ZSTD_literalsCompressionIsDisabled` and the `targetLength` mapping.

### Inferences
Derived decomposition on Silesia, all from the published figures above:

| Step | Ratio | Output change | Decode cost |
|---|---|---|---|
| lz4 to zstd `--fast=1` (FSE sequences, repcodes, no literal Huffman) | 2.101 to 2.439 | -13.9% | 3850 to 1850 MB/s, -52% |
| `--fast=1` to `-1` (literal Huffman, mostly) | 2.439 to 2.896 | -15.8% | 1850 to 1550 MB/s, -16% |
| Total lz4 to zstd -1 | 2.101 to 2.896 | -27.4% | -60% |
| lizard -10 to -30 (Huffman on fastLZ4) | 48.79% to 40.45% | -17.1% | 2161 to 1185, -45% |
| lizard -20 to -40 (Huffman on LIZv1) | 45.73% to 38.14% | -16.6% | 1674 to 1141, -32% |

- Huffman contributes about 16-17% consistently. The first zstd row is not "pure entropy": it also includes repcodes and a different match finder and block format, and it is not like-for-like since `--fast=1` and `-1` use different parameter rows. Treat these as +/- a few points.
- The most striking point: moving literal Huffman on costs 16% of zstd decode speed, while sequence coding costs 52%. So the sequence stage (bit-level FSE decoding, not literals) is what separates zstd from LZ4 decode speed. A byte-aligned or SIMD-friendly sequence layout that keeps Huffman literals, as Lizard's -30s do, is the most promising angle for "beat zstd on decode". Caveat: part of that 52% is shorter matches and more literal/match transitions, not entropy alone, and Lizard's slower Huffman decode shows implementation quality matters.
- Corpus dependence: Silesia is mixed. Text and binary with high-entropy literals see a different balance, so test on Crush's own corpus.

### Gaps
- `--no-compress-literals` numbers at equal parameters: not found published; the benchmark that would give them was blocked. This is the most valuable missing number.
- No direct LZ4 + Huffman-only prototype on current hardware was found. Lizard's numbers are from its own format, which is not LZ4.
- Ratio contribution of FSE for sequences alone vs a byte-aligned encoding at identical parse: not found.

## Q8. ANS patent considerations

### Takeaway
Base ANS and tANS/FSE are not what US 11234023 targets on its description, but its claims were not read, it covers rANS features, and it runs to about 2039. Prefer Huffman plus tANS, and take counsel before shipping rANS. This is not legal advice.

### Cited Findings
- US11234023B2, "Features of range asymmetric number system encoding and decoding", Microsoft Technology Licensing LLC. Filed and priority 2019-06-28, granted 2022-01-25. Google Patents lists status Active and anticipated expiration 2039-06-28, both stated as assumptions, not legal conclusions. Continuation US17/552,295 exists, as do CN and EP family filings and PCT/US2020/032397. The page flags family litigation via a Darts-ip link with no case details — [Google Patents](https://patents.google.com/patent/US11234023B2/en)
- Described features: selective decoder state flush between fragments via a per-fragment flag; adjustable symbol width (0 to -3 bits, 2-bit syntax element); per-fragment switching among static probability models (16 in the example, 4-bit value); a two-phase decode structure suited to hardware — [Google Patents](https://patents.google.com/patent/US11234023B2/en)
- Patent "covers modifications" to rANS, granted January 2022 — [The Register 2022](https://www.theregister.com/2022/02/17/microsoft_ans_patent/)
- Duda says the rANS variant is used in JPEG XL. Editor Jon Sneyers disagreed, saying Microsoft has made no ISO declaration affecting JPEG XL, and the only declarations are royalty-free ones from Google and Cloudinary — [The Register 2022](https://www.theregister.com/2022/02/17/microsoft_ans_patent/)
- Google's ANS video application drew a non-final rejection of all claims in 2018, citing Duda's work with a Giesen article and an older decoder patent — [EFF](https://www.eff.org/deeplinks/2018/08/after-patent-office-rejection-it-time-google-abandon-its-attempt-patent-use-public). Later reports say Google ceased its attempts — [Science in Poland](https://scienceinpoland.pl/en/news/news,80578,google-ceases-attempts-patent-jagiellonian-university-scientists-invention.html)
- No USPTO reexamination, IPR, or court challenge was found in searches; the patent's challenge history remains unclear — [Wikipedia on Duda](https://en.wikipedia.org/wiki/Jaros%C5%82aw_Duda_(computer_scientist)), [ESP wiki](https://wiki.endsoftwarepatents.org/wiki/ANS)

### Inferences
- A Crush design with per-block static rANS tables and model switching resembles the described "fragment" features. That proximity deserves counsel review if rANS is used.
- FSE/tANS was public by 2013 (Collet's post) and predates the 2019 filing, so it is the conservative choice, but this is a judgement, not a clearance.

### Gaps
- Exact claim language: the claims were not in the fetched portion (the page is 267 KB; the 0-200 KB range was read). Claim 1 must be read before any rANS decision.
- Current maintenance fee and legal status: only Google Patents' assumed status was found. USPTO Patent Center should be checked.
- Any Microsoft royalty-free licensing statement: none found.
