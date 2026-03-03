# Research: GDeflate GPU Decompression

**Date**: 2026-03-01
**Feature**: 009-gdeflate-gpu-decompression

## R1: GDeflate Bitstream Format

**Decision**: Implement GDeflate per IETF draft-uralsky-gdeflate-00 specification.

**Rationale**: GDeflate is an open standard (Apache 2.0) developed by Microsoft/NVIDIA for DirectStorage 1.1. It reformats DEFLATE streams to extract 32-way parallelism per 64KB tile without changing compression ratio. The specification is well-documented with a reference HLSL implementation.

**Key Format Details**:
- 64KB pages (tiles), each independently decompressible — matches our existing tile size
- 32 interleaved sub-streams per tile, one per SIMD lane
- Same Huffman + LZ77 coding as DEFLATE (RFC 1951)
- Three block types: non-compressed (00), fixed Huffman (01), dynamic Huffman (10)
- Extended codes: length 285 → up to 65538 bytes, distance codes 30-31 for full 64KB window
- Minimum stream size: 128 bytes (32 lanes × 4 bytes for initial state)
- Sub-streams are serialized round-robin so GPU warps read coalesced 32-bit words

**Alternatives considered**:
- LZ4: Higher throughput but lower compression ratio (~2:1 vs ~3:1)
- ANS: Best ratio but too complex for WGSL, primarily CUDA-focused
- Custom LZ77 (current): Inherently sequential per sub-stream, ~130 MiB/s

## R2: HLSL → WGSL Porting Feasibility

**Decision**: Port the HLSL reference decompressor to WGSL, emulating wave intrinsics via workgroup shared memory. Use subgroup operations as an optional optimization path.

**Rationale**: The HLSL shader uses wave intrinsics (WavePrefixSum, WaveReadLaneAt, WaveActiveBallot, WaveMatch) extensively, but the reference implementation already includes a fallback path that emulates all wave ops using `groupshared` memory + barriers. WGSL supports `var<workgroup>` + `workgroupBarrier()` which maps directly.

**HLSL → WGSL Mapping**:

| HLSL | WGSL (Core) | WGSL (Subgroups Extension) |
|------|-------------|---------------------------|
| `groupshared` | `var<workgroup>` | — |
| `GroupMemoryBarrierWithGroupSync()` | `workgroupBarrier()` | — |
| `InterlockedAdd` | `atomicAdd` | — |
| `ByteAddressBuffer` (SRV) | `var<storage, read> input: array<u32>` | — |
| `RWByteAddressBuffer` (UAV) | `var<storage, read_write> output: array<u32>` | — |
| `WavePrefixSum` | Emulate via shared memory | `subgroupExclusiveAdd` |
| `WaveReadLaneAt` | Emulate via shared memory | `subgroupBroadcast` |
| `WaveActiveBallot` | Emulate via shared memory | `subgroupBallot` |
| `WaveMatch` (SM6.5) | Emulate via shared memory | No direct equivalent |

**Critical WGSL limitation**: No native 64-bit integers. The BitReader's 64-bit state must be emulated as a `(lo: u32, hi: u32)` pair with manual carry logic. This is well-understood and adds ~10 lines of helper code.

**Shared memory budget**: ~2.3 KB worst case (g_tmp arrays + Huffman tables). Well within wgpu limits.

## R3: CPU Compressor Strategy

**Decision**: Use the `gdeflate-rs` Rust crate (FFI wrapper around Microsoft's C++ reference compressor) for the CPU compressor. If FFI is problematic, implement a pure-Rust GDeflate compressor.

**Rationale**: The compression path runs on CPU only. Microsoft's reference compressor is proven correct and Apache 2.0 licensed. A Rust FFI wrapper (`gdeflate-rs` by ProjectKML) already exists. Building a pure-Rust compressor is possible but complex (Huffman coding + 32-way sub-stream interleaving) and would require extensive validation against the spec.

**Alternatives considered**:
- Pure-Rust from scratch: Maximum control but high effort for the compressor side (encoder is more complex than decoder)
- `libdeflate` + post-process: Would require re-interleaving an existing DEFLATE stream, which is essentially re-encoding
- `gdeflate-rs` FFI: Fastest path, proven correct, but adds C++ build dependency

**Fallback**: If `gdeflate-rs` proves problematic (build issues, platform compat), implement a pure-Rust compressor. The spec is well-documented enough to do this.

## R4: Backward Compatibility Strategy

**Decision**: Bump `FORMAT_VERSION` from 1 to 2. Keep the existing file header/footer/index structure unchanged. Detect format version in `from_bytes()` and route to appropriate decompressor. LZ77 code remains for reading v1 files.

**Rationale**: The existing format has a `format_version: u32` field in both `GpuFileHeader` and `GpuFileFooter`. Currently set to 1, validated by exact equality check. Changing this to accept both 1 and 2 allows reading old files while new files use GDeflate. The tile header's `version` byte can also distinguish per-tile encoding.

**Changes required**:
- `GpuFileHeader::from_bytes()`: Accept format_version 1 or 2
- `GpuFileFooter::from_bytes()`: Accept format_version 1 or 2
- New compressions write format_version 2
- Decompression checks version and dispatches accordingly
- Tile header version byte: 1=LZ77, 2=GDeflate

## R5: GPU Pipeline Architecture

**Decision**: Create a new WGSL shader (`gdeflate_decompress.wgsl`) and a new compute pipeline alongside the existing LZ77 one. The backend selects the pipeline based on tile format version.

**Rationale**: The GDeflate shader has fundamentally different bindings, workgroup size, and decode logic from the LZ77 shader. A separate pipeline is cleaner than conditionalizing a single shader. The WgpuBackend already creates a pipeline at init time; it can create both and select at dispatch time.

**Buffer layout changes**:
- Current: `tile_meta (uniform)`, `compressed_data (storage read)`, `decompressed_data (storage rw)`, `sub_stream_lengths (storage rw)`
- GDeflate: `compressed_data (storage read)`, `control (storage rw)`, `decompressed_data (storage rw)`, `scratch (storage rw)`
- The GDeflate bindings follow the reference HLSL pattern

## R6: Performance Expectations

**Decision**: Target >1 GB/s decompression throughput. Accept that first iteration may not match nvCOMP's 10-24 GB/s (which uses CUDA + hardware-specific optimizations).

**Rationale**: The reference HLSL shader achieves 10-24 GB/s on modern NVIDIA GPUs with wave intrinsics. Our WGSL port using shared memory emulation will be slower (estimated 3-5x overhead for wave op emulation). On a mid-range GPU (GTX 1060 class), >1 GB/s is a realistic target, representing 7-10x improvement over the current 130 MiB/s LZ77 path.

## Sources

- [GDeflate IETF Specification](https://www.ietf.org/archive/id/draft-uralsky-gdeflate-00.html)
- [GDeflate Reference Implementation (Microsoft)](https://github.com/microsoft/DirectStorage/blob/main/GDeflate/README.md)
- [GDeflate HLSL Shader](https://github.com/microsoft/DirectStorage/blob/main/GDeflate/shaders/GDeflate.hlsl)
- [gdeflate-rs (Rust FFI bindings)](https://github.com/ProjectKML/gdeflate-rs)
- [WebGPU Subgroups Proposal](https://github.com/gpuweb/gpuweb/blob/main/proposals/subgroups.md)
