# crush-gpu

GPU-accelerated tile-based compression engine with 32-way parallel decompression via GDeflate.

## Overview

`crush-gpu` is the GPU acceleration crate for the [Crush](https://github.com/john-agentic-ai-tools/crush) compression toolkit. It implements a [GDeflate](https://github.com/microsoft/DirectStorage/blob/main/GDeflate/GDeflate_spec.pdf)-inspired compression format designed for massively parallel GPU decompression.

Key design principles:
- **64 KB independent tiles** enable parallel processing and random access
- **32-way sub-stream parallelism** matches GPU warp/wavefront width
- **Batched GPU dispatch** minimizes host-GPU synchronization overhead
- **Automatic CPU fallback** when no GPU is available or decompression fails

## Architecture

```text
                    ┌─────────────────────────────────┐
                    │           engine.rs              │
                    │   compress() / decompress()      │
                    └──────┬──────────────┬────────────┘
                           │              │
                   ┌───────▼──────┐ ┌─────▼──────────┐
                   │  gdeflate.rs │ │  backend/       │
                   │  CPU compress│ │  GPU decompress │
                   │  CPU fallback│ │  (wgpu/CUDA)    │
                   └──────────────┘ └─────┬──────────-┘
                                          │
                                    ┌─────▼──────────┐
                                    │  shader/        │
                                    │  WGSL compute   │
                                    └────────────────-┘
```

### Modules

| Module | Purpose |
|--------|---------|
| `engine` | Top-level compress/decompress API, tile management, random access |
| `backend` | GPU abstraction layer (`ComputeBackend` trait), wgpu implementation |
| `format` | CGPU binary file format (headers, tile index, footer) |
| `gdeflate` | GDeflate compression (CPU) and decompression (CPU fallback) |
| `scorer` | GPU eligibility scoring (file size, entropy, GPU availability) |
| `entropy` | Shannon entropy calculation for compressibility assessment |
| `vectorize` | Heuristic for text-heavy data detection |
| `lz77` | LZ77 match finding for the v1 sub-stream format |

## Performance

### GPU Decompression Throughput (Batched Dispatch)

**Test Environment:**
- GPU: NVIDIA GeForce RTX 3060 Ti (Vulkan)
- Rust: 1.93.1 (stable), release mode

| Corpus | GPU Throughput | CPU Throughput | Winner |
|--------|---------------|----------------|--------|
| log-1MB | 137 MiB/s | 329 MiB/s | CPU (small data) |
| binary-1MB | 86 MiB/s | 126 MiB/s | CPU (small data) |
| mixed-1MB | 87 MiB/s | 181 MiB/s | CPU (small data) |
| mixed-10MB | **344 MiB/s** | 186 MiB/s | **GPU 1.85x** |

GPU decompression outperforms CPU at larger data sizes where the per-tile dispatch overhead is amortized. The crossover point is around 2-4 MB.

### Batched vs Per-Tile Dispatch

Batching multiple tiles into a single GPU submission eliminates per-tile host-GPU synchronization overhead:

| Dispatch Mode | 1 MB (16 tiles) | Improvement |
|---------------|-----------------|-------------|
| Per-tile (old) | ~5-12 MiB/s | baseline |
| Batched (current) | **120+ MiB/s** | **10-24x** |

### Compression Throughput (CPU)

| Corpus | Throughput |
|--------|-----------|
| log-text-1MB | 178 MiB/s |
| binary-1MB | 70 MiB/s |
| mixed-1MB | 99 MiB/s |
| mixed-10MB | 104 MiB/s |

## File Format (CGPU)

```text
┌──────────────────────┐
│  GpuFileHeader (64B) │  Magic "CGPU", version, tile_size, tile_count
├──────────────────────┤
│  Tile 0              │  TileHeader (32B) + compressed payload
│  (128-byte aligned)  │
├──────────────────────┤
│  Tile 1              │
├──────────────────────┤
│  ...                 │
├──────────────────────┤
│  Tile Index          │  O(1) random access to any tile
├──────────────────────┤
│  GpuFileFooter (24B) │  Index offset, checksum, magic
└──────────────────────┘
```

- **Tile size**: 64 KB (default), independently decompressible
- **Alignment**: 128-byte boundaries for GPU memory coalescing
- **Checksums**: Optional per-tile CRC32 integrity verification
- **Random access**: Tile index enables O(1) decompression of any tile (~1 ms)

## GDeflate Algorithm

GDeflate distributes DEFLATE across 32 parallel sub-streams:

1. **LZ77 match finding** (greedy, 3-byte hash chains)
2. **Round-robin distribution** of symbols across 32 sub-streams
3. **Fixed Huffman encoding** (BTYPE=01) per sub-stream
4. **Interleaved serialization** for GPU-friendly memory access

On decompression, 32 GPU threads each decode one sub-stream in parallel, then reconstruct the original data.

Reference: IETF draft `draft-uralsky-gdeflate-00`

## Usage

### As a Library

```rust
use crush_gpu::engine::{compress, decompress, EngineConfig};
use std::sync::atomic::AtomicBool;

let cancel = AtomicBool::new(false);
let config = EngineConfig::default();

// Compress (always on CPU)
let data = b"Hello, GPU compression!".repeat(1000);
let compressed = compress(&data, &config, &cancel).expect("compress");

// Decompress (GPU if available, CPU fallback)
let decompressed = decompress(&compressed, &config, &cancel).expect("decompress");
assert_eq!(data.as_slice(), decompressed.as_slice());
```

### Configuration

```rust
use crush_gpu::engine::EngineConfig;

let config = EngineConfig {
    tile_size: 65536,          // 64 KB tiles (default)
    sub_stream_count: 32,      // Matches GPU warp width
    enable_checksums: true,    // Per-tile CRC32
    force_cpu: false,          // Allow GPU decompression
};
```

### Random Access

```rust
use crush_gpu::engine::{load_tile_index, decompress_tile_by_index, EngineConfig};

let config = EngineConfig::default();
let archive: &[u8] = &compressed_data;

// Load tile index (O(1) per tile)
let index = load_tile_index(archive).expect("load index");

// Decompress only the tile you need
let tile_data = decompress_tile_by_index(archive, 42, &index, &config)
    .expect("decompress tile");
```

### As a crush-core Plugin

`crush-gpu` auto-registers as a `crush-core` plugin via `linkme`:

```rust
use crush_core::{init_plugins, list_plugins};

init_plugins().expect("init");

for plugin in list_plugins() {
    println!("{}: {} MB/s", plugin.name, plugin.throughput);
    // Prints: gpu-deflate: 2000 MB/s
}
```

## GPU Backend

### Backends

| Backend | API | Feature flag | GPU vendor |
|---------|-----|-------------|------------|
| wgpu | Vulkan 1.2+ / Metal 2+ / DX12 | *(default)* | Any supported GPU |
| CUDA | CUDA 13.x via nvrtc | `cuda` | NVIDIA (compute capability 7.0+) |

Backend selection priority (with `--gpu-backend auto`, the default):
1. CUDA (if `cuda` feature enabled and NVIDIA GPU present)
2. wgpu (Vulkan on Windows/Linux, Metal on macOS)

Override at runtime: `crush compress --gpu-backend cuda` or `--gpu-backend wgpu`.

### Requirements

- 2 GB+ VRAM (discrete GPU recommended)
- Vulkan, Metal, or DX12 driver
- CUDA backend additionally requires [NVIDIA CUDA Toolkit](https://developer.nvidia.com/cuda-downloads) 13.x and Visual Studio 2022 Build Tools (Windows)

### GPU Eligibility

The scorer module automatically determines whether GPU acceleration benefits a given workload:

| Criterion | Threshold |
|-----------|-----------|
| File size | > 100 MB |
| GPU available | Yes |
| Shannon entropy | < 7.5 bits/byte |

All criteria must pass for GPU dispatch. High-entropy (incompressible) data is rejected to avoid wasting GPU resources.

## Running Benchmarks

```bash
# Throughput benchmarks (GPU vs CPU decompression)
cargo bench --package crush-gpu --bench throughput

# Compression ratio benchmarks
cargo bench --package crush-gpu --bench ratio

# All crush-gpu tests
cargo test --package crush-gpu
```

## Development

```bash
# Build
cargo build -p crush-gpu

# Build with CUDA support
cargo build -p crush-gpu --features cuda

# Test (standard — no GPU required for CI)
cargo test -p crush-gpu

# Clippy
cargo clippy -p crush-gpu --all-targets -- -D warnings

# Docs
cargo doc -p crush-gpu --no-deps
```

### CUDA Testing

CUDA tests are feature-gated behind `#[cfg(feature = "cuda")]` and require:

1. An NVIDIA GPU with >= 2 GB VRAM
2. CUDA Toolkit nvrtc DLLs on `PATH`

On Windows with CUDA 13.1, add the runtime libraries to your shell:

```bash
# Git Bash / MSYS2
export PATH="/c/Program Files/NVIDIA GPU Computing Toolkit/CUDA/v13.1/bin/x64:$PATH"
```

```powershell
# PowerShell
$env:PATH = "C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v13.1\bin\x64;$env:PATH"
```

Then run:

```bash
cargo test -p crush-gpu --features cuda -- cuda --nocapture
```

This executes 10 CUDA-specific tests:

- LZ77 roundtrip (small, 64 KB, multi-tile)
- GDeflate roundtrip (multiple sizes, batch)
- Cancellation (pre-set and mid-batch)
- CUDA vs CPU output parity
- Backend info validation

### Full Local GPU Verification

```bash
export PATH="/c/Program Files/NVIDIA GPU Computing Toolkit/CUDA/v13.1/bin/x64:$PATH"

# All crush-gpu tests (wgpu + CUDA)
cargo test -p crush-gpu --features cuda --nocapture

# Clippy (CUDA mode)
cargo clippy -p crush-gpu --features cuda --all-targets -- -D warnings
```

### CI Notes

GitHub Actions free tier has no GPU. CI runs `cargo clippy --all-targets -- -D warnings` and `cargo test` without the `cuda` feature. GPU and CUDA tests are local-only.

> **Note:** `nvcc` does not need to be on `PATH` at build time. The `cudarc` crate uses the explicit `cuda-13010` feature flag instead of auto-detecting the CUDA version.

## License

MIT
