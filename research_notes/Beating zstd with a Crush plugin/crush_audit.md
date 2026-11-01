# Crush audit: where Crush stands against zstd, and what a new plugin can exploit

Audit of branch `025-perf-lz-zero-fill` at b5d5141, 2026-10-31. All sources are repo files cited as `path:line`. No benchmarks were run; every number is copied from a committed file or computed from those numbers (computed values are labelled).

## Q1. Source of the user's numbers, and the comparison table

### Takeaway
The ratio claim is exact: zstd -3 3.70 vs Crush 3.503 on the `real` corpus at every core count. The speed claim "2.5 to 4 times faster" appears nowhere in the repo and is only loosely true. At the level that produces 3.70 (zstd -3) on real files, zstd compresses 1.41x to 2.41x faster than Crush. The 2.5x+ figures come from zstd -1, which compresses *looser* than Crush (3.233), or from the base64 `text` corpus. Decompression on real files is split: zstd wins on 1 core, Crush wins on 4 and all cores.

### Cited Findings
**Provenance**
- Source file: `perf/comparisons/wsl2-ubuntu-20.04-12th-gen-intel-core-i9-12900k-24t.json`. Header fields: date 2026-10-30, commit dc33c90, i9-12900K, 24 logical CPUs, Ubuntu 20.04, WSL2 kernel 5.15.167.4, crush 0.4.0, gzip 1.10, pigz 2.8, zstd 1.5.7, lz4 1.10.0, 128 MB, 5 reps, median, tmpfs, taskset; core sets `1`="0", `4`="0,2,4,6", `all`="0-23".
- A repo-wide grep for "2.5 to 4" and "2.5x" found no match outside `research_notes`/`reports`/`target` (which were excluded). 3.50 matches 3.503 at JSON line 308 and `perf/log/2026-10-29-random-compress.md:111-112`; zstd-3 is 3.7 in the JSON.
- Corpus `real` is `tar -cf - /usr/lib /usr/share | head -c 128MiB` — `perf/compare.py:46`. `text` is base64 of random bytes, 6 bits of entropy per byte — `perf/compare.py:41-42`. `repetitive` is `yes` output — `perf/compare.py:43`.
- Throughput divides by decimal MB (`size / 1e6`) of a 128 MiB file — `perf/compare.py:157,200`.
- Crush plugin/level in the `crush` row: `crush compress {i} -o {o} -f`, with no plugin or level — `perf/compare.py:85`. A 128 MiB file passes the 25 MiB threshold, so the CLI picks `parallel-deflate` — `crush-cli/src/algorithm.rs:8,48-53` — which is DEFLATE level 6, 1 MiB blocks, CRCs on, probe on — `crush-parallel/src/config.rs:63-77`. `--level` does not reach it (see Q3).
- `crush-fast-lz` is `compress --plugin fast-lz` and is measured at 1 core only — `perf/compare.py:77,87`.
- Thread counts: zstd compresses with `-T{threads}`, and its decompress command has no `-T` — `perf/compare.py:95-96`. lz4 is the same — `perf/compare.py:97-98`, though its decompress speed still rises with cores in the data (2,366-2,464 vs 1,634-1,699 on real). pigz gets `-p` both ways — `perf/compare.py:91-94`. All are pinned with taskset — `perf/compare.py:177`.

**Real files, MB/s (from the JSON)**

| tool | 1 core comp / decomp | 4 cores comp / decomp | all (24) comp / decomp | ratio |
|---|---|---|---|---|
| crush (parallel-deflate) | 130.1 / 774.7 | 456.5 / 1,697.0 | 1,023.8 / 1,999.1 | 3.503 |
| crush-fast-lz | 634.5 / 1,829.8 | not measured | not measured | 2.23 |
| zstd -1 | 273.9 / 1,153.3 | 1,467.0 / 1,315.1 | 2,641.7 / 1,307.1 | 3.233 |
| zstd -3 | 238.2 / 1,099.4 | 1,101.9 / 1,236.4 | 1,445.2 / 1,216.2 | 3.70 |
| lz4 -1 | 651.4 / 1,634.2 | 1,870.8 / 2,354.8 | 2,122.7 / 2,365.8 | 2.295 |
| lz4 -9 | 56.0 / 1,699.3 | 209.6 / 2,463.7 | 568.0 / 2,422.0 | 2.946 |
| pigz -6 | 44.3 / 330.3 | 167.1 / 344.6 | 615.9 / 343.9 | 3.481 |
| pigz -1 | 109.0 / 304.2 | 398.2 / 315.3 | 1,209.9 / 313.4 | 3.026-3.029 |
| gzip -6 | 44.3 / 247.5 | n/a | n/a | 3.474 |
| gzip -1 | 111.9 / 228.5 | n/a | n/a | 3.014 |

**Other corpora, MB/s comp / decomp (crush, zstd -1, zstd -3)**

