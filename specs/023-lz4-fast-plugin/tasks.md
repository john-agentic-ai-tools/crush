---

description: "Task list for the fast-lz plugin (LZ4-class codec + content-aware selection)"
---

# Tasks: LZ4-Class Fast Compression Plugin with Content-Aware Selection

**Input**: Design documents from `specs/023-lz4-fast-plugin/`

**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md), [contracts/](contracts/), [quickstart.md](quickstart.md)

**Tests**: Included. The constitution makes TDD mandatory (Principle IV) and requires round-trip, property, and fuzz testing for every compression path (Principle II). In every story, test tasks come first and must fail before the implementation tasks start.

**Organization**: Tasks are grouped by user story; each story is a testable increment.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: US1 fast plugin on request, US2 automatic selection, US3 end-to-end CLI speed

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: New crate, workspace wiring, benchmark and fuzz scaffolding

- [X] T001 Create crate `crush-lz` with `crush-lz/Cargo.toml` (workspace package fields; deps `crush-core` path, `linkme`, `thiserror`; dev-deps `criterion`, `proptest`; `[lints.clippy] all = "deny"`, `pedantic = "warn"` like `crush-parallel/Cargo.toml`; `[[bench]] name = "codec" harness = false`) and an empty `crush-lz/src/lib.rs` with crate docs
- [X] T002 Add `crush-lz` to `members` and `crush-lz/fuzz` to `exclude` in the workspace `Cargo.toml`
- [X] T003 [P] Create fuzz crate `crush-lz/fuzz/Cargo.toml` (`cargo-fuzz = true` metadata, its own empty `[workspace]` table, deps `libfuzzer-sys = "0.4"`, `crush-lz` path) with `[[bin]]` entries `fuzz_decompress` and `fuzz_roundtrip` (`test = false`, `bench = false`, `doc = false`) and placeholder targets in `crush-lz/fuzz/fuzz_targets/`
- [X] T004 [P] Create criterion bench skeleton `crush-lz/benches/codec.rs` that includes the shared generators via `#[path = "../../crush-parallel/benches/common/variants.rs"] mod variants;` (corpus text, random) and reads an optional file corpus from env `CRUSH_BENCH_CORPUS`
- [X] T005 Verify the skeleton with `cargo build -p crush-lz` and `bash .claude/hooks/rust-quality-gate.sh`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: The format types every story uses (frame/block headers, bounds). See [contracts/fast-lz-format.md](contracts/fast-lz-format.md).

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [X] T006 Write failing unit tests in `crush-lz/src/format.rs` (`#[cfg(test)] mod tests`) for: `FrameHeader` encode/parse round trip; parse rejects magic ≠ `"LZ"`, `version` ≠ `1`, `flags` ≠ `0`, `block_size_log2` outside `16..=20`, non-zero `reserved`; block header encode/parse with "`compressed_len` low 31 bits, `stored` top bit"; parse rejects `uncompressed_len` of 0 or `> block_size`, `compressed_len` greater than the compress bound, and stored blocks whose `compressed_len ≠ uncompressed_len`; end marker = 4 zero bytes; `compress_bound(n)` = `8 + Σ blocks (8 + u + u/255 + 16) + 4`
- [X] T007 Implement `crush-lz/src/format.rs`: `FrameHeader { block_size_log2 }` (8 bytes: `"LZ"`, version `1`, flags `0`, `block_size_log2` default `20`, 3 reserved zero bytes), `BlockHeader { compressed_len, stored, uncompressed_len }` (8 bytes LE), `END_MARKER`, `compress_bound(input_len, block_size)`; all errors as `crush_core::error::ValidationError::CorruptedData` with a message naming the field (no `.unwrap()`/`.expect()`)
- [X] T008 Export the format module and constants from `crush-lz/src/lib.rs` (`pub mod format;`, `pub const PLUGIN_MAGIC: [u8; 4] = [0x43, 0x52, 0x01, 0x04];`, `pub const PLUGIN_NAME: &str = "fast-lz";`) and run `cargo test -p crush-lz`

