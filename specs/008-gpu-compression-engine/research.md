# Research: GPU Compression Engine

**Feature**: `008-gpu-compression-engine`
**Date**: 2026-02-23

## R-001: GPU Compute Framework Selection

### Decision: `wgpu` as primary, `cudarc` as optional NVIDIA fast path

### Rationale

`wgpu` is the only mature, safe, cross-platform GPU compute API in Rust. It abstracts over Vulkan (AMD/NVIDIA on Linux/Windows), Metal (Apple), and DX12 (Windows) through the WebGPU specification. Compute shaders are written in WGSL (WebGPU Shading Language) and compiled to native shaders via `naga` at runtime.

For NVIDIA, a `cudarc`-based CUDA fast path is feature-gated to maximize performance. CUDA provides lower overhead, direct PTX kernel dispatch, and access to NVIDIA-specific optimizations (warp shuffle, shared memory banks) that wgpu's WGSL abstraction cannot fully exploit.

### Alternatives Considered

| Framework | Pros | Cons | Decision |
|-----------|------|------|----------|
| `wgpu` | Safe Rust API, cross-platform (Vulkan/Metal/DX12), active development, WebGPU standard | Higher overhead than native CUDA, WGSL less expressive than PTX | **Selected** as primary |
| `cudarc` | Direct CUDA access, lowest overhead for NVIDIA, nvrtc for runtime compilation | NVIDIA-only, adds build complexity | **Selected** as optional fast path |
| `ash` (raw Vulkan) | Maximum Vulkan control | Unsafe API, requires manual resource management, no Metal support | Rejected — too much unsafe code |
| `vulkano` | Safe Vulkan bindings | Vulkan-only (no Metal), less mature than wgpu | Rejected — wgpu provides broader platform coverage |
| `rust-gpu` | Write shaders in Rust | Experimental, compiler backend is unstable, limited GPU feature support | Rejected — not production-ready |
| `metal-rs` | Direct Metal API | Apple-only | Rejected — wgpu covers Metal through its backend |

### Implementation Notes

- Feature flag: `cuda` enables `cudarc` dependency and CUDA backend
- Backend selection at runtime: CUDA > wgpu (when NVIDIA detected and `cuda` feature enabled)
- WGSL shaders embedded at compile time via `include_str!`
- wgpu device initialization is synchronous (via `pollster::block_on` or equivalent)

## R-002: GPU Tile Format Design

### Decision: Custom Crush GPU format with 64KB tiles, 32 sub-streams, 128-byte alignment

### Rationale

The format draws directly from the GDeflate IETF specification (draft-uralsky-gdeflate-00) but adapted for the Crush ecosystem:

- **64KB tile size**: Industry-proven choice used by Microsoft DirectStorage and NVIDIA nvCOMP. Balances parallelism (thousands of tiles for large files) with compression ratio (enough context for effective LZ matching). Each tile decompresses independently, enabling random access and massively parallel GPU decompression.

- **32 sub-streams per tile**: Matches GPU warp width (NVIDIA warp = 32 threads). The compressed bitstream within each tile is interleaved across 32 sub-streams so that 32 GPU threads can decode Huffman codes simultaneously without synchronization. AMD wavefronts (64 threads) process two tiles per wavefront.

- **128-byte alignment**: GPU memory coalescing requires aligned access. 128 bytes = 32 threads × 4 bytes, ensuring each thread in a warp reads a naturally aligned 32-bit word in a single memory transaction.

### Format Layout

```text
Offset 0:           GpuFileHeader (64 bytes)
Offset 64:          Tile 0 — TileHeader (32 bytes) + payload (padded to 128-byte boundary)
...                 Tile N-1
Offset X:           TileIndexHeader (8 bytes)
Offset X+8:         TileIndexEntry[0..N] (24 bytes each)
Offset X+8+24N:     GpuFileFooter (24 bytes) ← last 24 bytes of file
```

### Key Structures

- **GpuFileHeader (64 bytes)**: Magic "CGPU", format version, engine version, tile size (64KB), tile count, uncompressed size, flags (entropy-checked, vectorize-used, checksums-enabled)
- **TileHeader (32 bytes)**: Format version byte, compressed size, uncompressed size (always 64KB except last tile), CRC32 of uncompressed data, sub-stream count (32), flags (stored/compressed)
- **TileIndexEntry (24 bytes)**: Tile offset (u64), compressed size (u32), uncompressed size (u32), checksum (u32), flags (u32)
- **GpuFileFooter (24 bytes)**: Same structure as crush-parallel footer for consistency — index offset, index size, footer checksum, format version, magic

