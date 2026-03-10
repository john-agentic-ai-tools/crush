# Implementation Plan: CLI GPU Integration Update

**Branch**: `010-cli-gpu-update` | **Date**: 2026-03-02 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/010-cli-gpu-update/spec.md`

## Summary

Integrate the existing `crush-gpu` crate into the CLI binary so users can compress/decompress files using GPU acceleration. The work is primarily CLI-level wiring: adding the `crush-gpu` dependency for force-linking (plugin registration), adding CLI flags (`--force-cpu`, `--gpu-device`), extending the configuration system with `gpu.*` keys, enhancing `plugins info` to show GPU device details, and updating the algorithm auto-selection logic when `gpu.enabled` is configured. A small addition to `crush-gpu` is needed: a `configure()` function to receive CLI-level settings (force-cpu, device index) since the `CompressionAlgorithm` trait doesn't support plugin-specific configuration.

## Technical Context

**Language/Version**: Rust (stable, pinned via `rust-toolchain.toml`)
**Primary Dependencies**: `crush-core` 0.2.0, `crush-gpu` 0.1.0, `crush-parallel` 0.1.0, `clap` 4, `wgpu` 28.0 (transitive via crush-gpu)
**Storage**: N/A (file-based CLI tool)
**Testing**: `cargo test`, integration tests via `assert_cmd` + `predicates`
**Target Platform**: Windows, Linux, macOS (anywhere wgpu runs)
**Project Type**: Cargo workspace (existing)
**Performance Goals**: GPU compression throughput > 750 MB/s on 10+ MB files (50% faster than parallel-deflate's 500 MB/s)
**Constraints**: GPU plugin always linked (binary size increase from wgpu is acceptable); GPU operations degrade gracefully on systems without compatible hardware
**Scale/Scope**: ~10 files modified across 2 crates (crush-cli, crush-gpu)

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|-----------|--------|-------|
| I. Performance First | PASS | GPU integration provides higher throughput (2000 MB/s claimed). No performance regression to existing CPU paths — GPU is additive. |
| II. Correctness & Safety | PASS | No `unsafe` code added. GPU errors propagate through existing `CrushError` chain. Fallback to CPU on failure. |
| III. Modularity & Extensibility | PASS | Uses existing plugin architecture (`linkme` registration). GPU config exposed via clean public API. No changes to `CompressionAlgorithm` trait. |
| IV. Test-First Development | PASS | Integration tests for new CLI flags. Plugin roundtrip test already exists via `crush plugins test`. |
| Dependency Minimalism | PASS | `crush-gpu` is already a workspace member. `wgpu` is already an approved workspace dependency. No new dependencies added. |
| Prohibited Dependencies | PASS | No async runtimes in core. `pollster` (blocking executor) already used by crush-gpu for wgpu async calls. |
| Quality Gates | PASS | All gates applicable: cargo test, clippy, coverage, benchmarks, docs. |

**Gate result: PASS — no violations.**

## Project Structure

### Documentation (this feature)

```text
specs/010-cli-gpu-update/
├── plan.md              # This file
├── research.md          # Phase 0: design decisions and alternatives
├── data-model.md        # Phase 1: configuration and entity models
├── quickstart.md        # Phase 1: developer onboarding
├── contracts/
│   ├── cli-flags.md     # CLI argument contract
│   └── gpu-config.md    # Configuration contract
├── checklists/
│   └── requirements.md  # Spec quality checklist
└── tasks.md             # Phase 2 output (via /speckit.tasks)
```

### Source Code (repository root)

```text
crush/
├── crush-gpu/
│   └── src/
│       ├── lib.rs            # MODIFY: add configure(), GpuPluginConfig, re-export discover_gpu/GpuInfo
│       └── engine.rs         # MODIFY: read global config in compress/decompress
├── crush-cli/
│   ├── Cargo.toml            # MODIFY: add crush-gpu dependency
│   └── src/
│       ├── main.rs           # MODIFY: force-link crush-gpu, call configure()
│       ├── cli.rs            # MODIFY: add --force-cpu, --gpu-device args
│       ├── config.rs         # MODIFY: add GpuConfig struct, gpu.* keys
│       ├── algorithm.rs      # MODIFY: GPU-aware auto-selection when gpu.enabled
│       └── commands/
│           ├── compress.rs   # MODIFY: thread GPU config from args
│           ├── decompress.rs # MODIFY: thread GPU config from args
│           └── plugins.rs    # MODIFY: show GPU device info in plugins info
└── Cargo.toml                # NO CHANGE (crush-gpu already a workspace member)
```

**Structure Decision**: Existing Cargo workspace structure. No new crates. Changes are confined to `crush-cli` (consumer) and `crush-gpu` (minor API addition). The `crush-core` crate is NOT modified — the `CompressionAlgorithm` trait remains unchanged.

## Key Design Decisions

### D1: GPU Configuration Side-Channel (Global Config)

**Problem**: The `CompressionAlgorithm` trait's `compress/decompress` methods accept only `(input, cancel_flag)`. There is no mechanism to pass plugin-specific configuration (like `force_cpu` or `gpu_device`) through the existing plugin interface. The `decompress()` function in crush-core also takes no options.

**Decision**: Add a `configure(GpuPluginConfig)` function to `crush-gpu` that stores settings in a process-global `OnceLock<GpuPluginConfig>`. The GPU plugin's `CompressionAlgorithm` implementation reads from this global config when constructing `EngineConfig`.

**Rationale**:
- No breaking changes to the core trait (constitution: modularity principle)
- Similar pattern to `crush_core::init_plugins()` — called once at startup
- Thread-safe via `OnceLock` (set once, read many)
- CLI explicitly configures GPU behavior through crush-gpu's public API

**Alternatives rejected**:
- Extending `CompressionAlgorithm` trait with config parameter: Breaking change to all plugin implementations, over-engineering for one plugin
- Environment variables: Non-programmatic, harder to test, fragile
- Thread-local storage: Incompatible with rayon parallelism

### D2: CLI Flags Scoped to Relevant Subcommands

**Decision**: `--force-cpu` on `decompress` only. `--gpu-device` on both `compress` and `decompress`. These are GPU-specific flags, not global flags.

**Rationale**: Follows existing pattern where `--block` is on `decompress` only and `--plugin` is on `compress` only. Keeps help text focused.

### D3: GPU Auto-Selection Gated by Configuration

**Decision**: GPU is NOT auto-selected by default. Users must either:
- Explicitly use `--plugin gpu-deflate`, OR
- Set `gpu.enabled = true` in configuration

When `gpu.enabled = true`, the algorithm selection function considers GPU alongside parallel-deflate for files above the existing 25 MB threshold.

**Rationale**: Prevents unexpected GPU usage on shared systems. Matches spec assumption about opt-in GPU auto-selection.

### D4: `plugins info` GPU Device Discovery

**Decision**: `crush plugins info gpu-deflate` calls `crush_gpu::discover_gpu()` to probe for GPU hardware and displays device details. This is a lazy probe — GPU is only initialized when this specific command runs, not at CLI startup.

**Rationale**: Avoids adding startup latency to all CLI commands. Users explicitly request GPU info when they want it.

## Complexity Tracking

> No constitution violations — this section is intentionally empty.
