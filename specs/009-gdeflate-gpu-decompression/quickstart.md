# Quickstart: GDeflate GPU Decompression

## Build

```bash
# Build the workspace (includes crush-gpu with GDeflate support)
cargo build --workspace

# Build with CUDA support (optional, NVIDIA only)
cargo build --workspace --features crush-gpu/cuda
```

## Test

```bash
# Run all crush-gpu tests (single-threaded for GPU stability)
cargo test --package crush-gpu -- --test-threads=1

# Run specific GDeflate roundtrip tests
cargo test --package crush-gpu gdeflate -- --test-threads=1

# Run backward compatibility tests (v1 LZ77 files)
cargo test --package crush-gpu backward_compat -- --test-threads=1
```

## Benchmark

```bash
# Throughput benchmarks (includes GDeflate vs LZ77 comparison)
cargo bench --package crush-gpu --bench throughput

# Compression ratio benchmarks
cargo bench --package crush-gpu --bench ratio
```

## Usage

The API is unchanged from the user's perspective:

```rust
use crush_gpu::engine::{compress, decompress, EngineConfig};
use std::sync::atomic::AtomicBool;

let cancel = AtomicBool::new(false);
let config = EngineConfig::default();

// Compress (now produces GDeflate format v2)
let compressed = compress(b"Hello, world!", &config, &cancel)?;

// Decompress (auto-detects format version, uses GPU if available)
let decompressed = decompress(&compressed, &config, &cancel)?;
assert_eq!(b"Hello, world!", decompressed.as_slice());

// Force CPU-only decompression
let cpu_config = EngineConfig { force_cpu: true, ..config };
let decompressed_cpu = decompress(&compressed, &cpu_config, &cancel)?;
```

## Key Files

| File | Purpose |
|------|---------|
| `crush-gpu/src/gdeflate.rs` | GDeflate CPU compressor + decompressor |
| `crush-gpu/src/shader/gdeflate_decompress.wgsl` | GPU decompression compute shader |
| `crush-gpu/src/backend/wgpu_backend.rs` | GPU pipeline (LZ77 + GDeflate) |
| `crush-gpu/src/engine.rs` | Orchestration (format version dispatch) |
| `crush-gpu/src/format.rs` | File format (v1/v2 versioning) |
