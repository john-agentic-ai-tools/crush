# Quickstart: GPU Compression Engine

**Feature**: `008-gpu-compression-engine`
**Date**: 2026-02-23

## Prerequisites

- Rust stable toolchain (pinned via `rust-toolchain.toml`)
- A GPU with Vulkan 1.2, Metal 2, or CUDA support and 2GB+ VRAM
- Vulkan SDK (for Vulkan validation layers during development) or Xcode (for Metal on macOS)
- Optional: CUDA toolkit 12.x (for NVIDIA CUDA fast path)

## Setup

### 1. Add the workspace member

Add `crush-gpu` to the workspace `Cargo.toml`:

```toml
[workspace]
members = [
    "crush-core",
    "crush-cli",
    "crush-parallel",
    "crush-gpu",
]
```

### 2. Create the crate

```bash
cargo init crush-gpu --lib
```

### 3. Configure dependencies

`crush-gpu/Cargo.toml`:

```toml
[package]
name = "crush-gpu"
version = "0.1.0"
edition.workspace = true

[features]
default = []
cuda = ["dep:cudarc"]

[dependencies]
crush-core = { version = "0.2.0", path = "../crush-core" }
wgpu = "28.0"
bytemuck = { version = "1.14", features = ["derive"] }
crc32fast = { workspace = true }
memmap2 = { workspace = true }
rayon = { workspace = true }
thiserror = { workspace = true }
linkme = { workspace = true }
cudarc = { version = "0.16", optional = true }
pollster = "0.4"

[dev-dependencies]
criterion = { workspace = true }
proptest = "1.5"
tempfile = "3.8"

[lints.clippy]
all = "deny"
pedantic = "warn"
unwrap_used = "deny"
expect_used = "deny"
panic = "deny"
panic_in_result_fn = "deny"
```

### 4. Verify GPU availability

```bash
# Check Vulkan support
vulkaninfo --summary 2>/dev/null || echo "No Vulkan"

# Check CUDA support (if targeting NVIDIA fast path)
nvidia-smi 2>/dev/null || echo "No CUDA"
```

## Build

```bash
# Standard build (wgpu only — Vulkan/Metal/DX12)
cargo build -p crush-gpu

# With CUDA fast path (requires CUDA toolkit)
cargo build -p crush-gpu --features cuda
```

## Test

```bash
# Run all tests
cargo test -p crush-gpu

# Run with CUDA fast path tests
cargo test -p crush-gpu --features cuda

# Run benchmarks
cargo bench -p crush-gpu
```

## Usage

The plugin registers automatically at compile-time. When `crush-gpu` is a workspace member and linked into the binary, the GPU plugin is available to the crush-core plugin selector.

```rust
use crush_core::plugin::PluginSelector;

// GPU plugin is automatically registered via linkme
let selector = PluginSelector::default();
let best = selector.select().expect("no plugins");

// For files >100MB with a GPU present and compressible data,
// the GPU plugin will be selected automatically
println!("Selected plugin: {}", best.name);
```

## Development Workflow

1. **Write tests first** (TDD — constitution requirement)
2. **Run quality checks**: `cargo fmt && cargo clippy --all-targets -- -D warnings`
3. **Run tests**: `cargo test -p crush-gpu`
4. **Run benchmarks**: `cargo bench -p crush-gpu`
5. **Validate GPU shaders**: Shaders are compiled at wgpu initialization — test on real hardware

## Key Files

| File | Purpose |
|------|---------|
| `src/lib.rs` | Plugin registration, public API |
| `src/engine.rs` | Compress/decompress orchestration |
| `src/format.rs` | GPU tile format (header, tile, index, footer) |
| `src/backend/mod.rs` | ComputeBackend trait, GPU discovery |
| `src/backend/wgpu.rs` | wgpu compute shader backend |
| `src/backend/cuda.rs` | Optional CUDA backend |
| `src/shader/*.wgsl` | WGSL compute shaders |
| `src/scorer.rs` | Eligibility scoring |
| `src/entropy.rs` | Shannon entropy calculation |
| `src/vectorize.rs` | Vectorized string matching (P5) |
