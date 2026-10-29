# Research: LZ4-Class Fast Compression Plugin

Phase 0 of [plan.md](plan.md). Each item: decision, rationale, alternatives.

## R1. Reference numbers and how targets are verified

**Decision**: Codec targets are measured in WSL2 Ubuntu on the same machine, in the same session as `lz4 -b1` (lz4 1.10.0 built from source into `~/.local/bin`), on the same 128 MB file (first 128 MB of `tar -cf - /usr/lib /usr/share`), single thread pinned with `taskset -c 0`. A small harness (`perf/lz-vs-lz4.sh`) builds the crush codec benchmark in WSL, runs both, and prints MB/s and ratio side by side. Windows criterion benchmarks (synthetic and file-based corpora) gate regressions and drive `/perf goal` A/B experiments.

Measured on 2026-10-29 (one core, in memory):

| codec | compress | decompress | ratio (real files) |
|---|---|---|---|
| lz4 -1 | 788 MB/s | 4,417 MB/s | 2.296 |
| zstd -1 | 580 MB/s | 1,788 MB/s | 3.233 |

Targets: compress ≥ 906 MB/s, decompress ≥ 2,650 MB/s, ratio ≥ 2.18.

**Rationale**: The reference corpus only exists on Linux (`/usr/lib`), and lz4's built-in benchmark is the honest codec-to-codec comparison; same-session measurement cancels machine drift, per the perf skill's rules.

