# Cleanup Summary — 023-lz4-fast-plugin

**Feature**: 023-lz4-fast-plugin | **Started**: 2026-10-29

Records, per user story, what shipped, what was removed or consolidated,
and what was deferred. Extended as each story finishes.

## US1 — Fast plugin on request (P1, MVP)

### What shipped

- New crate `crush-lz` with the `fast-lz` plugin (magic `43 52 01 04`),
  registered through `linkme`; format in
  [contracts/fast-lz-format.md](contracts/fast-lz-format.md).
- Encoder ([crush-lz/src/compress.rs](../../crush-lz/src/compress.rs)):
  greedy LZ77, 2^12-entry table indexed by a 6-byte hash, skip
  acceleration, output written into a pre-sized slice, early fallback to a
  stored block.
- Decoder ([crush-lz/src/decompress.rs](../../crush-lz/src/decompress.rs)):
  validating, fixed-size copies into a pre-sized slice, an out-of-line
  shortcut loop for the common sequence shape.
- crush-core: `content_aware()` / `estimate_ratio()` on the plugin contract;
  the default selector excludes content-aware plugins.
- CLI: `--plugin fast-lz`, inspect and decompress by magic.
- Tests: round trips and property tests, hand-built decoder rejects and copy
  cases, stored-threshold test, CLI tests; fuzz targets `fuzz_decompress`
  and `fuzz_roundtrip`.
- Benchmarks: criterion `crush-lz/benches/codec.rs` (`bench.py` suite
  `fast-lz`), `crush-lz/examples/codec_bench.rs`, `perf/lz-vs-lz4.sh`.

### Tuning outcome (T023)

Five experiments, see [perf-log.md](perf-log.md). Final, same session as
lz4 1.10.0: compress 0.92x (SC-001 not met), decompress 0.57x (SC-002 not
met), ratio 2.230 (SC-003 met). Targets unchanged; frontier recorded (user
decision).

### Duplications / dead code

- `detect-duplicates.ps1 -Path crush-lz`: 0 clones (2026-10-29).
- Hand-built frame helpers shared by two decoder test files moved to
  `crush-lz/tests/common/mod.rs` instead of being copied.
- Removed during tuning: the `Vec`-based emit helpers (`push_length_ext`,
  `emit_*` on `Vec<u8>`), the 4-byte table hash.

### Deferred

- Decoder tuning beyond 0.57x needs a sampling profiler (none installed:
  samply missing, WPR needs elevation, no perf in WSL) or unchecked copies.
- lz4's search-loop structure: measured no gain in safe Rust (experiment 4).

## US2 — Automatic selection uses the fast plugin only when it pays off (P2)

### What shipped

- crush-core ([selector.rs](../../crush-core/src/plugin/selector.rs)):
  `PluginSelector::select_for_input(input)`; with speed preferred
  (`SPEED_PREFERRED_THROUGHPUT` = 0.8) content-aware plugins are estimated
  and the best one at `MIN_ESTIMATED_RATIO` (1.10) or more is chosen,
  otherwise the scoring choice is unchanged. The decision is a pure
  `choose_for_input` so tests drive it with estimate doubles.
  `compress_with_options` uses it when no plugin is named.
- crush-lz ([estimate.rs](../../crush-lz/src/estimate.rs)): sampled estimate,
  about 1/128 of the input (4 x 4 KiB to 16 x 16 KiB), per-thread compressor.
- CLI: no routing code needed (`--level fast` maps to throughput 0.9 and
  inputs < 25 MB already defer to crush-core); docs in `algorithm.rs`; a
  test keeps the level presets in step with the selector threshold.
- Tests: selector unit tests, estimate unit and integration tests, CLI
  routing tests (verified to fail without the crush-core change), selection
  report (`#[ignore]`), criterion `fast_lz_select` benches.

### Changes to the plan

- Sample budget 1/128 instead of 1/64 (cost measured 2.02% at 16 MB).
- Estimate accuracy: conservative bound (at most 5% above, at least 75% of
  the real ratio) instead of "within 10%"; samples lack block history.
- Task T026's first sample-size formula (`input_len / n`) contradicted
  research R6 and was corrected.

### Duplications / dead code

- `detect-duplicates.ps1`: 0 clones in `crush-lz` and `crush-core/src/plugin`.
- Test-data generators (`xorshift`, `base64`, `log_text`) shared through
  `crush-lz/tests/common/mod.rs`. The CLI test keeps its own small copies
  (separate crate).

## US3 — End-to-end single-core speed through the CLI (P3)

### What shipped

- Byte-identity goldens and CLI == library checks
  (`crush-cli/tests/pipeline_identity.rs`), `CRUSH_TIMINGS=1` phase timings,
  `perf/compare.py` subsets and `--abba`.
- crush-core plugin hooks, all with defaults that keep the old behaviour:
  `compress_into` (payload after the header prefix, no assembly copy),
  `streams_compressed_output` + `compress_to` with
  `crush_core::compress_to_writer` (CRC patched by seeking back), and
  `decompress_to` with `crush_core::decompress_to_writer` (CRC checked
  before the first write). fast-lz implements all of them.
- crush-cli: input files of 1 MB or more memory-mapped with `populate()`
  (the one `unsafe`, user-approved, in `commands/utils.rs`); file output
  streamed for compression and decompression, removed on error/interrupt.
- Result (one core, real files, ABBA): compress 0.90x -> 1.01x of `lz4 -1`,
  decompress 0.81x -> 1.15x of `lz4 -d`.

### Changes to the plan

- Streaming compression output was not in tasks.md; added after T039/T041
  left compression at 0.97x of lz4.
- The memory map also reads the compressed input of `decompress` (same
  helper, same single `unsafe` call).
- Behaviour change: with `--force`, a compression or decompression that
  fails part-way now removes the output file it was writing (before, a
  failure before the write left an existing file in place).

### Duplications / dead code

- `detect-duplicates.ps1`: crush-core and crush-lz 0 clones. crush-cli had 2,
  both from before this feature: the plugin-name lookup duplicated in
  `decompress.rs` (21 lines) is now one helper `plugin_name_for`; the clone
  in `crush-cli/tests/compress.rs` (26 lines, untouched by this feature) is
  left as is.

## Polish

- Docs: README (feature bullet, fast-lz section, plugin list), `crush-lz/README.md`,
  `perf/README.md` (subset/ABBA runs, `lz-vs-lz4.sh`, `CRUSH_TIMINGS`),
  `.specify/memory/claude.md` (crate, format, plugin hooks).
- Rustdoc: no missing docs in crush-lz; examples on `FastLzConfig`,
  `compress`, `decompress`, `FastLzPlugin`; `cargo doc` clean with
  `-D warnings`.
- Coverage crush-lz 98.2% lines.

### Deferred / follow-ups

- `bench.py ab --suite full` fails when a crate is missing at the base ref
  (new crates); worked around with `--crates`. Tooling change for the user
  (`.claude/`).
- decompress_medium/1MB (deflate) about 4% slower, under the gate; cause
  not identified without a profiler.
- `--level fast` on incompressible small inputs picks `gpu-deflate` by
  weight scoring (pre-existing): output 0.2% larger than the input at about
  24 MB/s in the quickstart smoke test.