| data, cores | crush | zstd -1 | zstd -3 | ratio crush / z1 / z3 |
|---|---|---|---|---|
| text, 1 | 114.8 / 471.0 | 557.0 / 992.5 | 511.0 / 984.3 | 1.324 / 1.327 / 1.327 |
| text, 4 | 387.9 / 1,072.1 | 2,150.8 / 1,141.1 | 1,782.2 / 1,119.3 | same |
| text, all | 802.2 / 1,200.1 | 2,903.4 / 1,130.2 | 2,019.7 / 1,134.3 | same |
| repetitive, 1 | 908.5 / 2,289.3 | 2,253.0 / 2,646.8 | 1,725.3 / 2,656.0 | 328.8 / 10,924 / 10,882 |
| repetitive, 4 | 2,462.4 / 2,964.2 | 5,748.0 / 2,643.4 | 4,384.8 / 2,618.3 | same |
| repetitive, all | 2,726.0 / 2,809.6 | 4,630.0 / 2,633.4 | 4,226.0 / 2,624.9 | same |
| random, 1 | 650.0 / 1,324.7 | 1,234.1 / 2,326.0 | 934.9 / 2,315.2 | 1.0 |
| random, 4 | 1,114.6 / 1,450.4 | 2,921.7 / 4,357.4 | 2,213.6 / 4,411.6 | 1.0 |
| random, all | 1,198.2 / 1,386.5 | 2,981.8 / 4,472.7 | 2,407.3 / 4,436.2 | 1.0 |

crush-fast-lz at 1 core: text 2,281.1 / 3,116.8 ratio 1.0; repetitive 5,213.6 / 3,038.2 ratio 251.4; random 2,472.6 / 3,342.0 ratio 1.0.

**Compress speed, zstd / crush (computed from the tables)**

| data | zstd -1: 1 / 4 / all cores | zstd -3: 1 / 4 / all cores |
|---|---|---|
| real | 2.11x / 3.21x / 2.58x | 1.83x / 2.41x / 1.41x |
| text | 4.85x / 5.54x / 3.62x | 4.45x / 4.59x / 2.52x |
| repetitive | 2.48x / 2.33x / 1.70x | 1.90x / 1.78x / 1.55x |
| random | 1.90x / 2.62x / 2.49x | 1.44x / 1.99x / 2.01x |

**Decompress on real, zstd -3 vs crush (computed):** 1 core zstd 1.42x faster; 4 cores crush 1.37x faster; all cores crush 1.64x faster. Against zstd -1: zstd 1.49x; crush 1.29x; crush 1.53x. fast-lz at 1 core decompresses 1.66x faster than zstd -3 at ratio 2.23.

