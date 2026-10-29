# Implementation Plan: LZ4-Class Fast Compression Plugin with Content-Aware Selection

**Branch**: `023-lz4-fast-plugin` | **Date**: 2026-10-29 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/023-lz4-fast-plugin/spec.md`

## Summary

Add `fast-lz`, a single-threaded LZ4-class codec (LZ77 sequences, 64 KiB window, no entropy coding) in a new `crush-lz` crate registered through the existing plugin registry. Targets on one core, in memory, on the 128 MB real-file corpus: compress ≥ 906 MB/s (lz4 -1 +15%), decompress ≥ 2,650 MB/s (60% of lz4), ratio ≥ 2.18. crush-core's selector becomes content-aware: when speed is preferred and no plugin is named, it samples the input with `fast-lz` and uses it only if the estimated ratio is ≥ 1.10; otherwise, and for every non-fast level, selection is unchanged. Story 3 (phase 2) cuts the CLI pipeline's single-core overhead so `crush` with `fast-lz` beats the `lz4` tool end to end. Approach and alternatives: [research.md](research.md).

## Technical Context

**Language/Version**: Rust, toolchain 1.99.1 (pinned in `rust-toolchain.toml`), edition 2024

**Primary Dependencies**: `crush-core` (plugin trait, registry, errors), `linkme` (plugin registration, already a workspace dependency), `thiserror`; no new third-party crates. Dev: `criterion`, `proptest`; fuzz: `libfuzzer-sys` (existing pattern)

**Storage**: N/A (in-memory codec; files only via the CLI)

**Testing**: `cargo test` (unit, round-trip, proptest), criterion benches, `cargo fuzz` (WSL), CLI integration tests with `assert_cmd`

**Target Platform**: Windows, Linux, macOS (CI matrix); codec targets verified on the i9-12900K reference machine under WSL2 Ubuntu against lz4 1.10.0

**Project Type**: Rust library workspace + CLI

**Performance Goals**: SC-001 compress ≥ 906 MB/s; SC-002 decompress ≥ 2,650 MB/s; SC-003 ratio ≥ 2.18; SC-005 selection estimate ≤ 2% of compression time; SC-008 (Story 3) end-to-end faster than `lz4 -1` / `lz4 -d` on one core

**Constraints**: No `unsafe` in `crush-lz`; no `.unwrap()`/`.expect()` in production; ≤ 32 MB per thread (design: ≈ 1.1 MiB); no existing benchmark regresses > 5%; balanced/best selection unchanged

**Scale/Scope**: One new crate (~1.5–2k lines incl. tests), a provided-method extension of one trait, selector change, CLI wiring, benches, fuzz targets; Story 3 touches the CLI compress/decompress path

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle / gate | Status | How |
|---|---|---|
| I. Performance First | ✅ | Targets set against a measured external reference (lz4) in the same session; criterion benches; `/perf goal` A/B experiments for tuning; full-suite regression gate |
| II. Correctness & Safety | ✅ (Story 1–2) / ⚠ (Story 3) | No `unsafe` in `crush-lz`; decoder validates every length/offset before copying; proptest round trips; fuzz targets for decode and round trip. Story 3 may add one memory-map call (see Complexity Tracking) |
| III. Modularity & Extensibility | ✅ | New plugin crate via the existing `linkme` registry; trait gets *provided* methods only (non-breaking); selection policy generic over "content-aware" plugins, not hard-coded names |
| IV. Test-First | ✅ | Tasks order tests before implementation in every story (format contract tests, decoder rejection tests, selection tests, CLI tests) |
| Dependency minimalism | ✅ | No new crates; `linkme` already used workspace-wide |
| Quality gates | ✅ planned | fmt, clippy `-D warnings`, tests, docs, coverage > 80% for `crush-lz`, fuzz 100k, bench regression < 5% |
| Post-MVP cleanup | ✅ planned | `detect-duplicates.ps1` after each story; `cleanup-summary.md` |
| Compatibility policy | ✅ | New format v1; no compatibility layers; may change before first release |

Re-check after Phase 1 design: unchanged — the contracts keep `unsafe` out of `crush-lz`, add no dependencies, and keep existing plugins' behaviour (selection contract table).

## Project Structure

### Documentation (this feature)

```text
specs/023-lz4-fast-plugin/
├── plan.md              # This file
├── research.md          # Phase 0: decisions R1–R9
├── data-model.md        # Phase 1: format, compressor state, selection estimate
├── quickstart.md        # Phase 1: validation guide
├── contracts/
│   ├── fast-lz-format.md   # byte format + decoder/encoder obligations
│   └── selection.md        # trait additions, selector and CLI behaviour
├── checklists/requirements.md
├── perf-log.md          # created during /perf goal tuning
└── tasks.md             # /speckit-tasks output
```

### Source Code (repository root)

```text
crush-lz/                         # NEW crate (workspace member)
├── Cargo.toml                    # deps: crush-core, linkme, thiserror; dev: criterion, proptest
├── src/
│   ├── lib.rs                    # FastLzPlugin (CompressionAlgorithm impl, linkme registration), public compress/decompress
│   ├── format.rs                 # FrameHeader, block header encode/parse, bounds
│   ├── compress.rs               # block compressor (hash table, match finder, sequence writer)
│   ├── decompress.rs             # validating block decoder
│   └── estimate.rs               # sampled ratio estimate
├── tests/
│   ├── roundtrip.rs              # property + corpus round trips
│   ├── decoder_rejects.rs        # malformed-input contract cases
│   └── selection_report.rs       # #[ignore] routing/cost report (SC-004/005)
├── benches/codec.rs              # criterion: compress/decompress/estimate
└── fuzz/                         # own [workspace]; fuzz_decompress, fuzz_roundtrip