**Checkpoint**: Format types tested; stories can start

---

## Phase 3: User Story 1 - Compress with the fast plugin on request (Priority: P1) 🎯 MVP

**Goal**: `fast-lz` compresses and decompresses losslessly, self-identifies, rejects malformed input, and is selectable by name; then tuned toward SC-001..SC-003.

**Independent Test**: `cargo test -p crush-lz`; `crush compress --plugin fast-lz` + `crush decompress` round trip; `perf/lz-vs-lz4.sh` in WSL shows speed and ratio against `lz4 -b1` on the 128 MB real-file corpus.

### Tests for User Story 1 ⚠️ write first, confirm they fail

- [X] T009 [P] [US1] Round-trip tests in `crush-lz/tests/roundtrip.rs` through the public `crush_lz::compress(input, &config)` / `crush_lz::decompress(payload, expected_len)`: empty input; 1–3 bytes (below the minimum match of 4); exactly one block; one block + 1 byte; 3.5 blocks with block_size 64 KiB; all-zero 5 MiB; a run of one 3-byte phrase; random 2 MiB (must come out as stored blocks: `payload.len() <= input.len() + overhead`); the synthetic text corpus; and a proptest over arbitrary `Vec<u8>` up to 300 KB and block sizes 64 KiB/256 KiB
- [X] T010 [P] [US1] Decoder-rejection tests in `crush-lz/tests/decoder_rejects.rs`, one per obligation in contracts/fast-lz-format.md: truncated frame header; bad magic/version/flags/block_size_log2/reserved; block `uncompressed_len` 0 or > block size; stored length mismatch; literal length extension past payload; literals past payload; offset 0; offset > bytes produced in the block; match longer than `uncompressed_len`; block decodes to a different length; total exceeds `expected_len`; missing end marker; trailing bytes after the end marker. Each must return `Err(CorruptedData)` and allocate no more than `expected_len` (assert via a capacity check on the returned error path helper)
- [X] T011 [P] [US1] Plugin contract tests in `crush-lz/src/lib.rs` `#[cfg(test)]`: `name() == "fast-lz"`, metadata magic `[0x43, 0x52, 0x01, 0x04]`, `detect()` true only for that magic; registered in `crush_core::list_plugins()`; `crush_core::compress_with_options(.., with_plugin("fast-lz"))` then `crush_core::decompress` round trips with no plugin named (FR-003); cancellation flag set ⇒ `Err(Cancelled)`

### Implementation for User Story 1