**Quoted caveats**
- "These are wall-clock CLI numbers including file I/O and process start, so they aren't comparable with the criterion baselines above." — `perf/README.md:77-78`
- "zstd and lz4 are optional ... and use their own formats" — `perf/README.md:61-62`; "a reference point for speed and ratio, not a like-for-like DEFLATE comparison" — `perf/compare.py:8-11`
- Baselines "are **not** evidence for a performance claim on their own"; every claim compares two runs "in the same session on the same machine" — `perf/README.md:21-25`
- Run spread on real at all cores is large: crush 19.1%, zstd-1 22.7%, zstd-3 17.3% (`comp_spread_pct` in the JSON); "Run spread 15-34% (WSL)" — `perf/log/2026-10-29-random-compress.md:117`.
- Threads=4/8 swings "are machine variance on the 8P+8E CPU (they reversed sign between sessions)" — `perf/log/2026-10-29-random-compress.md:140-141`.
- The same CPU in a separate ABBA gave crush 992 MB/s and pigz -6 601 on real — `perf/log/2026-10-29-random-compress.md:112-113` (close to the JSON's 1,023.8 and 615.9).

### Inferences
- The user's sentence joins a speed from one zstd level to a ratio from another. The honest target pair is zstd -3 on real: 238 / 1,099 MB/s at 1 core, 1,102 / 1,236 at 4, 1,445 / 1,216 at all, ratio 3.70. Beating zstd -1's speed at 3.70 is a harder, different target.
- Crush's ratio deficit is small: 5.6% (3.70 / 3.503, computed). The speed deficit at 1 and 4 cores is the bigger gap.
- zstd's flat decompress speed across core sets (1.1-1.3 GB/s) fits its command having no `-T`. Crush's win on 4+ cores comes from independent blocks, not from a faster decoder: at 1 core it loses.
- On `random` at 4+ cores, zstd decompress is about 3x Crush's (1.76x at 1 core). Those are stored blocks, so Crush's limit there looks like its pipeline (Q4), not DEFLATE.
- fast-lz already beats zstd single-core decompression and doubles its compress speed, at ratio 2.23.

### Gaps
- Where the phrase "2.5 to 4 times" originated: not in the repo.
- Corpus composition: the head of a tar of `/usr/lib` then `/usr/share` differs per machine and is reused if present (`perf/compare.py:159-162`); no record of its file mix.
- The JSON has no confidence intervals and no ABBA (`"abba": false`).
- Is dc33c90 before or after the fast-lz zero-fill work? The JSON's fast-lz row (634.5 / 1,829.8) is close to the later log's 601 / 1,807 (`perf/log/2026-10-31-lz-zero-fill.md:77`), which were measured differently.

## Q2. Every Crush plugin

### Takeaway
There are four registered plugins, all DEFLATE-family or LZ4-class. None has a window larger than 64 KiB, and only DEFLATE paths have Huffman coding. No plugin has FSE/ANS or adaptive-table reuse across blocks, which is exactly the space zstd occupies.

### Cited Findings
| | deflate | parallel-deflate | gpu-deflate | fast-lz |
|---|---|---|---|---|
| magic id | 0x00 — `crush-core/src/plugin/default.rs:31` | 0x02 — `crush-parallel/src/lib.rs:46` | 0x03 — `crush-gpu/src/lib.rs:29` | 0x04 — `crush-lz/src/lib.rs:13` |
| crate/backend | `flate2` zlib feature, level default — `default.rs:46`, `Cargo.toml:32` | `libdeflater` (C libdeflate via `libdeflate-sys` in `Cargo.lock:1017,1026`) — `crush-parallel/src/block.rs:6` | own Rust: GDeflate — `crush-gpu/src/gdeflate.rs:1-12`; wgpu, optional CUDA — `crush-gpu/src/lib.rs:35-45` | own safe Rust — `crush-lz/src/compress.rs:1-9` |
| block size | whole input as one stream | 1 MiB default, 64 KiB-256 MiB — `config.rs:68,165` | 64 KiB tiles — `crush-gpu/src/format.rs:25` | 1 MiB default, 2^16..2^20 — `crush-lz/src/format.rs:22-26` |
| window | DEFLATE (32 KiB) | DEFLATE; each block fresh (libdeflate stateless per call — `specs/011-perf-optimizations/research.md:11`) | 32,768 — `crush-gpu/src/lz77.rs:65,73` | 64 KiB, matches stay within block — `.specify/memory/CLAUDE.md` fast-lz format; `compress.rs:21` |
| match finder | zlib | libdeflate level 6 — `config.rs:69` | hash chains — `lz77.rs:55,94` | greedy, single probe, 2^12 u32 table, 6-byte hash, step grows on misses, backward extension — `compress.rs:18-23,89-116,133-136` |
| entropy | Huffman | Huffman | fixed DEFLATE Huffman across 32 round-robin sub-streams — `gdeflate.rs:9-12` | none — `crush-lz/src/lib.rs:3-5` |
| threading | single-thread | rayon, `map_init` per-worker state — `crush-parallel/src/engine.rs:50-61` | GPU dispatch, CPU fallback — `gdeflate.rs:14`; compress is CPU-side — `README.md:129` | single-threaded — `perf/compare.py:77`, `specs/023-lz4-fast-plugin/research.md:87` |
| format | Crush outer header + raw DEFLATE | CRSH: 64 B header, 16 B block headers, trailing index 20 B per entry, 24 B footer, per-block CRC32, stored flag — `.specify/memory/CLAUDE.md` CRSH section; `engine.rs:79-94` | CGPU: 64 B header, 32 B tile headers padded to 128 B, index 24 B per entry, footer — `crush-gpu/src/format.rs:5-12` | frame "LZ", blocks `u32 clen / u32 ulen`, end marker — `crush-lz/src/format.rs:5` |
| metadata (MB/s, ratio) | 200, 0.35 — `default.rs:33,35` | 500, 0.65 — `crush-parallel/src/lib.rs:61-62` | 2000, 0.65 — `crush-gpu/src/lib.rs:103-104` | 900, 0.44 — `crush-lz/src/lib.rs:162-163` |
| random access | no | yes, `--block` — `crush-cli/src/commands/decompress.rs:21-51` | tile index in format | blocks independent; no index, frame is walked — `crush-lz/src/decompress.rs:132-177` |

- fast-lz outcome: 0.91x lz4 compress, 0.62x decompress in memory; 2.230 vs 2.296 ratio — `perf/log/2026-10-31-lz-zero-fill.md:36-38`.
- End to end it is ahead of the lz4 CLI: 1.06x compress, 1.24x decompress — `perf/log/2026-10-31-lz-zero-fill.md:75`.
- GPU numbers: decompress 560 MiB/s CUDA, 386 wgpu, 179 CPU on an RTX 3060 Ti — `README.md:121-127`; compress 69-179 MiB/s — `README.md:131-136`.

### Inferences
- The GPU path is no asset against zstd throughput today: its best published decompress (560 MiB/s, different corpus) is under half of single-threaded zstd's CLI rate on real files, and its metadata ratio is 0.65.
- Metadata values are hand-entered constants and are stale: parallel-deflate declares 0.65 and 500, while the JSON shows ratio 3.503 (0.285) on real.
- The only Huffman encoders found in Rust here are fixed-code; a dynamic entropy coder would be new code. (Based on the crates read; gpu `gdeflate.rs` was checked only in its header comments.)

### Gaps
- `crush-gpu/src/engine.rs` threading was not audited.
- `crush-lz` window limit "matches stay within their block" is from the notes file and `compress.rs` (`c` is an offset into `src`, one block); the contract file was not separately read.

## Q3. The plugin contract

### Takeaway
The contract does not force independent blocks. A plugin receives the entire input as a slice and returns an opaque payload, so a cross-block window, shared tables, and an embedded dictionary all fit with no trait change. What is missing is configuration (no level, thread count, or dictionary parameter), streaming input, and a random-access method. Auto-selection is the sharper obstacle: the CLI sends files of 25 MiB or more to `parallel-deflate` before the selector runs.

### Cited Findings
**Must implement** — `crush-core/src/plugin/contract.rs:75-239`
- Required: `name`, `metadata`, `compress(&[u8], Arc<AtomicBool>) -> Result<Vec<u8>>`, `decompress(same)`, `detect` — lines 77, 80, 100, 169, 218.
- Provided, overridable: `compress_into` (default copies, line 112-121), `streams_compressed_output` + `compress_to` (lines 126-151), `decompress_to` (default materialises, lines 184-193), `content_aware`, `estimate_ratio` (lines 225-238).
- Rules: never panic, validate input, return `PluginError::Cancelled`, no header in the payload — lines 17-22, 93.
- `decompress_to` doc: "Plugins that decode in independent pieces override it" — line 175; `compress_to`: likewise, lines 134-136. Independent pieces are the suggestion for streaming, not a requirement.

**Metadata and magic**
- `PluginMetadata`: name, version, 4-byte magic, throughput, `compression_ratio` (compressed/original, range (0,1]), description — `metadata.rs:8-29`.
- Magic layout `[0x43, 0x52, version, plugin_id]`; version must be 0x01 — `metadata.rs:15-17,79-80,115-117`.
- Outer format: 16 B header (magic, u64 size, flags, reserved), then 4 B CRC, then optional u16 + TLV metadata — `metadata.rs:31-46`, `crush-core/src/compression.rs:349-362`. TLV types today: mtime 0x01, permissions 0x02; unknown types ignored — `metadata.rs:280-308`.
- Used ids: 0x00, 0x02, 0x03, 0x04; "0x10/0x11 appear only in selector tests" — `specs/023-lz4-fast-plugin/research.md:24`. Tests also use 0x05, 0x06, 0x7F (`selector.rs:453-454`, `contract.rs:256`), not at runtime.
- Registry: compile-time `linkme` slice `COMPRESSION_ALGORITHMS` — `crush-core/src/plugin/mod.rs:36-37`; a duplicate magic logs and keeps the first — `registry.rs:74-86`; validation rejects empty name/version, throughput <= 0, ratio outside (0,1] — `registry.rs:45-71`.
- Decompress routing is by magic from the header — `crush-core/src/decompression.rs:223`; `detect` isn't used for that — `contract.rs:203-204`.

**Selection**
- Score: `w_t * norm(ln throughput) + w_r * norm(ratio)`, min-max over candidates — `selector.rs:127-168`. Default 0.7/0.3 — `selector.rs:68-73`.
- CLI levels: fast 0.9/0.1, balanced 0.5/0.5, best 0.1/0.9 — `crush-cli/src/cli.rs:331-346`.
- Content-aware plugins are excluded from scoring; used only with throughput weight >= 0.8 and estimate >= 1.10, best estimate wins — `selector.rs:173-177,181-186,251-266`.
- Estimate budget: about 2% of compress cost — `contract.rs:233-234`.
- CLI order: explicit plugin, stdin → parallel-deflate, >= 25 MiB → gpu or parallel, else `"default"` → selector — `crush-cli/src/algorithm.rs:43-53`. README: "Inputs of 25 MB or more and stdin keep the parallel engine." — `README.md:215`.
- 023 R7 says to revisit "when `fast-lz` gains multi-core compression" — `specs/023-lz4-fast-plugin/research.md:87`.

**Fit for a zstd-like plugin**
- Cross-block window: `compress` gets all input at once — `contract.rs:100`; the payload is opaque to the library — `contract.rs:93`.
- Shared tables or a dictionary in the payload: allowed for the same reason.
- No parameter for level, dictionary, or threads in any method — `contract.rs:100,169`. crush-gpu works around this with a process-global `OnceLock` — `crush-gpu/src/lib.rs:62-72`.
- Whole-input model: `compress_stdin` reads everything first — `crush-cli/src/commands/compress.rs:59-60`; `compress_stream` too, "deferred to feature 012-streaming-pipeline" — `crush-parallel/src/engine.rs:248-256`.
- `--block` random access is hard-wired to crush-parallel, not a trait method — `crush-cli/src/commands/decompress.rs:41,51`.

### Inferences
- A new plugin can ship in its own crate without touching the trait, exactly as crush-lz did (`research.md:24-28`), linked through `crush-cli/Cargo.toml:20-23`. Next free production id: 0x01 or 0x05 (no use of 0x01 found).
- To be default on large files it must change `crush-cli/src/algorithm.rs`. Otherwise it only runs with `--plugin`.
- Registering it as a non-content-aware plugin changes default selection through the min-max. Computed by hand with current metadata at balanced 0.5/0.5: deflate 0.5, gpu-deflate 0.5, parallel-deflate 0.2 — a tie that the name sort settles in favour of `deflate` (`selector.rs:281-285`). At library-default 0.7/0.3 gpu-deflate scores 0.7 vs 0.3. Those stale constants make scoring a weak foundation; `content_aware` only participates at >= 0.8, so balanced-level content-aware selection would need a contract or selector change.
- `estimate_ratio` needs a samples-based estimate; a new codec can copy `crush-lz/src/estimate.rs`.

### Gaps
- Not run: actual `select()` output. The scores above are hand arithmetic.
- `run_with_timeout` scoped variants beyond line 120 of `timeout.rs` were not read in full.

## Q4. Architectural assets and liabilities

### Takeaway
Real assets: independent-block parallel decompression, the O(1) index, per-file plugin choice, and memory-mapped I/O plus streaming hooks. Liabilities for a new plugin: `crush-parallel` is hard-wired to libdeflate rather than being a generic engine, its decompress path copies payloads serially, and there is no memory pool despite the constitution. Timeouts and per-block metadata are not meaningful costs.

### Cited Findings
**Assets**
- Parallel decompress into pre-partitioned output slices, no per-block output allocation — `crush-parallel/src/engine.rs:351-410`, `block.rs:223-225`. It's what wins 4+ core decompression (Q1).
- Per-worker reuse via `map_init` — `engine.rs:47-55,392-397`. `Hasher::combine` parallel CRC — `crush-core/src/compression.rs:384-399`.
- Random access: about 1.14 ms per block on 64 MB, 12 ns per index lookup — `README.md:138-151`. O(1) because absolute offsets are stored — `specs/007-parallel-gzip-engine/research.md:393-395`.
- Incompressible probe: `compress_incompressible/threads=1` -87.1% — `perf/log/2026-10-29-random-compress.md:93`. Stored blocks borrow input — `block.rs:92-101`.
- Plugin per file through magic routing — `decompression.rs:223`; content-aware selection, 18/18 correct routing — `specs/023-lz4-fast-plugin/perf-log.md:215`.
- Memory-mapped input with `populate()` and streamed output for plugins that opt in — `specs/023-lz4-fast-plugin/perf-log.md:257-260,285-291`. These took fast-lz from 0.90x / 0.83x of the lz4 CLI to 1.06x / 1.24x — `perf/log/2026-10-31-lz-zero-fill.md:75,82`.
- Cooperative cancellation: fast-lz checks per block — `crush-lz/src/compress.rs:288-290`, `decompress.rs:163`.
- Decoder hardening exists: bounded allocation from forged headers via `try_reserve_exact` — `perf/log/2026-10-31-lz-zero-fill.md:188-193`, `crush-lz/src/decompress.rs:39-50`.

**Liabilities**
- `crush-parallel` calls libdeflater directly — `block.rs:6`, `engine.rs:10`; `FileHeader::new` takes block size, level, flags, sizes, count, no codec field — `engine.rs:79-85`. A new codec can't use the engine without refactoring.
- Compress buffers everything: `collect`, then `vec![0u8; total_size]`, then parallel copy — `engine.rs:50-61,96,126-133`.
- Plugin output is copied once more: parallel-deflate doesn't override `compress_into`, so the default extends — `contract.rs:118-119`. Nor does it stream — `compression.rs:218-221`.
- Decompress phase 1 reads every block sequentially into its own `vec!`, through a `Cursor` over an in-memory slice — `engine.rs:281-282,332-349`. Then `vec![0u8; total_usize]` — `engine.rs:357`. Plugin has no `decompress_to` override, so the whole result is materialised, then written — `contract.rs:190-192`, `crush-parallel/src/lib.rs:80-90`.
- Outer CRC on decompress is serial over the compressed payload — `decompression.rs:40-43`; compress is parallel — `compression.rs:384`. Block CRCs add a second CRC, over uncompressed data — `block.rs:72-76,271-280`.
- Per-block allocation on compress: a compressed block takes `scratch`, and the worker callocs a new one — `block.rs:85-90,132-136`. The reuse variant was tried and regressed: copying out cost more — `perf/log/2026-10-29-random-compress.md:41-44`.
- Fresh-output page faults: about 9.5 ms per 128 MiB; memset removal mostly moved cost to faults — `perf/log/2026-10-31-lz-zero-fill.md:128-139`.
- No memory pool: no `pool.rs` in `crush-core/src`, no pool type in the crates (only rayon `with_pool`) — `crush-parallel/src/engine.rs:21`; `.specify/memory/constitution.md:29` requires pooling; the structure in `CLAUDE.md` lists `pool.rs`, `engine.rs`, `stream.rs`, `block.rs`, none of which exist in `crush-core/src`.
- Cancellation in parallel-deflate happens only in a post-pass through the progress callback, after all blocks finish — `engine.rs:169-188`, `crush-parallel/src/lib.rs:73`.
- Random-access exclusive to parallel-deflate — `crush-cli/src/commands/decompress.rs:51`.
- `crush-parallel` plugin builds a fresh config per call — `crush-parallel/src/lib.rs:74-76`; this is small for one file.

**Not meaningful costs**
- Timeout: default 0 means no timeout; one scoped thread per operation — `compression.rs:20,150-155,330-336`. Process start 1.73 ms vs lz4 1.31 ms — `specs/023-lz4-fast-plugin/perf-log.md:278`.
- Size overhead (computed): 128 blocks × 36 B (16 header + 20 index) plus headers is about 4.7 KB per 128 MiB. The real block cost is compression loss: 128 KB "cold-start penalty ... 7–10%", under 3% at 1 MB — `specs/007-parallel-gzip-engine/research.md:389`.

### Inferences
- Crush's asset and zstd-class ratios are in tension. A window across blocks makes a block depend on earlier output, which breaks parallel decode and random access: the two things that win today. The preserving options are larger independent blocks (bounded by 32 MB per thread) or shared read-only tables/dictionary at the head of the payload.
- Compress can still use prior-block data without changing decode independence only if decode is allowed to be sequential; that trade gives up the 4+ core decompress win.
- A generic block engine extracted from crush-parallel (codec trait over block compress/decompress into slice) is the highest-leverage refactor. A plugin writing into pre-partitioned slices inherits all the engine's assets.
- The decompress phase-1 copy is probably why Crush's `random` decompress plateaus near 1.4 GB/s while the stored path should be a memcpy. Not proven: no profile of this path exists.

### Gaps
- No measurement isolates phase-1 copy, serial outer CRC, or materialised output on parallel-deflate.
- Windows criterion baseline (`perf/baselines/windows-12th-gen-intel-core-i9-12900k-16c24t.json`, commit b464204, 2026-10-29): `compress_throughput` 1.345 s at 1 thread vs 113 ms default, decompress 144.6 ms vs 20.7 ms (lines 128-129, 224-225, 357-358, 389-390). This is about 11.9x and 7.0x scaling (computed), not a zstd comparison.

## Q5. Where Crush's time goes

### Takeaway
Recent profiling covers fast-lz only: match copy is at parity with lz4, and the remaining gaps are fresh-buffer page faults and code around accepted matches. No profile exists for parallel-deflate, which is the path measured against zstd. The `fast_lz_select` slowdown is binary placement, not code.

### Cited Findings
- Decompress (before the fix), 128 MB one core: fast-lz 53.3 ms vs lz4 30.3: zero-fill +12.3, `shortcuts` entry/exit +7.1, literals +4.0, match copy +0.5, tokens -1.0 — `perf/log/2026-10-31-diagnose-decompress-and-decoder.md:53-61`.
- "Match copy is level with lz4." — same file line 67. Two-loop design costs about 7 ms — line 69.
- Compress: fast-lz 185.6 vs lz4 171.2: search -5.1, landing/back-extension +11.2, match count -1.1, emit +2.7, zero-fill +7.2 — same file lines 83-90.
- "SC-001 can't come from these findings; going beyond lz4 would need a different trade-off." — same file lines 97-99.
- After the fix, memset fell from 24.6% to 0.0%, but about 76% of that time reappeared in the decoder as faults — `perf/log/2026-10-31-lz-zero-fill.md:128-131`. Removing faults "would need a reusable output buffer, which is out of scope" — lines 138-139.
- Earlier frontier: going further "would need unchecked copies/loads (`unsafe`) or a different algorithm" — `specs/023-lz4-fast-plugin/perf-log.md:188-189`. Before pipeline work, read 22 ms, codec+core 202, write 15.7, total 240 — lines 264-268.
- `fast_lz_select` diagnosis (only on this local branch, commit b5d5141; memory `fast-lz-select-placement-finding.md`): "instructions flat, time up, same functions and shares, so placement of unchanged code or data" — `perf/log/2026-10-31-lz-zero-fill.md:215-216`. 383,228 instructions per call on both sides; identical disassembly — lines 218-222. Windows rotation at `estimate/1MB`: afc4bad 24.11 µs, control -2.1%, 2220227 (benchmarks only) +30.7%, db29789 +8.2% — lines 229-234. "Alignment within a 64-byte line does not explain it"; mechanism unidentified — lines 239-244.
- Similar layout effect: `decompress_medium/1MB` +4-5% from `inflate_fast` moving within the binary, "Nothing to fix in Crush" — `perf/log/2026-10-31-diagnose-decompress-and-decoder.md:13-14,24-29`.
- Noise: A/A of 1-3%; floors 5.6-12.6% at threads=8 — `diagnose` log line 19; `perf/log/2026-10-29-random-compress.md:130-131`.

### Inferences
- Microbench A/Bs on hot compress code can move by 30% with no code change, so a head-to-head on small inputs isn't trustworthy; use 128 MB rows and ABBA.
- Since DEFLATE level 6 at 130 MB/s per core is the main compress cost and it's C libdeflate, per-block pipeline tuning can't close a 1.8x-2.4x gap. A different codec is needed, consistent with the plugin premise.

### Gaps
- No `samply`/`perf` profile of `parallel-deflate` CLI compress or decompress, and no `CRUSH_TIMINGS` run for it. The diagnose log confirms external comparison "still took manual work" — lines 109-110.
- Why page faults cost about 9.5 ms is untested — `perf/log/2026-10-31-lz-zero-fill.md:262`.

## Q6. Constitution constraints

### Takeaway
A safe-Rust port or new codec needs no amendment. A zstd C binding is not banned outright but isn't on the allowed list, and earlier research said zstd needs an amendment. The allowed list is already out of date: C-backed `libdeflater` ships in the default path without being listed. Decoders are expected to be safe Rust unless `unsafe` is "absolutely necessary".

### Cited Findings
- Live constitution version is 1.6.0, last amended 2026-02-07 — `.specify/memory/constitution.md:487`. `CLAUDE.md` still says 1.0.0.
- Performance First: "Use zero-copy operations", "Apply memory pooling", benchmarks not speculation — `constitution.md:24-31`.
- Safety: "100% memory safe Rust code (no `unsafe` unless absolutely necessary and extensively documented)"; no `.unwrap()`/`.expect()`; input validation; fuzz; property tests — `constitution.md:39-44`.
- Modularity: "ZSTD, LZ4, Brotli, and custom algorithms should be straightforward to add without core engine changes." — `constitution.md:59`. Plugins independently testable; configuration via builders — `constitution.md:55-57`.
- TDD; 100k fuzz iterations per release — `constitution.md:63-71`.
- Allowed: rayon, flate2 "(Phase 1 only, replace in Phase 2)", crc32fast, memmap2, thiserror, clap, criterion, cargo-husky — `constitution.md:83-90`. "Every dependency MUST be justified" — line 79. Prohibited: async runtimes, heavy frameworks, deprecated, vulnerable — lines 94-97.
- Nothing in the constitution mentions C, FFI, or `-sys` crates (read in full).
- De facto: `libdeflater` (`Cargo.toml:33`, `crush-parallel/Cargo.toml:19`), `flate2` with `zlib` (`Cargo.toml:32`), `linkme`, `crossbeam`, `wgpu`, `bytemuck`, `pollster`, `cudarc` are present; `libdeflater`, `linkme`, `crossbeam` are unlisted. `libz-sys` had an advisory (RUSTSEC-2026-0418) — `perf/log/2026-10-29-random-compress.md:121-123`. 011 spec: libdeflater "remains the DEFLATE implementation for Phase 1" — `specs/011-perf-optimizations/spec.md:108`.
- Existing `unsafe`: mmap in `crush-parallel/src/engine.rs:211` and `crush-cli/src/commands/utils.rs` ("the one `unsafe`" — `.specify/memory/CLAUDE.md`), plus `crush-gpu/src/backend/cuda.rs:66,209,351`. The CLI mmap needed explicit user sign-off — `specs/023-lz4-fast-plugin/research.md:101`, `perf-log.md:257`. No `forbid(unsafe_code)` or `deny(unsafe_code)` was found in the crate roots or manifests; `unwrap_used = "deny"` is in crate manifests — `crush-core/Cargo.toml:56`, `crush-parallel/Cargo.toml:41`, `crush-gpu/Cargo.toml:49`.
- 023 rejected `unsafe` pointer arithmetic ("rejected by the user and the constitution") and `lz4_flex` ("new dependency, rejected by the request") — `specs/023-lz4-fast-plugin/research.md:56`.
- Targets: within 5% of gzip single-threaded, match pigz on 4+ cores, "< 32MB per thread for default block size", "> 500 MB/s compression on modern 8-core CPU" — `constitution.md:320-323`. No zstd target exists.
- Gates: tests, clippy, coverage > 80%, < 5% benchmark regression, docs, fuzz 100k, miri/valgrind for unsafe, task checklist — `constitution.md:103-110`. Post-MVP cleanup with jscpd, no duplication > 20 lines — `constitution.md:116-145`. Claims "MUST be backed by criterion benchmarks" — line 327.
- Amendment: proposal, minimum 7 days of feedback, maintainer approval, migration plan, version bump — `constitution.md:462-466`. Agents "MUST implement ONLY features explicitly specified" — line 445.

### Inferences
- A C `zstd` binding: precedent says it can pass (libdeflater), but the plain reading of 007's research says amend first, and it wouldn't be a Crush codec. It would also make "beat zstd" circular.
- A Rust port or original codec faces safe-Rust speed limits already measured: fast-lz at 0.62x lz4 in-memory decode, with `unsafe` named as the next step (`perf-log.md:188-189`). Reaching zstd's decode speed in safe Rust isn't shown by any data here. Note zstd's rate is far lower than lz4's (1,100-1,300 vs 1,634+ CLI on real), so the bar is lower than fast-lz's was.
- 32 MB per thread is workable for a window of several MiB; the phrase "for default block size" gives latitude, though it's a Phase 1 target.
- The list needs a cleanup amendment regardless; pairing it with this feature gives the plugin a clean constitutional footing.

### Gaps
- No file states the project's position on C dependencies. The maintainer's intent is needed.
- Whether `FormatHandler` and `ChecksumAlgorithm` traits from `constitution.md:53` exist was not checked.

## Q7. Prior research and next feature number

### Takeaway
zstd was deferred, never rejected on merit: out of scope for gzip-comparable Phase 1 because of the dependency list, and later because entropy coding would miss 023's speed target. The next feature number is 026.

### Cited Findings
- 007: "Zstd is not in the allowed list and would require a constitution amendment... zstd would give better ratios but is architecturally out of scope for this phase." SC-006 required output within 5% of gzip — `specs/007-parallel-gzip-engine/research.md:383-385`.
- 007 borrowed from zstd's seekable format: trailing index design — `research.md:123-139,177-182`; absolute offsets instead, for O(1) — `research.md:393-395`.
- 007: 1 MiB chosen over 128 KB; "no shared dictionary" cold-start 7–10% at 128 KB, under 3% at 1 MB — `research.md:389`.
- 023 reference numbers, in memory, one core: zstd -1 580 / 1,788 MB/s, 3.233; lz4 -1 788 / 4,417, 2.296 — `specs/023-lz4-fast-plugin/research.md:11-14`.
- 023 rejected: bit-exact LZ4 frame, "larger window (better ratio, slower, 3-byte offsets)", "entropy coding (that is zstd's niche; misses the speed target)" — `research.md:40`.
- 023 generalises hooks to "future content-aware plugins (e.g. a dictionary or columnar codec)" — `research.md:79`. Its CLI overhead: pipeline "caps one core at ~614 MB/s" (later reduced) — `research.md:20`, `spec.md:52`.
- 004: zstd appears as a trait example (`research.md:480`), an external-sequence-producer API (`research.md:770`), and QAT plugin link (`research.md:849`); no decision on zstd. Plan listed RFC 8878 study — `specs/004-plugin-system/plan.md:286,305`. 004 completion lists "Additional plugins: LZ4, Zstd, Brotli (future features)" — `specs/004-plugin-system/IMPLEMENTATION-COMPLETE.md:368`; 005 "Implement ZSTD plugin for ratio" — `specs/005-cli-implementation/COMPLETION_SUMMARY.md:235`.
- 003: 4-byte magic is an industry standard — `specs/003-rust-workspace-setup/research.md:1029`; CLI contract once listed `--algo deflate, brotli, zstd` — `specs/003-rust-workspace-setup/contracts/cli-interface.md:206`.
- Spec dirs on disk: 001-011 and 023. Local branches run to `025-perf-lz-zero-fill`; remote has `024-perf-profiling`. `Get-NextBranchNumber` takes the highest of branches, remote refs, and specs — `.specify/extensions/git/scripts/powershell/create-new-feature-branch.ps1:153-171`; `branch_numbering: sequential` — `.specify/extensions/git/git-config.yml`.

### Inferences
- `specs/` undercounts: use 026 (`026-...`). `/speckit-git-feature` will compute it.
- The speed objection in 023 was against an lz4-beating goal. Against zstd -3, it doesn't apply: a plugin aiming at 240+ MB/s per core has about 2.6x the time budget of fast-lz at 634 MB/s (computed).
- A zstd in-memory figure of 1,788 MB/s vs CLI 1,153 MB/s shows pipelines matter on both sides; CLI head-to-heads favor Crush's mmap design.

### Gaps
- No spec has evaluated an entropy coder, a large-window LZ, or dictionaries for Crush. Nothing in research prices them.

## Q8. Benchmark tooling for a head-to-head gate

### Takeaway
The tooling is enough with small additions: `perf/compare.py` is the external gate and `bench.py` the regression gate. A new plugin needs one row in `compare.py`, one suite in `bench.py`, and an adaptation of `perf/lz-vs-lz4.sh` for in-memory `zstd -b3`.

### Cited Findings
- `compare.py`: ABBA, subsets, round-trip `cmp` checks, median, `comp_spread_pct` — `perf/compare.py:18-23,143-144,186,199-207`. Run: `python3 perf/compare.py --cores 1 --tools crush-fast-lz,lz4-1 --data real --abba --reps 10` — line 23. Subset runs write JSON only with `--out` — lines 234-235.
- Adding a plugin: entry in `tools()` — lines 84-99; `SINGLE_THREADED` — line 77. Linux only — line 150. On Windows use WSL, with tools built into `~/.local/bin` — `perf/README.md:62-70`. zstd and lz4 are optional and silently skipped if absent — `perf/compare.py:79,100-101`.
- `perf/lz-vs-lz4.sh`: same session, one core, in memory, exits 1 on a miss; SC thresholds hard-coded; uses `crush-lz` example `codec_bench` — lines 9-10, 24-28, 49-56.
- `bench.py`: `ab` (A B B A, bootstrap 95% CI), `noise`, `ceiling`, `profile`, `profile-diff`, `callgrind`, `save-summary` — `.claude/skills/perf/bench.py:10-51`. Suites and crate mapping — lines 119-149. A new crate must be added to `PERF_SRC`, `HOT_RE`, `SUITES`, `CRATE_SUITE`, `CALLGRIND_CASES` — lines 102-108, 119-172.
- Regression: significant slowdown above 5% is `REGRESSION` — `perf/README.md:48-50`. Benches under 100 µs are advisory — `bench.py:94-98`.
- `CRUSH_TIMINGS=1` prints per-phase times — `perf/README.md:98-100`.
- Fuzz harness pattern with own `[workspace]`, run in WSL — `specs/023-lz4-fast-plugin/research.md:110`.
- After-implement gate is mandatory: `/speckit-crush-perf-gate` — `CLAUDE.md`, Extensions and Hooks.

### Inferences
- Recommended gate for a plan: `compare.py --tools crush-<new>,zstd-3,zstd-1 --data real --abba --reps 10` at `--cores 1`, `4`, `all`, with ratio and both directions, pass criteria stated against the live zstd run rather than the JSON (the 023 lesson: "0.60x governs, not 2,650 MB/s", `perf/log/2026-10-31-lz-zero-fill.md:22`).
- `real` alone is one corpus from `/usr/lib` head. A plan should add text-like or log data; the `text` corpus is base64, which only tests entropy coding (ratio 1.32 for all).

### Gaps
- No criterion bench runs an external tool; the constitution's "criterion" wording (`constitution.md:327`) doesn't fit head-to-heads, so a plan should say so explicitly.