crush-core/src/plugin/contract.rs # + content_aware(), estimate_ratio() provided methods
crush-core/src/plugin/selector.rs # + select_for_input(); select() excludes content-aware plugins
crush-core/src/compression.rs     # compress_with_options → select_for_input when no plugin named
crush-cli/Cargo.toml              # + crush-lz dependency (links the plugin)
crush-cli/src/algorithm.rs        # doc/tests for --level fast below the size threshold
crush-cli/tests/                  # fast-lz CLI integration tests
Cargo.toml                        # workspace member crush-lz; fuzz dir excluded
perf/lz-vs-lz4.sh                 # same-session codec comparison with lz4 -b1 (WSL)
.claude/skills/perf/bench.py      # suite "fast-lz" (user commits .claude changes)

# Story 3 (phase 2)
crush-cli/src/commands/compress.rs, decompress.rs  # pipeline: mapped input, fewer copies, streamed output
perf/compare.py                   # --cores / --tools filters for the end-to-end ABBA check
```

**Structure Decision**: New plugin crate alongside `crush-parallel` and `crush-gpu`, following the same registration pattern; crush-core gets only the generic selection hook; the CLI gains a dependency and tests, plus pipeline work in Story 3.

## Delivery phases

1. **Story 1 (P1, MVP)** — format + compressor + decoder + plugin registration + benches + fuzz. Gate: round trips, decoder rejection contract, fuzz 100k, first same-session comparison with lz4. Then `/perf goal` tuning (session-gated, `specs/023-lz4-fast-plugin/perf-log.md`) until SC-001..003 are met or the experiment cap is reached.
2. **Story 2 (P2)** — trait provided methods, `select_for_input`, estimate, CLI wiring and tests, selection report.
3. **Story 3 (P3, phase 2)** — measure the single-core pipeline, then remove copies (R8). Starts with a user decision on the memory-map `unsafe`.
4. **Polish** — docs (README plugin table, `perf/README.md`, `.specify/memory/claude.md`), cleanup phase, full gate, `crush-perf` after-implement hook.

## Risks

| Risk | Likelihood | Mitigation / fallback |
|---|---|---|
| Safe Rust compressor stays below 1.15 × lz4 at ratio ≥ 2.18 | Medium–high | `/perf goal` experiments (R4 list); if the cap is reached, report the achieved speed/ratio frontier and ask the user to accept it or relax ratio (≤ 10%) — never silently relax |
| Decoder below 2,650 MB/s | Medium | `extend_from_within` copies, overlap doubling; profile-guided tweaks |
| Estimate cost > 2% on 1 MB inputs | Low | sample count scales with input size (data-model); measured by the selection report |
| Base64-like data with a 1.10–1.2 estimate routed to `fast-lz` while DEFLATE would do much better | Low | threshold tunable (spec assumption); selection report covers it |
| Story 3 unreachable without `unsafe` mmap | Medium | user decision at Story 3 start; record the measured ceiling either way |

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|---|---|---|
| (Story 3 only, pending user approval) one `unsafe` memory-map of regular input files in `crush-cli`, same pattern and SAFETY comment as `crush-parallel::compress_file` | `fs::read` copies the whole input into freshly zeroed memory; with no compression work one core already tops out at ~614 MB/s, below lz4's 634 MB/s end to end | Keeping `fs::read` leaves SC-008 unreachable by construction; other copy removals (R8 items 2–4) are tried first and may be enough |
