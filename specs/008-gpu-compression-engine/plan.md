# Implementation Plan: GPU Compression Engine

**Branch**: `008-gpu-compression-engine` | **Date**: 2026-02-23 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `specs/008-gpu-compression-engine/spec.md`

## Summary

Implement a new `crush-gpu` workspace crate that provides GPU-accelerated parallel compression/decompression as a crush plugin. The engine uses a tile-based format inspired by Microsoft GDeflate — 64KB independent tiles with 32-way sub-stream parallelism — designed for massively parallel GPU decompression. Cross-platform GPU support is achieved via `wgpu` compute shaders (WGSL), with an optional CUDA fast path via `cudarc` for NVIDIA. The plugin auto-activates when files exceed 100MB, a compatible GPU is present (Vulkan 1.2 / Metal 2, 2GB+ VRAM), and data entropy is below 7.5 bits/byte.

## Technical Context

**Language/Version**: Rust (latest stable, pinned via `rust-toolchain.toml`)
**Primary Dependencies**:
- `wgpu` — cross-platform GPU compute (Vulkan, Metal, DX12 backends)
- `cudarc` — optional NVIDIA CUDA fast path (feature-gated)
- `crush-core` — plugin trait, header format, error types
- `crc32fast` — tile checksums
- `memmap2` — memory-mapped file I/O for large files
- `thiserror` — error types
- `linkme` — compile-time plugin registration
- `bytemuck` — safe GPU buffer casting (zero-copy data transfer)

**Storage**: File-based (Crush archive format with GPU tile extension)
**Testing**: `cargo test`, `criterion` benchmarks, property-based tests (`proptest`), fuzz testing (`cargo-fuzz`)
**Target Platform**: Windows (primary), Linux, macOS (cross-platform via wgpu)
**Project Type**: Rust workspace crate (library)
**Performance Goals**: >2 GB/s compression throughput on NVIDIA 2048+ CUDA core GPU; 4x faster than CPU parallel; within 5% DEFLATE compression ratio
**Constraints**: <256MB GPU memory; <100ms GPU initialization; files >100MB only; entropy <7.5 bits/byte
**Scale/Scope**: Single GPU, files from 100MB to multi-GB

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

### I. Performance First — PASS

- GPU acceleration is the core value proposition, directly aligned
- 64KB tile size matches GDeflate proven design for GPU parallelism
- 128-byte alignment for GPU memory coalescing
- Zero-copy file I/O via `memmap2`
- Benchmark-driven: criterion benchmarks for throughput and compression ratio

### II. Correctness & Safety — PASS

- Pure Rust implementation (no `unsafe` — `wgpu` and `bytemuck` are safe APIs)
- No `.unwrap()` in production code
- CRC32 checksums per tile for integrity verification
- Tile version byte rejects unknown formats (no silent corruption)
- Round-trip property tests (compress → decompress → verify)
- Fuzz testing for format parsing and decompression

### III. Modularity & Extensibility — PASS

- New crate `crush-gpu` follows existing `crush-parallel` plugin pattern exactly
- Implements `CompressionAlgorithm` trait with `linkme` distributed slice registration
- Plugin ID `0x03` in Crush header magic (0x00=deflate, 0x02=parallel-deflate, 0x03=gpu-deflate)
- Compute backend abstraction (trait-based) allows adding new GPU vendors without engine changes
- CUDA fast path is feature-gated, not required

### IV. Test-First Development — PASS

- TDD: tests written before implementation for each component
- Unit tests for tile format serialization, entropy sampling, GPU buffer management
- Integration tests for full compress/decompress round-trip
- Benchmark suite for throughput comparison against CPU parallel
- Fuzz testing for format parsing

### Dependency Justification

