# Zstandard compressor internals: negative levels and levels 1-5

**Version basis.** Latest published release: v1.5.7, 2025-02-19 ([release list](https://github.com/facebook/zstd/releases)). Code below was read directly from `dev` at commit `49cf5179` (2026-10-05); line numbers refer to that commit. `lib/compress/zstd_fast.c`, `zstd_double_fast.c`, `clevels.h` and `zstd_preSplit.c` show **zero diff** between tag v1.5.7 and that commit (local `git diff --stat`), so everything said about those files applies equally to v1.5.7.

**Version trap.** The `dev` CHANGELOG has a "v1.6.0 (Dec 2025)" entry and `zstd.h` defines 1.6.0, but no v1.6.0 tag or GitHub release exists as of this research: `gh release list` and the tag list stop at v1.5.7 / v1.5.7-kernel ([CHANGELOG](https://github.com/facebook/zstd/blob/49cf5179/CHANGELOG), [zstd.h](https://github.com/facebook/zstd/blob/49cf5179/lib/zstd.h)). Treat 1.6.0 as unreleased. Its only listed change is legacy format support off by default. The GitHub API now returns `meta/zstd` as the raw-content host for this repo, so the project may have moved orgs; the `facebook/zstd` URLs still resolved.

## What exactly do ZSTD_fast and ZSTD_dfast do?

### Takeaway
`ZSTD_fast` is a single-entry, unchained hash table with a 4-cursor software pipeline, adaptive skip, and a repcode probe. `ZSTD_dfast` adds a second table with an 8-byte hash and a longer-match check at ip+1. Both are scalar: the speed comes from load-latency scheduling, branch-misprediction control, and small cache-resident tables, not from SIMD.

### Cited Findings

**ZSTD_fast: table and hash**
- One `U32` table of `1<<hashLog` entries holding indices (`base + idx`), with no chain table; the chain table is allocated only for `strategy != ZSTD_fast` without row matching — [zstd_compress.c:283](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_compress.c#L283), [ZSTD_sizeof_matchState](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_compress.c)
- The hash is multiplicative. For 4 bytes: `(u * 2654435761U) >> (32-h)`. There are separate primes for 5-8 byte hashes (`prime5bytes`...`prime8bytes`, with `prime8bytes = 0xCF1BBCDCB7A56463`). `minMatch` selects a compile-time-specialized instance for mls 4, 5, 6, 7; mls 3 falls into the 4 case — [zstd_compress_internal.h:923-948](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_compress_internal.h#L923), [zstd_fast.c](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_fast.c)
- Confirmation is by a 4-byte compare (`MEM_read32`) regardless of mls, then `mLength = 4` — [zstd_fast.c](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_fast.c)
- Eight specializations exist: 4 mls values × `useCmov` 0/1. `useCmov = windowLog < 19`. CDict tables, not CCtx tables, carry 8-bit short-cache tags (`ZSTD_SHORT_CACHE_TAG_BITS 8`) — [zstd_fast.c](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_fast.c), [zstd_compress_internal.h:1507](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_compress_internal.h#L1507)

**ZSTD_fast: pipeline (PR #2749, Felix Handte, merged 2021-09-09, shipped v1.5.1)**
- Each search is 4 latency-chained stages: Hash, Lookup, Load, Compare. The pipeline interleaves them across adjacent positions using 4 cursors `ip0..ip3`, where `ip1 = ip0+1`, `ip2 = ip0+step`, `ip3 = ip2+1`. The loop body is manually unrolled to two positions per iteration — [zstd_fast.c block comment](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_fast.c), [PR #2749](https://github.com/facebook/zstd/pull/2749)
- One iteration does: read `rval` at `ip2 - rep_offset1`; write `hashTable[hash0]`; repcode test at ip2; candidate test at ip0; lookup for ip1; hash ip2; advance; write back; candidate test; lookup; hash; advance by step — [zstd_fast.c](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_fast.c)
- A found match flushes the pipeline (like a mispredict in a CPU); re-entry needs H T / H work — [PR #2749](https://github.com/facebook/zstd/pull/2749)
- History: the original was sequential; #1562 (Terrell) processed two positions at once; #2749 added this pipeline; #2921 restored "stagger" so that `step` applies between pairs of positions, not every search, after #2749 made negative levels too step-sensitive — [PR #2749](https://github.com/facebook/zstd/pull/2749), [PR #2921](https://github.com/facebook/zstd/pull/2921)

**ZSTD_fast: step and skip**
- `stepSize = targetLength + !targetLength + 1`, minimum 2. `kStepIncr = 1 << (kSearchStrength-1) = 128` with `kSearchStrength = 8`: every 128 bytes without a match, `step++`. That checkpoint also issues `PREFETCH_L1(ip1+64)` and `PREFETCH_L1(ip1+128)`. These prefetch **input**, not the hash table — [zstd_fast.c](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_fast.c), [zstd_compress_internal.h:32](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_compress_internal.h#L32)
- Skip is applied between pairs: `ip0/ip1` stay adjacent and `step` is the gap from ip0 to ip2 — [zstd_fast.c comment](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_fast.c)
- Skip payoff on incompressible input: level 3 reaches ~3500 MB/s on a 10 MB incompressible sample (i7-9700K, gcc-9) — [v1.5.5 notes](https://github.com/facebook/zstd/releases/tag/v1.5.5)

**ZSTD_fast: branch handling**
- PR #4144 (Ilya Tokar): replace `if (idx < limit && x[idx]==val)` with a conditional-move address selection, comparing unconditionally against a dummy on invalid index. Measured about -9% to -16% ns/byte on fleetbench for levels -7..0 at window logs 15-16. This PR shows no GitHub merge record; the change landed as commit `e8fce389` — [PR #4144](https://github.com/facebook/zstd/pull/4144)
- `ZSTD_selectAddr` forces this with inline `cmp`/`cmova` on GCC/Clang x86-64, and is a plain ternary elsewhere — [zstd_compress_internal.h:652](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_compress_internal.h#L652)
- PR #4165 (Collet): the cmov regresses long inputs by 3-6%, so the variant is chosen at runtime by `windowLog < 19`. silesia.tar in 32 KB blocks, i7-9700K: gcc-9 280 → 343 MB/s. Clang-16 initially lost the gain (271 MB/s), which the PR says it then worked around — [PR #4165](https://github.com/facebook/zstd/pull/4165)
- The `__asm__("")` statement inside `ZSTD_match4Found_cmov` is a compiler barrier that forces test ordering after inlining — [zstd_fast.c](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_fast.c)

**ZSTD_fast: match emission**
- Backward extension is a byte loop: `while ((ip0>anchor) & (match0>prefixStart) && ip0[-1]==match0[-1])`. A repcode hit checks only one byte backward. Forward uses `ZSTD_count` — [zstd_fast.c](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_fast.c)
- After a match, exactly two "complementary" insertions: `current0+2` and `ip0-2`. Positions inside the match are otherwise not inserted. The already-computed `hash1` is also written back, guarded by `step <= 4` on the second candidate test — [zstd_fast.c](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_fast.c)
- Then an immediate check for `rep_offset2` at the new position, emitted with litLength 0 — [zstd_fast.c](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_fast.c)
- The trailing positions when `ip3 >= ilimit` are deliberately unsearched: "it seems to be a meaningful performance hit to try to search them." `HASH_READ_SIZE = 8` keeps reads in bounds — [zstd_fast.c](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_fast.c)

**ZSTD_dfast**
- Two tables: `hashTable` is the **long** table (`hashLog` bits, always 8-byte hash); `chainTable` is reused as the **short** table (`chainLog` bits, `minMatch`-byte hash). Note that `chainLog` is not a chain here — [zstd_double_fast.c](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_double_fast.c)
- Per position: (1) write both tables, (2) repcode at ip+1, (3) long match via `MEM_read64`, (4) look up the next long candidate, (5) short match via `MEM_read32`. If short hits, `_search_next_long` also tries the long candidate at ip1 and takes it only if `l1len > mLength`. That condition is PR #4171, which saves ~75 KB on silesia at level 3 for ≤1% speed — [zstd_double_fast.c](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_double_fast.c), [PR #4171](https://github.com/facebook/zstd/pull/4171)
- Step starts at 1 with `kStepIncr = 1 << kSearchStrength = 256`, half the fast acceleration rate. `ZSTD_selectAddr` with a 10-byte dummy array applies the cmov trick to both candidates. aarch64 adds `PREFETCH_L1(ip+256)` — [zstd_double_fast.c](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_double_fast.c)
- Unlike fast, the dfast pipelining performs no speculative throwaway work: long lookup for ip1 is moved earlier because it would be inspected anyway. PR #2774 also writes back ip+1 on long matches, improving ratio — [PR #2774](https://github.com/facebook/zstd/pull/2774)
- Complementary insertion is 4 writes: `curr+2` into both tables, `ip-2` into long, `ip-1` into short — [zstd_double_fast.c](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_double_fast.c)

### Inferences
- For a default-class level 1, the 16,384-entry, 64 KiB table likely sits in L2 on current CPUs, which is why no hash-table prefetch exists on this path. Table bytes derive from `hSize * sizeof(U32)`.
- `step=2` at levels 1+ means every position is searched during the first 128 bytes after a match (ip0, ip1, ip2=ip0+2, ip3). Real skipping starts when `step` reaches 3.
- Level 1's mls=7 hash paired with a 4-byte verify means a candidate passing the verify usually matches at least 7 bytes, so the table is biased to longer matches, which cost fewer bits per byte.

### Gaps
- No primary-source cycle breakdown of fast by stage (hash, lookup, verify, `ZSTD_count`, entropy) was found. Crush can profile that directly.
- Whether the Clang-16 cmov loss was fully resolved was not confirmed by a released measurement.

## What are the compression parameters for levels -5..5 by source size class?

### Takeaway
Levels are a size-indexed table. The default-class pattern is: L1-2 `fast`, L3-4 `dfast`, L5 `greedy`. Tiny inputs shift strategies earlier, and in the 128-256 KB class even level 2 is `dfast`. Negative levels share one base row and only change `targetLength` (hence skip) and turn off Huffman.

### Cited Findings
From [clevels.h](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/clevels.h) (identical in v1.5.7). W=windowLog, C=chainLog, H=hashLog, S=searchLog, L=minMatch, TL=targetLength.

| Class | Level | W | C | H | S | L | TL | Strategy |
|---|---|---|---|---|---|---|---|---|
| >256 KB (default) | neg base | 19 | 12 | 13 | 1 | 6 | -level | fast |
| | 1 | 19 | 13 | 14 | 1 | 7 | 0 | fast |
| | 2 | 20 | 15 | 16 | 1 | 6 | 0 | fast |
| | 3 | 21 | 16 | 17 | 1 | 5 | 0 | dfast |
| | 4 | 21 | 18 | 18 | 1 | 5 | 0 | dfast |
| | 5 | 21 | 18 | 19 | 3 | 5 | 2 | greedy |
| ≤256 KB | neg base | 18 | 12 | 13 | 1 | 5 | -level | fast |
| | 1 | 18 | 13 | 14 | 1 | 6 | 0 | fast |
| | 2 | 18 | 14 | 14 | 1 | 5 | 0 | dfast |
| | 3 | 18 | 16 | 16 | 1 | 4 | 0 | dfast |
| | 4 | 18 | 16 | 17 | 3 | 5 | 2 | greedy |
| | 5 | 18 | 17 | 18 | 5 | 5 | 2 | greedy |
| ≤128 KB | neg base | 17 | 12 | 12 | 1 | 5 | -level | fast |
| | 1 | 17 | 12 | 13 | 1 | 6 | 0 | fast |
| | 2 | 17 | 13 | 15 | 1 | 5 | 0 | fast |
| | 3 | 17 | 15 | 16 | 2 | 5 | 0 | dfast |
| | 4 | 17 | 17 | 17 | 2 | 4 | 0 | dfast |
| | 5 | 17 | 16 | 17 | 3 | 4 | 2 | greedy |
| ≤16 KB | neg base | 14 | 12 | 13 | 1 | 5 | -level | fast |
| | 1 | 14 | 14 | 15 | 1 | 5 | 0 | fast |
| | 2 | 14 | 14 | 15 | 1 | 4 | 0 | fast |
| | 3 | 14 | 14 | 15 | 2 | 4 | 0 | dfast |
| | 4 | 14 | 14 | 14 | 4 | 4 | 2 | greedy |
| | 5 | 14 | 14 | 14 | 3 | 4 | 4 | lazy |

- Table selection: `tableID = (rSize<=256KB)+(rSize<=128KB)+(rSize<=16KB)`. Negative levels use row 0 with `cp.targetLength = -level`. Row 0's literal TL value of 1 is overwritten — [ZSTD_getCParams_internal](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_compress.c)
- `ZSTD_minCLevel()` = `-ZSTD_TARGETLENGTH_MAX` = -131072. Default level is 3, level 0 maps to it — [zstd_compress.c:8206](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_compress.c#L8206), [zstd.h](https://github.com/facebook/zstd/blob/49cf5179/lib/zstd.h)
- When source size is known, `ZSTD_adjustCParams_internal` further clamps windowLog to source size, and hashLog to `windowLog+1` — [zstd_compress.c](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_compress.c)

Derived step and table memory for the default class:

| Level | Initial step | Match-table bytes |
|---|---|---|
| -5 | 6 | 32 KiB |
| -1 | 2 | 32 KiB |
| 1 | 2 | 64 KiB |
| 2 | 2 | 256 KiB |
| 3 | 1 | 512 KiB long + 256 KiB short |
| 4 | 1 | 1 MiB + 1 MiB |
| 5 | n/a (row) | 2 MiB + 512 KiB tags |

### Inferences
- Level -1 and level 1 start with the **same** step of 2. Their ratio gap (86.9 MB vs 73.2 MB on silesia) therefore comes mostly from Huffman being off, plus hashLog 13 vs 14 and mls 6 vs 7.
- Level 2 costs four times the table memory of level 1 and uses mls 6, trading L2-cache residency for finding shorter and older matches.
- The minMatch sequence 7, 6, 5 from L1 to L3 means the fastest level skips short matches that entropy coding could hardly pay for anyway.

### Gaps
- Level-tuning methodology (paramgrill targets) was not reviewed.

## How are repcodes used, how much do they contribute, and why does it matter vs. LZ4?

### Takeaway
At fast levels repcodes are a cheap one-load probe that replaces an expensive offset with a nearly free symbol, and it is the signature gap against LZ4-class formats. No published figure for their share or ratio contribution was found. Measure this on Crush corpora rather than assuming it.

### Cited Findings
- Format: 3 recency-ordered offsets, starting at 1, 4, 8. `offset_value` 1-3 selects a slot. When `literals_length == 0` they shift by one, so value 1 = Rep2, 2 = Rep3, 3 = Rep1-1 — [format spec](https://github.com/facebook/zstd/blob/49cf5179/doc/zstd_compression_format.md), [zstd_internal.h:65](https://github.com/facebook/zstd/blob/49cf5179/lib/common/zstd_internal.h#L65)
- History carries across compressed blocks; non-compressed blocks don't contribute — [format spec](https://github.com/facebook/zstd/blob/49cf5179/doc/zstd_compression_format.md)
- `ZSTD_fast` tests **rep1 only** in the main loop, at ip2, once per iteration, and the check is branchless: `(MEM_read32(ip2)==rval) & (rep_offset1>0)`. It tests rep2 only immediately after a match. All fast noDict repcode emissions use `REPCODE1_TO_OFFBASE`; with litLength 0 that encodes the second slot. Only `rep[0]` and `rep[1]` are touched — [zstd_fast.c](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_fast.c)
- dfast tests rep1 at ip+1 for every searched position, before the long-candidate test, and has the same post-match rep2 loop — [zstd_double_fast.c](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_double_fast.c)
- In the pipeline the hashes for ip0 and ip1 are already computed when the repcode test runs, so a repcode hit does not save hashing. What it saves is the candidate verify and backward byte loop (only a 1-byte backward check is done), and the offset bits — [zstd_fast.c](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_fast.c)
- Offset cost: `ofCode = highbit32(offBase)`, written as `ofCode` raw bits on top of an FSE symbol. So a 64 KiB offset costs ~16 raw bits plus the code, while repcodes (offBase 1-3) cost 0-1 extra bits plus the code — [ZSTD_seqToCodes](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_compress.c), [zstd_compress_sequences.c:316-356](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_compress_sequences.c#L316)
- zstd's API docs state repcodes improve ratio but the benefit varies greatly with the sequences, and that repcode resolution is expensive in `ZSTD_compressSequences` context — [zstd-sys docs mirror](https://docs.rs/zstd-sys/latest/zstd_sys/fn.ZSTD_compressSequences.html)
- ZSTDMT calls `ZSTD_invalidateRepCodes` at the start of every non-first job — [zstdmt_compress.c:757](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstdmt_compress.c#L757)

### Inferences
- Background knowledge not re-verified in this research: the LZ4 block format has a fixed 2-byte offset and no repeat-offset concept. Against that, zstd's repcode benefits structured data most (tables, fixed-stride records), where the same offset recurs across small edits.
- Repcode matches also shorten the offset FSE alphabet in use, an indirect entropy gain.
- The cost to port is low: one extra 32-bit load and compare per iteration, placed where the pipeline's data is already in flight.

### Gaps
- **No published measurement** was found for repcode share of sequences or ratio gain with/without repcodes at levels 1-3. Measure by running `ZSTD_generateSequences` or an instrumented `ZSTD_storeSeq` on target corpora; a build with repcode checks removed gives the ratio and speed contribution directly.

## How are literals and sequences separated, and how do blocks and splitting affect speed and ratio?

### Takeaway
Match finding only writes to a per-block `seqStore`, and entropy coding happens afterwards in one batch. That separation lets the search loop stay tight and lets Huffman and FSE use tuned table reuse. Pre-block splitting (v1.5.7) buys 0.3-0.5% on archives at <2% speed on fast levels.

### Cited Findings

**seqStore**
- `SeqDef` is 8 bytes: `U32 offBase; U16 litLength; U16 mlBase`. Literals are appended to a separate buffer; `llCode`, `ofCode`, `mlCode` are parallel byte arrays filled in one post-pass by `ZSTD_seqToCodes`. A single length above 0xFFFF per block is handled out of band through `longLengthType/Pos` — [zstd_compress_internal.h:85](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_compress_internal.h#L85), [ZSTD_storeSeqOnly](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_compress_internal.h#L760)
- `ZSTD_storeSeq` unconditionally copies 16 bytes first ("literals are likely short"), then wildcopies the remainder, using a slower safe path near the block end — [zstd_compress_internal.h:801-835](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_compress_internal.h#L801)
- Memory: `tokenSpace` = block + 32 bytes + `maxNbSeq*8` + 3 code arrays — [ZSTD_estimateCCtxSize_usingCCtxParams_internal](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_compress.c)

**Literals**
- Huffman is attempted only if literals reach 64 bytes for fast, or 6 if a valid repeat table exists (`ZSTD_minLiteralsToCompress`) — [zstd_compress_literals.c](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_compress_literals.c)
- **Negative levels disable Huffman**: `literalCompressionMode` auto returns disabled when `strategy == ZSTD_fast && targetLength > 0` — [zstd_compress_internal.h](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_compress_internal.h); this explains the small splitter gain there per [PR #4178](https://github.com/facebook/zstd/pull/4178)
- Single stream below 256 literals, otherwise 4 streams, except 1 stream when a valid repeat table and a 3-byte header are in use. Below `ZSTD_lazy` and ≤1 KB of literals, `HUF_flags_preferRepeat` reuses the previous table. `LitHufLog = 11`. Cheap FSE-based table-log estimate for levels below btultra — [zstd_compress_literals.c](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_compress_literals.c), [huf.h:117](https://github.com/facebook/zstd/blob/49cf5179/lib/common/huf.h#L117), [huf_compress.c:1287](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/huf_compress.c#L1287)
- Incompressible early-out: if `litSize/nbSeq >= 20` and literals are ≥40,960 bytes, histogram 4 KB at each end and abandon when `largestTotal <= 68` — [zstd_compress.c:2915](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_compress.c#L2915), [huf_compress.c:1370-1381](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/huf_compress.c#L1370)
- v1.5.1 Huffman rework, literal compression at 128 KB: 748 → 927 MB/s (gcc-11), 810 → 927 MB/s (clang-13). At 4 KB: 223 → 321 MB/s. Overall effect: 1-4% compression speed — [v1.5.1 notes](https://github.com/facebook/zstd/releases/tag/v1.5.1)

**Sequences**
- FSE logs: LL 9, ML 9, Off 8 — [zstd_internal.h:108-110](https://github.com/facebook/zstd/blob/49cf5179/lib/common/zstd_internal.h#L108)
- Below `ZSTD_lazy`, encoding type is a cheap heuristic: `set_repeat` when valid and `nbSeq < 1000`; `set_basic` for few sequences or flat distribution; else a new table. `lazy`+ computes actual bit costs — [ZSTD_selectEncodingType](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_compress_sequences.c#L158)

**Blocks and frame**
- `ZSTD_BLOCKSIZE_MAX = 128 KB`; 3-byte block header. Fallback to raw if savings below `ZSTD_minGain = (srcSize>>6)+2` (for strategy < btultra), i.e. about 1.6%; RLE path exists — [zstd.h:148](https://github.com/facebook/zstd/blob/49cf5179/lib/zstd.h#L148), [zstd_compress_internal.h](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_compress_internal.h), [ZSTD_compress_frameChunk](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_compress.c)
- After long matches, `ZSTD_buildSeqStore` limits catch-up: when more than 384 positions behind, it jumps to within 192 — [ZSTD_buildSeqStore](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_compress.c)

**Block splitting**
- Pre-splitter chooses `splitLevels[] = {0,0,1,2,2,3,3,4,4,4}` by strategy; applied only when ≥128 KB remain and at least 3 bytes of savings have been verified, so **the first block is never split** — [ZSTD_optimalBlockSize](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_compress.c#L4581)
- `fast` (split level 0): `ZSTD_splitBlock_fromBorders`. Histogram 512 bytes at the start and end of a 128 KB block; if different, sample the middle and split at 32, 64 or 96 KB — [zstd_preSplit.c](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_preSplit.c)
- `dfast` (level 1): `byChunks` over 8 KB chunks with 1-in-43 sampling and 8-bit fingerprints. `greedy`/`lazy`: 1-in-11, 9-bit. `lazy2`/`btlazy2`: 1-in-5, 10-bit. `btopt`+: every byte, 10-bit — [zstd_preSplit.c](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_preSplit.c)
- Measured on silesia.tar. L1: 73,422,067 → 73,193,855 (-0.31%); L2: 69,503,582 → 69,309,934 (-0.28%), <2% speed cost — [PR #4178](https://github.com/facebook/zstd/pull/4178). L3: 66,456,850 → 66,133,685 (-0.49%); L4: -298,843; L5: -299,458, <2% speed cost — [PR #4176](https://github.com/facebook/zstd/pull/4176)
- The first PR's static 92 KB size for fast levels ([PR #4136](https://github.com/facebook/zstd/pull/4136)) was superseded by #4178; the current code has no static 92 KB case — [ZSTD_optimalBlockSize](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_compress.c#L4581)
- The sequence-based post-splitter is on only for `strategy >= btopt && windowLog >= 17` (default since v1.5.0, levels 16+) — [ZSTD_resolveBlockSplitterMode](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_compress.c#L269), [v1.5.0 notes](https://github.com/facebook/zstd/releases/tag/v1.5.0)

### Inferences
- The gains above come from a heterogeneous tar. On homogeneous data expect far less.
- Block size is the unit of entropy adaptation: 128 KB means roughly 7-8 Huffman and FSE table decisions per MiB. A Rust encoder can spend ratio budget here without touching the match loop.
- Crush's independent parallel blocks should keep the two-phase split so that match finding never touches the entropy state.

### Gaps
- Published time split between match search and entropy stage for L1-3 was not found.

## What does the row-based match finder do with SIMD, and which levels use it?

### Takeaway
It is a bucketed hash table with a byte tag per entry scanned by SIMD; only `greedy/lazy/lazy2` use it. For inputs above 256 KB that is levels 5-12, so it is **not** part of levels 1-3 or 4 at default class.

### Cited Findings
- Enabled in auto mode for greedy-through-lazy2 when `windowLog > 14` (library), `> 17` under `ZSTD_LINUX_KERNEL` because the kernel has no SIMD and 128 KB is common there (commit `68dfd14a`, 2025-03, post-v1.5.7) — [zstd_compress.c:238-266](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_compress.c#L238), [v1.5.0 notes](https://github.com/facebook/zstd/releases/tag/v1.5.0)
- Structure: rows of 16, 32 or 64 entries (`rowLog = BOUNDED(4, searchLog, 6)`), each with `U32` index plus 8-bit tag (`ZSTD_ROW_HASH_TAG_BITS 8`). The tag table costs one byte per hash entry. An 8-entry rolling cache (`ZSTD_ROW_HASH_CACHE_SIZE`) precomputes hashes and prefetches rows — [zstd_lazy.c](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_lazy.c), [zstd_compress_internal.h:270](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_compress_internal.h#L270)
- `ZSTD_row_getMatchMask` builds a bitmask from tag compares: SSE2 (`_mm_cmpeq_epi8`/`_mm_movemask_epi8` on 16-byte chunks; **no AVX2 variant in `zstd_lazy.c`**), NEON, RVV, or a SWAR fallback. It rotates by row head, then walks set bits oldest-to-newest with prefetch before verification — [zstd_lazy.c](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_lazy.c)
- Long-match guard: past 384 positions, update only the first 96 and last 32 — [zstd_lazy.c:923](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_lazy.c#L923)
- Speed: +25% at L5, +50% at L6 and up to +110% at L12 on silesia, with "2-3x+" internally at L5-7 on contended machines; SSE2 needed for the full gain — [v1.5.0 notes](https://github.com/facebook/zstd/releases/tag/v1.5.0)
- Below `ZSTD_btopt`-class, `step = ((ip-anchor) >> 8) + 1`, and over 8-byte steps (about 2 KB without matches) the finder stops inserting skipped positions. Result on incompressible data: L5 400 → 2500 MB/s in v1.5.5 — [zstd_lazy.c:1658](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_lazy.c#L1658), [v1.5.5 notes](https://github.com/facebook/zstd/releases/tag/v1.5.5)
- `ZSTD_row_getMatchMask` on ARM was tuned with `vshrn_n_u16` for 3-5% at levels 8-9 — [PR #3139](https://github.com/facebook/zstd/pull/3139)

### Inferences
- The tag-probe design is the natural "beat L3" route for a Rust library: a 16-entry row scan with SIMD fits in-between dfast and greedy cost, and Rust `std::arch` or `core::simd` can use AVX2 where zstd only uses SSE2.
- For inputs ≤16 KB (windowLog 14), the default stays on hash chains; the v1.5.0 notes confirm row is slower there.

### Gaps
- No published data on an AVX2 or AVX-512 row probe compared with SSE2.
- `ZSTD_row` at L4 of the ≤256 KB class specifically was not benchmarked in the sources.

## What SIMD, BMI2 and assembly does the compressor use? Which hot paths are scalar?

### Takeaway
On x86-64 the ordinary L1-3 path has no wide SIMD: matching and counting are 64-bit scalar, copying is 16-byte SSE2, and bitstream writing gets BMI2 by function multiversioning. Scalar hot paths are where a Rust port has the most room.

### Cited Findings
- No `.S` assembly exists in `lib/compress` (only decoder-side asm is noted in release notes) — [lib/compress listing](https://github.com/facebook/zstd/tree/49cf5179/lib/compress), [v1.5.1 notes](https://github.com/facebook/zstd/releases/tag/v1.5.1)
- `ZSTD_count`: 8-byte `MEM_readST` XOR loop, `ZSTD_NbCommonBytes` is count-trailing-zeros >> 3 — scalar, 4/2/1 tail — [zstd_compress_internal.h:879](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_compress_internal.h#L879), [bits.h:157](https://github.com/facebook/zstd/blob/49cf5179/lib/common/bits.h#L157)
- `ZSTD_wildcopy`: one 16-byte copy, then a double-copy loop; `ZSTD_copy16` is NEON, SSE2 or RVV. `WILDCOPY_OVERLENGTH 32`. No AVX2 version — [zstd_internal.h:183-211](https://github.com/facebook/zstd/blob/49cf5179/lib/common/zstd_internal.h#L183)
- `fast` and `dfast` contain only `PREFETCH_L1` and inline cmov; no vector intrinsics — [zstd_fast.c](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_fast.c), [zstd_double_fast.c](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_double_fast.c)
- AVX2 in `zstd_compress.c` is confined to the `convertSequences`/block-summary paths for `ZSTD_compressSequencesAndLiterals`, with NEON/SVE2/RVV counterparts (added post-1.5.7 for ARM and RISC-V) — [zstd_compress.c:7142](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_compress.c#L7142)
- Dynamic BMI2 multiversioning: `HUF_compress1X_usingCTable_internal_bmi2` and `ZSTD_encodeSequences_bmi2` — the same C compiled with BMI2 target attribute. Comment notes `shrx` efficiency for `HUF_addBits` — [huf_compress.c:883,1125](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/huf_compress.c#L883), [zstd_compress_sequences.c:405](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_compress_sequences.c#L405), [portability_macros.h:77-99](https://github.com/facebook/zstd/blob/49cf5179/lib/common/portability_macros.h#L77)
- Huffman encoder uses a custom `HUF_CStream_t` with **two** bit containers, unrolled symbols per flush, and a table stored as (nbBits, value) in one `size_t`. Encoding goes backward through input — [huf_compress.c:825-1030](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/huf_compress.c#L825)
- Histogram: `HIST_count_parallel_wksp` is scalar multi-table counting, used above 1500 bytes. An SVE2 variant arrived after 1.5.7 with threshold 500 — [hist.c:22-25,321](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/hist.c#L22), commit `7e4937bc`
- `ZSTD_hashPtr` is scalar multiplicative, per position — [zstd_compress_internal.h:923-948](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_compress_internal.h#L923)

### Inferences
- Candidates to beat in Rust: 16-32 byte `ZSTD_count` via vector compare, for long-match inputs; AVX2 literal copy; batch hashing of 4+ positions with `vpmulld`/`vpmuludq`. The cost is that mispredict-heavy short matches may gain nothing, so gate on profile data.
- Function multiversioning with `#[target_feature(enable = "bmi2")]` is the Rust equivalent of `DYNAMIC_BMI2`.

### Gaps
- No primary measurement isolating BMI2's effect at L1-3 was found.

## How does multi-threaded zstd behave?

### Takeaway
ZSTDMT splits the input into large jobs, each with an overlap prefix, in one frame. Ratio stays near single-thread, but in-order flushing, buffer limits and serial tasks can cap practical scaling well below core count.

### Cited Findings
- Job size: `jobLog = MAX(20, windowLog + 2)`, capped at 1 GiB on 64-bit. Minimum job 512 KB. Max workers 256 (64-bit) — [zstdmt_compress.c:1186](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstdmt_compress.c#L1186), [zstdmt_compress.h:29-36](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstdmt_compress.h#L29)
- Overlap: `overlapRLog = 9 - overlapLog`. For fast through lazy, overlapLog is 6, so overlap = `1 << (windowLog-3)` = window/8 — [zstdmt_compress.c:1200-1240](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstdmt_compress.c#L1200)
- A non-first job loads its prefix via `ZSTD_compressBegin_advanced_internal(..., ZSTD_dct_rawContent, ZSTD_dtlm_fast, ...)`, so **it re-indexes the overlap** (`fastHashFillStep = 3`), drops its frame header, and invalidates repcodes. Chunks are 4 × 128 KB between progress updates — [zstdmt_compress.c:742-772](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstdmt_compress.c#L742)
- XXH64 checksum and LDM generation run serially, in job order, in `ZSTDMT_serialState` — [zstdmt_compress.c:594-615](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstdmt_compress.c#L594)
- Memory: round buffer capacity = `MAX(window, targetSection*nbWorkers) + targetSection*(2 + (overlap>0))`; job table `nbWorkers+2`; output buffer pool up to `2*nbWorkers+3`; plus one CCtx per worker — [zstdmt_compress.c:278,930,1320-1330](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstdmt_compress.c#L278)
- CLI default since v1.5.7: multi-threaded, `MAX(1, MIN(4, logicalCores/4))` workers; `-T0` means physical cores — [zstdcli.c:47](https://github.com/facebook/zstd/blob/49cf5179/programs/zstdcli.c#L47), [v1.5.7 CHANGELOG](https://github.com/facebook/zstd/blob/49cf5179/CHANGELOG), [man page](https://github.com/facebook/zstd/blob/49cf5179/programs/zstd.1.md)
- Bakhvalov's analysis (Silesia, level 3, version and hardware unstated): "Performance scaling stops at around 5 threads." Causes: jobs are posted only when input buffers are free (idle gaps of 20-40 ms), one fragmentation-prone pool shares input and output buffers, an unflushed finished job blocks the next, and output is flushed strictly in order — [Easyperf 2024](https://easyperf.net/blog/2024/05/10/Thread-Count-Scaling-Part3)
- Contrast, **independent** 4 MB blocks under lzbench 2.4 (not ZSTDMT) on EPYC 9555P: -1 400 → 3039 → 8672 MB/s (T1, T8, T32); -3 240 → 1524 → 4627 MB/s; lz4 537 → 3915 → 11671 MB/s. 32 threads have at most ~25× available — [lzbench README](https://github.com/inikep/lzbench)

Derived job and buffer figures, default class:

| Level | Job size | Overlap | Round buffer at T8 |
|---|---|---|---|
| 1 | 2 MiB | 64 KiB | 22 MiB |
| 2 | 4 MiB | 128 KiB | 44 MiB |
| 3 | 8 MiB | 256 KiB | 88 MiB |

### Inferences
- lzbench scaling is 7.6× at T8 for -1 and 6.4× for -3. The -1 vs -3 gap under identical block handling suggests memory bandwidth and cache contention grow with table size, since dfast's tables are larger and random-access.
- 4 MB independent blocks cost almost no ratio at silesia scale: -1 is 34.58% vs 34.53% solid. So a Crush block-parallel format can skip overlap and shared prefix, and still be close, for block sizes ≫ window. Below 1 MB the picture may differ.
- Bakhvalov's bottlenecks are implementation details, not format limits. Parallel output collection without in-order blocking, and per-worker preallocated buffers, directly attack them. This is a realistic place to beat zstd on many-core machines.
- ZSTDMT output is one frame with cross-job references, so it cannot be decompressed in parallel without additional indexing. Crush's block format can.

### Gaps
- No clean published `zstd -T` sweep with a stated version and hardware was found. The other search hits (a Centmin Mod forum thread, a Redpill Linpro test) lacked thread controls or had internally odd results, so they are not used here. A direct sweep on Crush bench machines is needed.
- ZSTDMT's total resident memory at each `-T` was not measured.

## What role do long-distance matching and dictionaries play?

### Takeaway
Neither is active at levels 1-3 by default. LDM matters for very large inputs with distant redundancy; dictionaries matter for small data, where they sharply raise ratio.

### Cited Findings
- LDM auto-enables only when `strategy >= btopt && windowLog >= 27`. `--long` defaults to windowLog 27; decompressors need a flag above that — [zstd_compress.c:290](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_compress.c#L290), [man page](https://github.com/facebook/zstd/blob/49cf5179/programs/zstd.1.md)
- LDM uses a gear rolling hash sampled every `1<<hashRateLog` bytes. Defaults: minMatch 64 (32 for btultra+), bucketLog ≥ 4, `hashRateLog = 7 - strategy/3`. Its sequences feed the normal block compressor for gaps — [zstd_ldm.c](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_ldm.c)
- With dictionaries, CDict tables use 8-bit tags to reject cache-missing dictionary probes, and `ZSTD_compressBlock_fast_dictMatchState` was pipelined for +5-6%. In that test, an HTML headers dataset went from 2.32 to 3.41 ratio with a dictionary — [zstd_double_fast.c](https://github.com/facebook/zstd/blob/49cf5179/lib/compress/zstd_double_fast.c), [PR #3086](https://github.com/facebook/zstd/pull/3086)
- README: dictionary gains concentrate in the first few KB and must be data-specific; with a dictionary, speed also rises — [README](https://github.com/facebook/zstd/blob/49cf5179/README.md)
- v1.5.7 cites ~+5% for low-level dictionary compression, with "none to +15%" in release notes — [CHANGELOG](https://github.com/facebook/zstd/blob/49cf5179/CHANGELOG), [v1.5.7 notes](https://github.com/facebook/zstd/releases/tag/v1.5.7)

### Inferences
- For a block-parallel format with 64 KB-4 MB blocks, a shared trained dictionary may be the cheapest ratio win on small blocks. LDM competes with Crush's block-independent parallelism and is not a priority for beating L1-3.

### Gaps
- No current primary benchmark of `--long` ratio gain at low levels was retrieved: the Collet blog URL tried returned 404.

## Published speed/ratio figures and what changed from 1.5.0 onward

### Takeaway
Levels 1-3 have been format- and parameter-stable. Since 1.5.0, single-stream throughput on large inputs improved only in single-digit steps; the big jumps were for small blocks and incompressible data. There are no match-path changes since 1.5.7 in `dev`.

### Cited Findings

**lzbench 2.4 on silesia.tar (211,947,520 bytes): EPYC 9555P @ 3.2 GHz, single thread, gcc 15.2, Ubuntu 26.04, turbo off** — [lzbench README](https://github.com/inikep/lzbench)

| Codec | Compress MB/s | Decompress MB/s | Size (bytes) | Ratio |
|---|---|---|---|---|
| lz4 1.10.0 | 550 | 3832 | 100,880,800 | 2.10 |
| zstd 1.5.7 --fast=5 | 563 | 1949 | 103,023,359 | 2.06 |
| zstd 1.5.7 --fast=3 | 509 | 1803 | 94,602,144 | 2.24 |
| zstd 1.5.7 --fast=1 | 453 | 1707 | 86,916,294 | 2.44 |
| zstd 1.5.7 -1 | 411 | 1351 | 73,193,704 | 2.90 |
| zstd 1.5.7 -2 | 352 | 1257 | 69,309,797 | 3.06 |
| zstd 1.5.7 -5 | 126 | 1172 | 62,740,852 | 3.38 |
| libdeflate 1.26 -1 | 202 | 783 | 73,502,791 | 2.88 |

Ratios are computed from the listed sizes. The full single-thread table has no `zstd -3` row.

- Level 3 (independent 4 MB blocks, same machine): 31.37% size (ratio ~3.19), 240 MB/s compress, 1151 MB/s decompress; -1 under the same mode: 34.58%, 400 MB/s, 1303 MB/s — [lzbench README](https://github.com/inikep/lzbench)
- Level 3 size at 1.5.7 development time: 66,133,685 bytes (ratio ~3.20) — [PR #4176](https://github.com/facebook/zstd/pull/4176)
- Project README (i7-9700K @ 4.9 GHz, gcc 14.2, lzbench, Silesia): zstd 1.5.7 -1 2.896 / 510 / 1550 MB/s; --fast=1 2.439 / 545 / 1850; --fast=4 2.146 / 665 / 2050; lz4 1.10.0 2.101 / 675 / 3850 — [README](https://github.com/facebook/zstd/blob/49cf5179/README.md)

**Release changes relevant to levels ≤5**

| Version (date) | Change | Quantified effect |
|---|---|---|
| 1.5.0 (2021) | Row match finder for L5-12 | +25% (L5) to +110% (L12) |
| 1.5.1 (2021-12) | Pipelined fast and dfast (#2749, #2774, #2921); Huffman rewrite | ~3-5%; plus 1-4% from Huffman |
| 1.5.2 (2022) | Small-stream init regression fixed (#2969) | L3 31 → 94 MB/s on small-input streaming |
| 1.5.4 (2023-02) | Streaming large-file path | `ZSTD_compressStream` -1: 392 → 429 MB/s |
| 1.5.5 (2023-04) | Row finder small-stream and incompressible | L5 400 → 2500 MB/s on incompressible; L3 unchanged control |
| 1.5.6 (2024-03) | Mostly `targetCBlockSize` and high-level gains | none material at L1-3 found |
| 1.5.7 (2025-02) | cmov for small blocks; block splitter; dfast longer-match rule | -1: 4 KB 280 → 310, 32 KB 383 → 458, 128 KB 460 → 485 MB/s; L1-3 -0.3 to -0.5% size |
| dev, unreleased | BMI2 CPUID skip, RVV/SVE2/NEON paths, 64-bit long-offset elision (`ae703425`) | x86 match path unchanged; no benchmark found |

Sources: [v1.5.0](https://github.com/facebook/zstd/releases/tag/v1.5.0), [v1.5.1](https://github.com/facebook/zstd/releases/tag/v1.5.1), [PR #2969](https://github.com/facebook/zstd/pull/2969), [v1.5.4](https://github.com/facebook/zstd/releases/tag/v1.5.4), [v1.5.5](https://github.com/facebook/zstd/releases/tag/v1.5.5), [v1.5.6](https://github.com/facebook/zstd/releases/tag/v1.5.6), [v1.5.7](https://github.com/facebook/zstd/releases/tag/v1.5.7), [dev commits](https://github.com/facebook/zstd/commits/dev/lib/compress).

- Caveat on the "~5%" pipeline claim: at the initial commit of #2749 on a Xeon E5-1650 v3, silesia.tar moved 2.884 → 2.888 with speed **355 → 352 MB/s** (gcc-10) and 363 → 336 MB/s (clang-11). The gain concentrated on poorly compressible inputs (P5: 789 → 903 MB/s). Later commits in the PR were tuned further — [PR #2749](https://github.com/facebook/zstd/pull/2749)

### Inferences
Trade-offs per step, from the tables above (lzbench single-thread except where noted):
- `--fast=1` → `-1`: 15.8% smaller for 9.3% less speed. Best step on the curve; Huffman on literals is the likely primary cause.
- `-1` → `-2`: 5.3% smaller for 14.4% less speed.
- `-1` → `-3`: 8-10% smaller for ~40% less speed (4 MB block mode: 34.58 → 31.37%, 400 → 240 MB/s).
- lz4 → zstd `-1`: 27.4% smaller for 25% less compression speed and 65% less decompression speed.
- libdeflate -1 reaches the same size as zstd -1 at half the compression and 58% of the decompression speed.
- The steep L2→L3 speed cliff indicates an opening: a design between `fast` and `dfast`, e.g. a two-candidate bucket with cheap tags, could occupy the ratio of L3 at meaningfully better speed.

### Gaps
- There is no single same-machine series across zstd versions for levels 1-3 in primary sources. The numbers above span machines and compilers; compare within a row only.
- lzbench does not publish a `zstd -3` row in its single-threaded README table, nor `-4`.

## What should a Rust implementation port or beat?

### Takeaway
Port the cheap structural ideas first, since they account for most of zstd's ratio edge over LZ4 at nearly no search cost: separated streams, Huffman literals, FSE sequences, repcodes, table reuse. Beat zstd on parallel architecture and wide-SIMD paths it leaves scalar.

### Cited Findings
- All facts are in the sections above. The Rust ecosystem has active ports: `mzstd` appears in lzbench as of 2026-10-15 ([lzbench README](https://github.com/inikep/lzbench)), and its docs show a core `compressor` module structure ([mzstd source](https://docs.rs/crate/mzstd/latest/source/src/compressor/core/seq_store.rs)). Its claimed byte-identity with zstd 1.5.7 came from a search snippet and was not verified.

### Inferences

**Port (high confidence, known value)**
1. Pipelined fast loop with `ip0..ip3`, hash reuse across iterations, and write-back. Rust needs `unsafe` pointer reads or carefully bounds-elided slices, and possibly `#[inline(always)]`, to preserve the schedule.
2. Runtime strategy selection analogous to `useCmov`: bounds validity branches vs cmov per window size. Rust's `select_unpredictable` or black-box patterns may be needed to force a cmov, since zstd resorted to inline asm.
3. Repcode probe at the cost of one load; `rep_offset2` after each match.
4. minMatch/hashLog per size class, including `step` adaptation (128-byte cadence).
5. Fast heuristics: min-literals thresholds, `set_repeat` < 1000 seq, 4 KB head+tail incompressibility check.
6. Two-point complementary insertion; never index whole matches.
7. Border-sampling pre-splitter: 512-byte histograms, very low cost, +0.3% on mixed archives.

**Beat (higher risk, needs measurement)**
1. Parallel block scaling, addressing ZSTDMT's buffer-pool and in-order flush limits. Evidence suggests ~5-thread stalls in zstd vs near-linear lzbench independent-block scaling.
2. Parallel decompression, which ZSTDMT frames do not offer without an index.
3. L3-class ratio at L1-2-class speed through a tag-filtered 2-way or 4-way bucket, filling the L2→L3 cliff.
4. Wider `ZSTD_count`, literal copy, and multi-position hashing using AVX2 or AVX-512, each validated against short-match workloads.
5. Parameter switching at block boundaries from fast sampling, already hinted by zstd's splitter fingerprinting.
6. Hash-table prefetch for large-table levels; zstd's noDict fast path has none, only input prefetch.

**Do not chase first**
- Beating `-1` on every corpus while keeping an incompatible format provides little benefit if ratio falls below 2.9 on silesia. Bench against the full curve instead.
- LDM at this speed tier. Parameter defaults show zstd itself does not use it at low levels.
- High-level decoders or `btopt` parsing; they are outside the 1-5 speed target.

### Gaps
- No primary source quantifies how much of zstd -1's ratio edge over LZ4 comes from each of repcodes, Huffman, FSE and table sizing. The analysis above is inferred; an ablation study with a modified zstd build would settle this.
- Rust-specific code generation, such as whether `rustc` keeps the cmov and load ordering, was not tested here.