### Alternatives Considered

| Approach | Pros | Cons | Decision |
|----------|------|------|----------|
| GDeflate-compatible bitstream | Interop with DirectStorage/nvCOMP | Locked to DEFLATE algorithm, no Crush header integration | Rejected — A-009 |
| Crush-parallel CRSH format with larger blocks | Reuse existing code | Not optimized for GPU memory patterns, no sub-stream parallelism | Rejected |
| Custom GPU format (selected) | Full control over alignment, sub-streams, versioning | No interop with existing GDeflate tools | **Selected** |

## R-003: Entropy Sampling for Eligibility

### Decision: Shannon entropy calculated on 1MB sample, threshold 7.5 bits/byte

### Rationale

Shannon entropy measures the average information content per byte. Truly random data has entropy ~8.0 bits/byte (maximum). Encrypted data, already-compressed files, and random data all fall in the 7.5-8.0 range. Compressible data (text, structured binary, logs) typically has entropy between 3.0-7.0 bits/byte.

The 7.5 threshold is a standard heuristic used by tools like `binwalk` and `ent`. It's aggressive enough to catch truly incompressible data while allowing structured binary formats through.

### Sampling Strategy

1. Read first 1MB of the file (or entire file if <1MB)
2. Count byte frequency distribution (256 buckets)
3. Calculate Shannon entropy: `H = -Σ(p_i × log2(p_i))` where `p_i` is frequency of byte `i`
4. If `H > 7.5`: reject (data unsuitable for GPU compression)
5. If `H <= 7.5`: proceed with GPU compression

### Performance

- Sampling 1MB takes <1ms on modern hardware (simple byte counting)
- Negligible compared to GPU initialization overhead (~50-100ms)
- Applied only once per file, before GPU resources are allocated

### Alternatives Considered

| Method | Pros | Cons | Decision |
|--------|------|------|----------|
| Shannon entropy (selected) | Fast, well-understood, proven heuristic | Doesn't detect some edge cases (e.g., repeated incompressible blocks) | **Selected** |
| Sample-compress 1MB | Most accurate | Slow (~10ms), requires full compression pipeline | Rejected for initial check |
| File extension heuristic | Instant | Unreliable, easily spoofed, misses many cases | Rejected |
| Magic byte detection | Fast | Only detects known compressed formats, not encrypted data | Rejected as sole method |

## R-004: Compression Algorithm for GPU Tiles

### Decision: LZ77-based compression with Huffman coding, adapted for 32-way parallel decode

### Rationale

GDeflate demonstrates that the DEFLATE algorithm (LZ77 + Huffman) can be effectively parallelized for GPU by interleaving the compressed bitstream across 32 sub-streams. The key insight is that Huffman decoding is inherently serial per-stream, but by splitting into 32 independent sub-streams, 32 GPU threads can decode in parallel.

**Compression** (CPU-side): The LZ77 matching phase runs on CPU (matching is memory-latency-bound, not compute-bound — poor GPU fit). Huffman code assignment and bitstream interleaving are also done on CPU. The CPU produces the GPU-friendly tile format.

**Decompression** (GPU-side): Each tile's 32 sub-streams are decoded in parallel by a GPU thread group (warp/wavefront). This is the primary performance win — decompression throughput scales with GPU thread count.

### Why Compression is CPU, Decompression is GPU

The asymmetry is intentional and follows GDeflate's design:
- LZ77 match finding requires random memory access with data-dependent addresses — extremely poor GPU utilization
- Huffman encoding is a simple lookup table — fast on CPU
- Huffman decoding of 32 independent streams is embarrassingly parallel — perfect GPU workload
- Real-world use case: compress once (CPU), decompress many times (GPU) — optimizing decompression is higher value

### Alternatives Considered

| Approach | Pros | Cons | Decision |
|----------|------|------|----------|
| Full GPU compression + decompression | Maximum GPU utilization | LZ77 matching is a poor GPU workload, lower compression ratio | Rejected |
| CPU compression, GPU decompression (selected) | Best ratio + best decomp throughput | Compression throughput limited by CPU | **Selected** |
| GPU-native algorithm (e.g., ANS) | Better GPU parallelism for encoding | Different compression format, no DEFLATE compatibility | Deferred to future iteration |