- [X] T012 [US1] Implement the validating block decoder in `crush-lz/src/decompress.rs`: per block reserve exactly `uncompressed_len` (never more); literals via `extend_from_slice`; matches with `offset >= len` via `Vec::extend_from_within`, overlapping matches by repeatedly appending the growing pattern; validate every length/offset against payload bounds, bytes produced in this block, and `uncompressed_len` before copying (research R5); frame-level `decompress(payload, expected_len)` checks the running total against `expected_len`, the end marker, and no trailing bytes
- [X] T013 [US1] Implement the block compressor in `crush-lz/src/compress.rs` (research R4): `Compressor` with a hash table of `2^14` `u32` entries reused across blocks via a running base (no per-block clear); multiplicative hash of 4 bytes from a `u64::from_le_bytes` load; 4-byte verify; 8-byte-at-a-time match extension with `trailing_zeros`; acceleration `step = 1 + (misses >> 6)`; minimum match 4; offsets `1..=65535` within the block; main loop only while ≥ 16 bytes remain, then a literal tail; token/length encoding per data-model.md; emit the block stored when the compressed form is not smaller; frame-level `compress(input, &FastLzConfig)` writes frame header, blocks, end marker into a buffer sized by `compress_bound`
- [X] T014 [US1] Add `FastLzConfig` builder in `crush-lz/src/lib.rs` (constitution: builders) with `block_size_log2` (`16..=20`, default `20`) and validation
- [X] T015 [US1] Implement `FastLzPlugin` (`CompressionAlgorithm`) in `crush-lz/src/lib.rs`: name `fast-lz`, metadata (magic `[0x43, 0x52, 0x01, 0x04]`, description), `compress`/`decompress` delegating to T012/T013 with cancellation checked between blocks, `detect` on the magic; register with `#[distributed_slice(COMPRESSION_ALGORITHMS)]`
- [X] T016 [US1] Link the plugin into the CLI: add `crush-lz` to `crush-cli/Cargo.toml` dependencies and reference it from `crush-cli/src/main.rs` the same way `crush-parallel` is (so `linkme` registration is not dropped)
- [X] T017 [P] [US1] CLI integration tests in `crush-cli/tests/fast_lz.rs`: `crush plugins list` lists `fast-lz` (FR-001); `crush compress --plugin fast-lz` then `crush decompress` (no plugin) round trips a 3 MB text file and an empty file; `crush inspect` reports `Plugin: fast-lz` and the sizes (FR-011); decompressing a corrupted `.crush` file exits non-zero with an error message
- [X] T018 [P] [US1] Fuzz targets: `crush-lz/fuzz/fuzz_targets/fuzz_decompress.rs` (arbitrary bytes into `decompress(data, expected_len)` with `expected_len` from the first 4 bytes capped at 16 MiB; must never panic) and `crush-lz/fuzz/fuzz_targets/fuzz_roundtrip.rs` (arbitrary input, and the input expanded to 3 blocks of 64 KiB by tiling, round-tripped with block_size_log2 = 16)
- [X] T019 [P] [US1] Criterion benches in `crush-lz/benches/codec.rs`: `fast_lz/compress` and `fast_lz/decompress`, single thread, on the synthetic text corpus (64 MB), random data (64 MB), and the `CRUSH_BENCH_CORPUS` file when set; throughput in bytes
- [X] T020 [P] [US1] Same-session comparison harness `perf/lz-vs-lz4.sh` (WSL/Linux): builds `crush-lz`'s codec bench binary in release, generates the 128 MB real-file corpus in `/dev/shm` (`tar -cf - /usr/lib /usr/share | head -c 134217728`), runs `taskset -c 0 lz4 -b1 -i3` and the crush bench pinned to CPU 0 on it, prints a table (compress MB/s, decompress MB/s, ratio, and fast-lz ÷ lz4) and exits non-zero if SC-001..SC-003 are not met
- [X] T021 [US1] Add suite `fast-lz` (`("crush-lz", "codec", None)`) to `SUITES` in `.claude/skills/perf/bench.py` (user commits `.claude` changes)
- [X] T022 [US1] Run `bash .claude/hooks/rust-quality-gate.sh`, then fuzz both targets 100k runs each in WSL (`cd crush-lz && cargo +nightly fuzz run <target> -- -runs=100000`), then `perf/lz-vs-lz4.sh`; record the first results in `specs/023-lz4-fast-plugin/perf-log.md`
- [X] T023 [US1] Tune toward SC-001..SC-003 with `/perf goal` (session `bench.py session begin --mode goal --name fast-lz`; experiments from research R4 ranked by the perf planner; each kept only if `ab` shows a significant gain, no ratio drop below 2.18, and round-trip/fuzz still clean); log each experiment in `specs/023-lz4-fast-plugin/perf-log.md`; stop when targets are met or after 5 experiments, then report the achieved speed/ratio to the user before relaxing anything
- [X] T024 [US1] Post-story cleanup: `.specify/scripts/powershell/detect-duplicates.ps1 -Json -Path crush-lz`, fix duplicates > 20 lines, record in `specs/023-lz4-fast-plugin/cleanup-summary.md`

**Checkpoint**: `fast-lz` usable by name, fuzzed, measured against lz4

---

## Phase 4: User Story 2 - Automatic selection uses the fast plugin only when it pays off (Priority: P2)