**Alternatives**: Windows-only criterion vs a Windows lz4 binary (no matching corpus, extra tooling); end-to-end CLI timing only (dominated by Crush's pipeline overhead, which caps one core at ~614 MB/s with no compression work — that is Story 3, not the codec).

## R2. Where the plugin lives

**Decision**: A new workspace crate `crush-lz` (library `crush_lz`), registered into crush-core's plugin registry through the existing `linkme` distributed slice, exactly like `crush-parallel`. `crush-cli` depends on it so the plugin is linked into the binary. Plugin name `fast-lz`, magic `[0x43, 0x52, 0x01, 0x04]` (0x00 deflate, 0x02 parallel-deflate, 0x03 gpu-deflate are taken; 0x10/0x11 appear only in selector tests).

**Rationale**: Constitution III: plugins independently testable, unused plugins compile away, no core-engine changes beyond the selection hook. A separate crate also gives the codec its own benches and fuzz targets.

**Alternatives**: A module inside `crush-core` (mixes a codec into the core library, harder to keep independent); inside `crush-parallel` (unrelated engine and format).

## R3. Compressed format

**Decision**: Crush's own block format, LZ4-style sequences inside independent blocks (full layout in [contracts/fast-lz-format.md](contracts/fast-lz-format.md)):
- Stream = small frame header (format version, block size) + sequence of blocks + end marker.
- Each block ≤ 1 MiB of input, compressed independently, with a header holding compressed and uncompressed sizes and a stored flag (block kept raw when it doesn't shrink).
- Inside a block: sequences of `token (4-bit literal length, 4-bit match length) · extended literal length · literals · 2-byte little-endian offset · extended match length`, minimum match 4, offsets 1..65,535 (64 KiB window), extended lengths in 255-byte steps — the well-understood LZ4 block grammar.
- Integrity: the crush-core outer header's CRC32 over the payload (as for every plugin) plus exact size checks per block on decode.

**Rationale**: The LZ4 sequence grammar is proven fast to decode and simple to fuzz. Independent 1 MiB blocks bound memory (FR-012), keep allocation proportional to declared sizes (FR-005), give a natural cancellation and stored-block granularity, and leave multi-core use open later without a format change. A 64 KiB window keeps offsets 2 bytes and the hash table cache-resident.

**Alternatives**: Bit-exact LZ4 frame format (would add xxHash and frame features we don't need, no benefit since compatibility with `lz4` files is not a requirement); larger window (better ratio, slower, 3-byte offsets); entropy coding (that is zstd's niche; misses the speed target).

## R4. Compressor design for ≥ 1.15× lz4 speed in safe code

**Decision** (starting design; tuned with `/perf goal` experiments after the first working version):
- Single-probe hash table of 2^14 `u32` entries (64 KiB, fits L1/L2), indexed by a multiplicative hash of 4 bytes read as a little-endian `u32` from an 8-byte `u64` load (`u64::from_le_bytes` on a fixed-size slice; one bounds check per load).
- Table reused across blocks without clearing: positions stored relative to a running base so stale entries fail the offset/verify check instead of needing a 64 KiB memset per block.
- Match verification with a 4-byte compare, then extension 8 bytes at a time (`(a ^ b).trailing_zeros() / 8`).
- LZ4-style acceleration: the step between probes grows with consecutive misses (`1 + misses >> 6`), so incompressible regions are skipped quickly.
- Output written into a buffer pre-sized to the worst-case bound with slice writes; literals copied with `copy_from_slice`/`extend_from_slice`.
- Hot loops written so the main loop runs only while ≥ 16 bytes remain (one bounds-check-friendly limit), with a simple tail loop.

Candidate experiments for the `/perf goal` phase (ranked by the perf planner then): 5- or 6-byte hashing (fewer false candidates on binary data), skip heuristic tuning, two-way probing, backward match extension, branch layout. Each is kept only if it improves speed without dropping the ratio below 2.18.

**Rationale**: These are the techniques that make LZ4 fast; the safe-Rust cost is bounds checks, which are minimised by fixed-size array conversions and loop limits LLVM can hoist. Beating C lz4 by 15% at a ratio within 5% is not guaranteed; the plan treats it as the goal of the tuning phase and records the trade-off if it can't be met (see Risks in plan.md).

**Alternatives**: `unsafe` pointer arithmetic (rejected by the user and the constitution); existing crates such as `lz4_flex` (new dependency, rejected by the request).

## R5. Decompressor design for ≥ 2,650 MB/s in safe code

**Decision**: Decode each block into a `Vec<u8>` reserved to the block's declared uncompressed size (never more). Literals via `extend_from_slice`. Matches: non-overlapping (`offset >= length`) via `Vec::extend_from_within`; overlapping (short offsets, runs) by repeatedly appending the growing pattern (`extend_from_within` of `min(offset_so_far, remaining)` bytes), which doubles the copied span each step. Every length and offset is validated against the bytes already produced and the declared size before copying.

**Rationale**: `extend_from_within` compiles to `memmove` without per-byte bounds checks, which is what gets safe Rust within reach of 60% of lz4's 4.4 GB/s. Validation before each copy satisfies FR-005 (no out-of-range back-references, no over-allocation).

**Alternatives**: Byte-by-byte copy loops (simple, ~3–5× slower on long matches); writing into a pre-zeroed output slice with index arithmetic (bounds checks on every write; zeroing cost).

## R6. Content-aware automatic selection

**Decision**:
- `CompressionAlgorithm` gains two provided (default-implemented) methods, so existing plugins compile unchanged:
  - `fn content_aware(&self) -> bool { false }` — the plugin is only eligible for automatic selection after an estimate;
  - `fn estimate_ratio(&self, input: &[u8]) -> Option<f64> { None }` — a cheap estimate of `input_len / compressed_len` from samples.
- `PluginSelector` gains `select_for_input(&self, input: &[u8])`:
  1. Content-aware plugins are removed from the candidate set unless the weights prefer speed (`throughput >= 0.8`; the CLI's `--level fast` is 0.9).
  2. If speed is preferred, each content-aware plugin's estimate is taken; it stays a candidate only if the estimate is ≥ 1.10.
  3. A content-aware plugin that survives step 2 is chosen; otherwise the existing weight-based scoring runs over the remaining (non-content-aware) plugins — today's behaviour.
- `compress_with_options` uses `select_for_input` when no plugin is named. Without a speed preference, step 1 removes `fast-lz` before scoring, so balanced and best choose exactly as before (FR-010).
- `fast-lz`'s estimate compresses evenly spaced samples: about 1/64 of the input, between 4 × 4 KiB and 16 × 16 KiB, so the cost stays ≲ 1.5% of compressing the whole input (FR-008, SC-005) and covers the whole input rather than its head.

**Rationale**: Keeps the policy in crush-core where every caller (library users and CLI) gets it, and generalises to future content-aware plugins (e.g. a dictionary or columnar codec) without special-casing names. Using the fast codec itself on samples is the most faithful predictor of its ratio and is cheap at this codec's speed.

**Alternatives**: Byte-entropy estimate (misses LZ-only redundancy and overrates base64 text: its 6-bit entropy looks compressible to a histogram but LZ4 can't exploit it without entropy coding — the exact case to reject); hard-coding `"fast-lz"` in the CLI (bypasses library users, not extensible); adding the plugin to normal weight scoring (would change balanced users' choice).

## R7. Interaction with the CLI's size-based routing

**Decision**: Phase 1 keeps the CLI rule that inputs ≥ 25 MB (and unknown-size stdin) go to `parallel-deflate`/`gpu-deflate`, for every level. Below the threshold with no plugin named, the CLI already defers to crush-core, which now applies `select_for_input`. Explicit `--plugin fast-lz` works at any size.

**Rationale**: `fast-lz` is single-threaded in this feature; on large inputs the multi-core DEFLATE engine (~1,000 MB/s on 24 threads) would be faster than one core, so routing large `--level fast` inputs to `fast-lz` would make them slower for users with many cores. Revisit when `fast-lz` gains multi-core compression (its independent blocks already allow it).

**Alternatives**: Route all `--level fast` inputs to `fast-lz` (regresses multi-core users on large files); add a thread-aware cost model (premature without multi-core `fast-lz`).

## R8. Story 3 — end-to-end CLI pipeline (phase 2)

**Decision**: Measure first, then remove the largest single-core costs found. Known costs on the compress path (single core, from feature 021's analysis): `fs::read` into a fresh buffer, plugin output buffer, outer-format assembly copy, CRC32 over the output, `fs::write`. Candidates, each verified with an ABBA end-to-end harness (extension of `perf/compare.py` restricted to one core):
1. Read regular input files through `memmap2` (already an allowed dependency and used by `crush-parallel::compress_file`) instead of `fs::read`, removing one full copy and the page-zeroing of a fresh buffer.
2. Let the plugin write directly after a reserved header prefix (removes the outer assembly copy for `fast-lz`).
3. Overlap CRC32 with output writing, or compute it per block while compressing.
4. Decompress: write blocks to the output file as they are decoded instead of materialising the whole result first.

**Rationale**: The ~614 MB/s ceiling with no compression work shows the pipeline, not the codec, limits end-to-end speed; these are the copies on that path.

**Constitution note**: Item 1 introduces an `unsafe` memory-map call in `crush-cli` (same pattern and SAFETY argument as the existing one in `crush-parallel`). It is listed in Complexity Tracking and requires the user's sign-off when Story 3 starts; if not approved, Story 3 proceeds with items 2–4 only and SC-008 may not be reachable.

**Alternatives**: Leave the pipeline as is (SC-008 unreachable: the pipeline alone is slower than `lz4`).

## R9. Benchmarks and fuzzing

**Decision**:
- `crush-lz/benches/codec.rs` (criterion): compress/decompress single-thread on (a) the synthetic `generate_corpus` text corpus, (b) incompressible random data, (c) a file corpus from `CRUSH_BENCH_CORPUS` when set (skipped otherwise), and the selection estimate on 1 MB / 16 MB / 128 MB inputs.
- `bench.py` suite `fast-lz` (crush-lz benches) for `/perf goal` A/B runs; `perf/lz-vs-lz4.sh` for the same-session comparison with `lz4 -b1` in WSL.
- `crush-lz/fuzz`: `fuzz_decompress` (arbitrary bytes into the decoder — must never panic or over-allocate) and `fuzz_roundtrip` (arbitrary input, plus inputs expanded to multi-block size so block boundaries and the acceleration path are hit). 100k iterations each, run under WSL (libFuzzer's ASan runtime isn't available on this Windows setup), with the crate's fuzz `Cargo.toml` carrying its own `[workspace]` table (the lesson from `crush-parallel/fuzz`).

**Rationale**: Constitution II/IV require fuzzing of all compression paths and criterion benchmarks; the WSL harness is how the codec targets are proven.
