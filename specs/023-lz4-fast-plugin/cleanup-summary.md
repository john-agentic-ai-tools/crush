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