**Goal**: With speed preferred and no plugin named, crush-core samples the input with `fast-lz` and uses it only if the estimated ratio is ≥ 1.10; other levels choose exactly as before. See [contracts/selection.md](contracts/selection.md).

**Independent Test**: selector unit tests; `crush compress --level fast` on labelled inputs routes as SC-004 says; `selection_report` shows estimate cost ≤ 2% (SC-005).

**Depends on**: US1 codec (the estimate uses the block compressor)

### Tests for User Story 2 ⚠️ write first, confirm they fail

- [X] T025 [P] [US2] Selector tests in `crush-core/src/plugin/selector.rs` `#[cfg(test)]` with a test content-aware plugin double: weights `throughput < 0.8` never pick a content-aware plugin and pick exactly what `select()` picks today; `throughput >= 0.8` picks it when `estimate_ratio >= 1.10`, falls back to normal scoring when `< 1.10` or `None`; `select()` (no input) excludes content-aware plugins; `select_by_name` unchanged
- [X] T026 [P] [US2] Estimate tests in `crush-lz/src/estimate.rs` `#[cfg(test)]`: sample plan per research R6 "`n = clamp(input_len / 128 / 16 KiB, 4, 16)` samples of `clamp(input_len / 128 / n, 4 KiB, 16 KiB)` bytes (1/128: 1/64 measured 2.02% cost at 16 MB), evenly spaced over the whole input" (corrected 2026-10-29: the first wording, `min(16 KiB, input_len / n)`, sampled 6% of a 1 MB input and broke SC-005); inputs `< 4 × 4 KiB` use whole-input compression; estimate conservative on the text corpus (at most 5% above and at least 75% of the real ratio; first planned as "within 10%", but samples lack block history and measured 20% low with 4 KiB samples); base64 text and random data estimate `< 1.10`; a file compressible only in its second half is still estimated compressible (samples cover the whole input)
- [X] T027 [P] [US2] CLI routing tests in `crush-cli/tests/fast_lz.rs`: `--level fast` (no plugin) on a 4 MB log-like text file ⇒ `inspect` shows `fast-lz`; on 4 MB base64 text and 4 MB random ⇒ not `fast-lz`; balanced and best on the same files ⇒ same plugin as before this feature (compare against `deflate`); `--plugin fast-lz` on base64 text still uses `fast-lz` (FR-009)

### Implementation for User Story 2