## R-005: Vectorized String Matching (P5)

### Decision: SIMD-accelerated LZ77 matching on CPU during compression, activated conditionally

### Rationale

String vectorization for compression refers to using SIMD instructions (SSE4.2, AVX2, or NEON) to accelerate the LZ77 dictionary matching phase. Instead of comparing one byte at a time, SIMD processes 16-32 bytes simultaneously, finding longer matches faster.

**When it helps**: Text-heavy data (logs, CSV, JSON, source code) with repetitive string patterns. Longer matches = fewer literals = better compression ratio. SIMD matching can find matches that a greedy single-byte matcher would miss.

**When it doesn't help**: Binary data, media files, or data with few repeating patterns. The overhead of SIMD setup isn't justified when matches are short or rare.

### Activation Logic

1. During entropy sampling (R-003), also compute a "string density" metric: ratio of printable ASCII bytes to total bytes
2. If string density > 70% AND entropy < 6.0 bits/byte: activate vectorized matching
3. Compare output size from standard matching vs vectorized matching on a 1MB sample
4. Use whichever produces smaller output for the full file
5. If vectorized output is not at least 1% smaller: fall back to standard matching

### Implementation

- Use Rust's `std::arch` SIMD intrinsics for portable SIMD across x86 and ARM
- 128-bit (SSE4.2) as baseline, 256-bit (AVX2) when available
- Match length comparison: `_mm_cmpeq_epi8` for 16-byte parallel comparison
- Hash-based match finding with SIMD-accelerated hash computation

### Alternatives Considered

| Approach | Pros | Cons | Decision |
|----------|------|------|----------|
| CPU SIMD matching (selected) | Portable, well-understood, proven benefit on text | CPU-only, limited to single-core | **Selected** |
| GPU parallel matching | Massive parallelism | Poor memory access patterns for hash tables, high latency | Rejected — R-004 rationale |
| No vectorization | Simplest implementation | Leaves 3-10% compression ratio improvement on table for text data | Rejected for P5 |

## R-006: Cross-Platform Backend Architecture

### Decision: Trait-based backend abstraction with runtime GPU discovery

### Rationale

The `ComputeBackend` trait abstracts GPU vendor differences behind a uniform interface. At startup, the engine probes available GPUs and selects the best backend:

1. **NVIDIA + `cuda` feature enabled**: Use CUDA backend (lowest overhead)
2. **NVIDIA without `cuda` feature**: Use wgpu Vulkan backend
3. **AMD**: Use wgpu Vulkan backend
4. **Apple Silicon**: Use wgpu Metal backend
5. **No GPU / below minimum requirements**: Return `None`, trigger CPU fallback

### Backend Trait

```
ComputeBackend
├── name() → &str
├── gpu_info() → GpuInfo (vendor, vram, compute_capability)
├── compress_tiles(tiles: &[RawTile]) → Vec<CompressedTile>
├── decompress_tiles(tiles: &[CompressedTile]) → Vec<RawTile>
└── release() — GPU resource cleanup
```

### GPU Discovery

- wgpu's `Instance::enumerate_adapters()` for non-CUDA backends
- `cudarc::driver::CudaDevice::count()` for CUDA availability
- Filter by minimum requirements: Vulkan 1.2 / Metal 2 + 2GB VRAM
- Select highest-capability GPU when multiple are present

## R-007: CPU Fallback Decompression

### Decision: Pure Rust CPU decompressor that reads GPU tile format without any GPU dependency

### Rationale

FR-002 requires decompression on any system, with or without GPU. The CPU fallback decompressor:

1. Reads the GPU tile format (GpuFileHeader, TileHeader, TileIndex)
2. For each tile, reads all 32 sub-streams sequentially
3. Decodes Huffman codes and executes LZ77 copies
4. Validates CRC32 per tile
5. Uses rayon for parallel tile decompression (different tiles on different CPU threads)

This ensures archives are portable — a file compressed with an NVIDIA GPU can be decompressed on a Mac without a GPU.

### Performance Target

CPU fallback decompression should match existing `crush-parallel` CPU decompression throughput. The GPU tile format adds slight overhead (sub-stream interleaving) but this is offset by rayon parallelism across tiles.
