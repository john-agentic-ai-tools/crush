# Implementation Plan: Hybrid High-Speed Codec

**Branch**: `026-hybrid-speed-codec` | **Date**: 2026-10-31 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/026-hybrid-speed-codec/spec.md`

## Summary

Make the parallel block engine codec-agnostic and fix its scaling, then add `sprint`, a new safe-Rust codec that uses zstd's stream model inside Crush's own format. Blocks are 4 MiB and independent, so parallel decompression and random access carry over. Each block holds 128 KiB segments with Huffman literals, tANS sequences, repeat offsets and a level-3-class match finder. Work is ordered so that measurement comes before design commitments: the head-to-head and profiles come first, then the engine fix, then a prototype that prices safe Rust before the format is frozen.

Technical decisions and their evidence are in [research.md](research.md). The source analysis is [the research report](../../reports/Beating%20zstd%20with%20a%20Crush%20plugin.md).

## Technical Context

**Language/Version**: Rust 1.99.1 (edition 2024), pinned in `rust-toolchain.toml`

**Primary Dependencies**: Workspace only: `rayon`, `crc32fast`, `linkme`, `crush-core`, `crush-parallel`. No new runtime dependencies. `proptest` and `criterion` for tests and benchmarks.

**Storage**: Files. Sprint uses the CRSH container at format version 2 inside the Crush outer header (magic `43 52 01 05`).

**Testing**: `cargo test`, `proptest` round trips, `cargo-fuzz` (100k iterations per target), `criterion`, `perf/compare.py --abba`

**Target Platform**: x86-64 Linux and Windows as gated platforms, with the i9-12900K under WSL2 as the reference machine. Other targets must pass tests but have no speed gates.

**Project Type**: Rust library workspace with a thin CLI

**Performance Goals**: SC-001 to SC-005 and SC-010 in the spec. The derived budget is about 314 MB/s per core on compression at ratio 3.70, and at least 1,100 MB/s single-core decompression.

**Constraints**: Below 32 MB per thread. No C dependency added (FR-015). `#![forbid(unsafe_code)]` on the codec crate unless an exception is recorded (FR-017, FR-018). No `.unwrap()` or `.expect()` in production code.

**Scale/Scope**: One new crate of about 3-4k lines, a refactor of `crush-parallel`, and CLI and benchmark wiring.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle or gate | Status | How the plan satisfies it |
|---|---|---|
| I. Performance First | PASS | Every design decision has a measured gate. Per-worker state serves as the memory pooling requirement. |
| II. Correctness & Safety | PASS | Safe by default, `forbid(unsafe_code)`, validated decode tables, bounded allocation, fuzz and proptest. The exception path is stricter than the constitution: it needs a recorded measurement, not just "absolutely necessary". |
| III. Modularity | PASS | `BlockCodec` trait, plugin registered through `linkme`, independently testable crate, builder configuration kept. |
| IV. Test-First | PASS, with a duty on tasks | Tasks must put failing tests before implementation and hold for approval. Test tasks are mandatory for this feature. |
| Dependencies | PASS | None added. `libdeflater` is pre-existing and unlisted, so it falls under the amendment below. |
| Quality gates | PASS | Coverage, fuzz, docs and regression gates all apply. Miri applies to any unsafe exception and to the existing engine paths touched. |
| Post-MVP cleanup | PASS, with a duty on tasks | `detect-duplicates.ps1` and `cleanup-summary.md` after each user story. |
| Development Rules (no speculative compatibility) | PASS | Reading existing CRSH v1 is a spec requirement (FR-003), not speculation. No compatibility promise for sprint before the freeze. |

**Post-design re-check (Phase 1)**: PASS. No violations, so Complexity Tracking is empty.

**Governance item, not a gate failure**: The spec calls for a cleanup amendment covering external head-to-heads, targets that name zstd, and the stale dependency list. It has a 7-day feedback period and must not block engineering work. It is a task in the Polish phase.

## Project Structure

### Documentation (this feature)

```text
specs/026-hybrid-speed-codec/
├── plan.md              # This file
├── research.md          # Phase 0 decisions
├── data-model.md        # Phase 1 entities and validation
├── quickstart.md        # Phase 1 validation guide
├── contracts/
│   ├── block-codec.md   # Engine trait contract
│   ├── sprint-format.md # File format contract (draft until freeze)
│   └── cli.md           # User-facing behavior
├── checklists/
└── tasks.md             # Created by /speckit-tasks
```

### Source Code (repository root)

```text
crush-parallel/src/
├── codec.rs             # NEW: BlockCodec trait, DeflateCodec
├── block.rs             # Generic over BlockCodec; DEFLATE specifics move to codec.rs
├── engine.rs            # Generic pipeline; borrowed payloads; per-block cancel
├── format.rs            # Accepts v1 and v2; codec id in header
└── index.rs             # Random access through BlockCodec

crush-sprint/            # NEW crate
├── src/
│   ├── lib.rs           # SprintPlugin, registration, forbid(unsafe_code)
│   ├── codec.rs         # BlockCodec implementation
│   ├── format.rs        # Segment and section headers, validation
│   ├── bits.rs          # Forward and backward bit streams
│   ├── histogram.rs     # Byte statistics and sampling
│   ├── huffman.rs       # 4-stream literals, encode and decode
│   ├── tans.rs          # Table build, encode and decode
│   ├── matcher.rs       # Two-table search and repeat offsets
│   ├── sequences.rs     # Sequence encode and decode
│   └── decompress.rs    # Segment decode loop
├── benches/codec.rs     # bench.py suite `sprint`
├── examples/codec_bench.rs  # In-memory timing
├── tests/               # Round-trip, malformed-input, format tests
└── fuzz/                # Own workspace: decompress, roundtrip, tables

crush-core/src/decompression.rs   # Outer CRC handling, per profile
crush-cli/src/commands/           # --block generalised, parallel output if needed
perf/compare.py                   # New tool row, in-memory mode, build rotation
.claude/skills/perf/bench.py      # New `sprint` suite
```

**Structure Decision**: A separate crate follows the `crush-lz` precedent and keeps `forbid(unsafe_code)` enforceable. It depends on `crush-parallel`, because the engine owns the container. `crush-parallel` gains no dependency on sprint.

## Delivery Order and Go/No-Go Points

| Step | Work | Story | Decision |
|---|---|---|---|
| 1 | Head-to-head harness, baselines, in-memory zstd figures | US3 | Is measurement trustworthy? |
| 2 | Profile `parallel-deflate`, 1/4/all cores, both directions | US1 | Where does the time go? |
| 3 | `BlockCodec` extraction with no behaviour change | US1 | Regression-free refactor |
| 4 | Scaling fixes, one at a time | US1 | Meets SC-005? |
| 5 | Block-size sweep of zstd -3 on `real` | US2 | Confirms 4 MiB, or moves to 8 MiB |
| 6 | Prototype Huffman decode and match loop, safe | US2 | Below 0.8x of zstd triggers the exception process |
| 7 | Full codec; sequence-layout experiments | US2 | Format freeze |
| 8 | Fuzz, gates, public report | US2, US3 | Opt-in release |
| 9 | Promotion to automatic selection | US4 | Only after all gates pass |

**At a measured wall**: Record the evidence, then try safe alternatives, then request an unsafe exception (FR-017). There is no libzstd fallback. Fable may be used for a hard diagnosis of this kind, within Pro plan usage, and the escalation is stated when it happens.

## Complexity Tracking

No constitution violations to justify.