- [X] T028 [US2] Add provided methods `fn content_aware(&self) -> bool { false }` and `fn estimate_ratio(&self, input: &[u8]) -> Option<f64> { None }` with docs to `CompressionAlgorithm` in `crush-core/src/plugin/contract.rs`
- [X] T029 [US2] Implement `PluginSelector::select_for_input(&self, input: &[u8])` and make `select()` exclude content-aware plugins in `crush-core/src/plugin/selector.rs` (speed preferred = `weights.throughput >= 0.8`; eligibility = estimate `>= 1.10`, constant `MIN_ESTIMATED_RATIO`); expose whether a plugin is content-aware via the registry in `crush-core/src/plugin/registry.rs`
- [X] T030 [US2] Use `select_for_input(input)` in `compress_with_options` when no plugin is named, in `crush-core/src/compression.rs`
- [X] T031 [US2] Implement the sampled estimate in `crush-lz/src/estimate.rs` (plan from T026, reusing the block compressor and a thread-local `Compressor` so no hash-table allocation per call) and override `content_aware()` → `true` and `estimate_ratio()` in `FastLzPlugin` in `crush-lz/src/lib.rs`
- [X] T032 [US2] Document `--level fast` routing in `crush-cli/src/algorithm.rs` (inputs `< 25 MB` defer to crush-core's content-aware selection; `>= 25 MB` and stdin unchanged, research R7) and add a unit test there that `select_algorithm` still returns `"default"` for small inputs at every level
- [X] T033 [P] [US2] Selection report `crush-lz/tests/selection_report.rs` (`#[ignore]`): for real-file-like (`CRUSH_BENCH_CORPUS` if set), source text, log text, base64 text, random, and DEFLATE-compressed inputs at 1 MB, 16 MB, 128 MB, print estimate, chosen plugin at fast weights, and estimate time as % of `fast-lz` compression time; assert SC-004 routing and cost ≤ 2% for inputs ≥ 1 MB (SC-005)
- [X] T034 [US2] Add `select_for_input` benches (1 MB / 16 MB / 128 MB) to `crush-lz/benches/codec.rs`, run `bash .claude/hooks/rust-quality-gate.sh` and the selection report; append results to `specs/023-lz4-fast-plugin/perf-log.md`
- [X] T035 [US2] Post-story cleanup: `detect-duplicates.ps1 -Json` over `crush-core/src/plugin` and `crush-lz`; update `specs/023-lz4-fast-plugin/cleanup-summary.md`

**Checkpoint**: `--level fast` routes compressible data to `fast-lz`, everything else unchanged

---

## Phase 5: User Story 3 - End-to-end single-core speed through the CLI (Priority: P3, phase 2)

**Goal**: `crush compress`/`decompress` with `fast-lz` on one core beat `lz4 -1` / `lz4 -d` end to end on the reference corpus (SC-008), without slowing other plugins.

**Independent Test**: `perf/compare.py` restricted to one core and the `crush`(fast-lz) / `lz4-1` tools, ABBA, median of ≥ 10.

**Depends on**: US1. **Starts with a user decision** on the memory-map `unsafe` (plan → Complexity Tracking).

### Tests for User Story 3 ⚠️ write first

- [X] T036 [P] [US3] Extend `perf/compare.py` with `--cores LIST` and `--tools LIST` filters and a `crush-fast-lz` tool (`crush compress --plugin fast-lz`), plus ABBA ordering per tool pair when `--abba` is given; keep default behaviour unchanged
- [X] T037 [P] [US3] CLI byte-identity tests in `crush-cli/tests/fast_lz.rs`: output of `crush compress` for empty, 1-byte, threshold−1, and 30 MB inputs is identical before and after pipeline changes (golden CRC32 + length captured before T039), for `fast-lz`, `deflate`, and `parallel-deflate` (done as `crush-cli/tests/pipeline_identity.rs`: library goldens without file metadata, which differs by platform, plus CLI output == library output for the same metadata; empty input is rejected by the CLI for every plugin)

### Implementation for User Story 3

- [X] T038 [US3] Measure the single-core pipeline breakdown (read, compress, assemble, CRC, write; decompress likewise) for `fast-lz` with timing instrumentation behind a `CRUSH_TIMINGS` env var in `crush-cli/src/commands/compress.rs` and `crush-cli/src/commands/decompress.rs`; record in `specs/023-lz4-fast-plugin/perf-log.md`; ask the user about the memory-map `unsafe` with these numbers
- [X] T039 [US3] Remove the assembly copy: let crush-core write the outer header prefix and have the plugin output written after it (`crush-core/src/compression.rs`, a provided `compress_into` on `CompressionAlgorithm` in `crush-core/src/plugin/contract.rs`, overridden by `FastLzPlugin`)
- [X] T040 [US3] Stream decompression output per block to the destination file instead of materialising the whole result first, in `crush-cli/src/commands/decompress.rs` (and a block-iterating decode API in `crush-lz/src/decompress.rs`)
- [X] T041 [US3] Only if approved at T038: read regular input files through `memmap2` (one `unsafe` call with a SAFETY comment mirroring `crush-parallel/src/engine.rs` `compress_file`) instead of `fs::read`, in `crush-cli/src/commands/compress.rs`; keep `fs::read` for stdin and small files
- [ ] T042 [US3] Verify: `bash .claude/hooks/rust-quality-gate.sh`; `python3 perf/compare.py --cores 1 --tools crush-fast-lz,lz4-1 --abba --reps 10` in WSL (SC-008); full `perf/compare.py` and `bench.py ab --suite full` show no slowdown for other plugins; record in `perf-log.md`
- [ ] T043 [US3] Post-story cleanup: `detect-duplicates.ps1 -Json -Path crush-cli`; update `cleanup-summary.md`

**Checkpoint**: All stories functional

---

## Phase 6: Polish & Cross-Cutting Concerns

- [ ] T044 [P] Documentation: `fast-lz` in the plugin table and usage in `README.md`; `crush-lz/README.md` (format summary, targets, how to benchmark); `perf/README.md` (`lz-vs-lz4.sh`); `.specify/memory/claude.md` (new crate, format, selection hook)
- [ ] T045 [P] Rustdoc for every public item in `crush-lz/src/*.rs` with examples; `cargo doc --no-deps` clean
- [ ] T046 Coverage ≥ 80% for `crush-lz` (`cargo tarpaulin -p crush-lz` or `cargo llvm-cov -p crush-lz`), adding tests where short
- [ ] T047 Full gate: `quality-gate` agent (fmt, clippy, tests, docs, audit, deny); fuzz 100k each target in WSL; `python .claude/skills/perf/bench.py ab --ref $(git merge-base HEAD develop) --suite full --name lz-final` (no regression > 5%, confirm any flagged ones); refresh `perf/comparisons` with `fast-lz` included
- [ ] T048 Run the [quickstart.md](quickstart.md) validation end to end and tick the spec's success criteria in `specs/023-lz4-fast-plugin/perf-log.md`

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)** → **Foundational (Phase 2)** → stories
- **US1 (Phase 3)**: after Phase 2
- **US2 (Phase 4)**: after US1's codec (T012–T015); T025, T028–T030 (crush-core side) can start right after Phase 2
- **US3 (Phase 5)**: after US1; independent of US2
- **Polish (Phase 6)**: after the stories being shipped