| Dependency | Justification | Constitution Status |
|------------|--------------|-------------------|
| `wgpu` | Core GPU compute abstraction — only mature cross-platform GPU API in Rust. Safe API, no `unsafe`. | New — justified: core to feature |
| `cudarc` | Optional NVIDIA CUDA fast path. Feature-gated, not required. | New — justified: optional perf optimization |
| `bytemuck` | Safe GPU buffer casting. Zero-copy data transfer between CPU and GPU. | New — justified: performance-critical GPU data transfer |
| `naga` | WGSL shader compilation (transitive via wgpu). | Transitive — no direct dependency |
| `crush-core` | Plugin trait, header format. | Existing workspace dependency |
| `crc32fast` | Tile checksums. | Existing allowed dependency |
| `memmap2` | Memory-mapped file I/O. | Existing allowed dependency |
| `thiserror` | Error types. | Existing allowed dependency |
| `linkme` | Plugin registration. | Existing workspace dependency |
| `rayon` | CPU fallback decompression parallelism. | Existing allowed dependency |

**Note**: `wgpu`, `cudarc`, and `bytemuck` are not in the constitution's "Allowed Core Dependencies" list. These are justified as GPU-specific dependencies that are essential for the feature. `wgpu` and `bytemuck` are safe APIs with no `unsafe` usage. `cudarc` is feature-gated and optional. None are async runtimes (prohibited). These should be added to the constitution's allowed list for the GPU crate specifically.

### Post-Phase 1 Re-check

Re-evaluate after data model and contracts are designed to ensure:
- GPU buffer management stays within 256MB constraint
- Tile format maintains round-trip correctness across backends
- CPU fallback decompression doesn't introduce new dependencies

## Project Structure

### Documentation (this feature)

```text
specs/008-gpu-compression-engine/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output
│   └── plugin-api.md    # Plugin integration contract
├── checklists/          # Validation checklists
│   └── requirements.md  # Spec quality checklist
└── tasks.md             # Phase 2 output (/speckit.tasks)
```

### Source Code (repository root)

```text
crush-gpu/
├── Cargo.toml
├── src/
│   ├── lib.rs           # Plugin registration, public API re-exports
│   ├── engine.rs        # Main compress/decompress orchestration
│   ├── format.rs        # GPU tile format (header, tile, index, footer)
│   ├── backend/
│   │   ├── mod.rs       # ComputeBackend trait + discovery
│   │   ├── wgpu.rs      # wgpu compute shader backend (Vulkan/Metal/DX12)
│   │   └── cuda.rs      # Optional CUDA backend (feature-gated)
│   ├── shader/
│   │   ├── compress.wgsl    # Compression compute shader
│   │   └── decompress.wgsl  # Decompression compute shader
│   ├── scorer.rs        # Eligibility scoring (size, GPU, entropy)
│   ├── entropy.rs       # Shannon entropy sampling
│   └── vectorize.rs     # Vectorized string matching (P5)
├── benches/
│   ├── throughput.rs    # Compression/decompression throughput benchmarks
│   └── ratio.rs         # Compression ratio comparison benchmarks
├── fuzz/
│   └── fuzz_targets/
│       ├── fuzz_format.rs       # GPU tile format parsing
│       └── fuzz_decompress.rs   # Decompression from arbitrary input
└── tests/
    ├── roundtrip.rs     # End-to-end compress/decompress tests
    ├── format.rs        # Tile format serialization tests
    ├── eligibility.rs   # Scorer/entropy threshold tests
    └── backend.rs       # Backend detection and selection tests
```

**Structure Decision**: New workspace crate `crush-gpu` following the same pattern as `crush-parallel`. Separate `backend/` module for GPU vendor abstraction. Compute shaders stored as `.wgsl` files in `shader/` directory, embedded at compile time.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| `wgpu` not in allowed deps | Core GPU compute — no alternative exists in Rust for cross-platform GPU | Could use raw Vulkan/Metal bindings but would be unsafe and require 3x more code |
| `cudarc` not in allowed deps | NVIDIA CUDA fast path for maximum performance | wgpu alone is sufficient but leaves 30-50% NVIDIA performance on the table |
| `bytemuck` not in allowed deps | Safe CPU↔GPU buffer casting | Manual byte manipulation would be unsafe and error-prone |