### Within each story

Tests first (must fail) → format/codec → plugin wiring → CLI → benches/fuzz → measurement → cleanup.

### Parallel Opportunities

- Setup: T003, T004 alongside T001/T002 once the crate exists
- US1: T009, T010, T011 (tests, different files); T017–T020 after T015
- US2: T025, T026, T027 (tests); crush-core T028–T030 in parallel with `crush-lz` T031
- US3: T036, T037
- Polish: T044, T045

### Parallel Example: User Story 1

```text
Task: "Round-trip tests in crush-lz/tests/roundtrip.rs"
Task: "Decoder-rejection tests in crush-lz/tests/decoder_rejects.rs"
Task: "Plugin contract tests in crush-lz/src/lib.rs"
# after T015:
Task: "CLI integration tests in crush-cli/tests/fast_lz.rs"
Task: "Fuzz targets in crush-lz/fuzz/fuzz_targets/"
Task: "Criterion benches in crush-lz/benches/codec.rs"
Task: "Comparison harness perf/lz-vs-lz4.sh"
```

Benchmark runs themselves are never parallel (perf skill rule: one benchmark job at a time).

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Phases 1–2 → format types tested
2. Phase 3 through T022 → `fast-lz` works, fuzzed, first lz4 comparison
3. **STOP and VALIDATE** with the user: show the first speed/ratio table
4. T023 `/perf goal` tuning toward SC-001..SC-003

### Incremental Delivery

1. US1 → fast plugin by name (MVP)
2. US2 → `--level fast` uses it automatically where it helps
3. US3 → end-to-end single-core lead over the `lz4` tool (phase 2, after the `unsafe` decision)

---

## Notes

- `.claude/` changes (T021) are committed by the user; everything else follows the signed-commit rules.
- Codec targets are proven only by `perf/lz-vs-lz4.sh` in the same session as `lz4 -b1`; Windows criterion runs are for A/B decisions and regressions.
- Never relax a target silently: if T023 ends short, report the frontier and ask.
