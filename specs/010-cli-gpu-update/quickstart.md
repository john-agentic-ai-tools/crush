# Quickstart: CLI GPU Integration Update

**Feature**: 010-cli-gpu-update
**Date**: 2026-03-02

## Prerequisites

- Rust stable toolchain (pinned via `rust-toolchain.toml`)
- GPU with Vulkan 1.0, Metal, or DX12 support (optional — CPU fallback works without GPU)
- Working `crush` workspace that builds cleanly

## Build

```bash
# Build entire workspace (includes crush-gpu)
cargo build

# Run tests
cargo test

# Verify GPU plugin appears
cargo run -p crush-cli -- plugins list
```

## Development Workflow

### 1. Add crush-gpu dependency to crush-cli

In `crush-cli/Cargo.toml`:
```toml
[dependencies]
crush-gpu = { version = "0.1.0", path = "../crush-gpu" }
```

### 2. Force-link GPU plugin

In `crush-cli/src/main.rs`:
```rust
use crush_gpu as _;
```

### 3. Verify plugin registration

```bash
cargo run -p crush-cli -- plugins list
# Should show: default, parallel-deflate, gpu-deflate
```

### 4. Test GPU compression

```bash
# Explicit GPU selection
cargo run -p crush-cli -- compress --plugin gpu-deflate testfile.bin -o testfile.crush

# Decompress (auto-detects CGPU format)
cargo run -p crush-cli -- decompress testfile.crush -o testfile.out

# Force CPU decompression
cargo run -p crush-cli -- decompress --force-cpu testfile.crush -o testfile.out

# Check GPU info
cargo run -p crush-cli -- plugins info gpu-deflate
```

### 5. Test configuration

```bash
# Set GPU preferences
cargo run -p crush-cli -- config set gpu.enabled true
cargo run -p crush-cli -- config set gpu.device 0

# Verify
cargo run -p crush-cli -- config list
```

## Key Files to Modify

| File | What Changes |
|------|-------------|
| `crush-cli/Cargo.toml` | Add `crush-gpu` dependency |
| `crush-cli/src/main.rs` | Force-link, call `crush_gpu::configure()` |
| `crush-cli/src/cli.rs` | Add `--force-cpu`, `--gpu-device` args |
| `crush-cli/src/config.rs` | Add `GpuConfig` struct, env var merging, validation |
| `crush-cli/src/algorithm.rs` | GPU-aware auto-selection |
| `crush-cli/src/commands/compress.rs` | Thread GPU config |
| `crush-cli/src/commands/decompress.rs` | Thread GPU config, `--force-cpu` |
| `crush-cli/src/commands/plugins.rs` | GPU device info display |
| `crush-gpu/src/lib.rs` | Add `configure()`, `GpuPluginConfig`, re-exports |
| `crush-gpu/src/engine.rs` | Read from global config |

## Testing Strategy

- **Integration tests**: Use `assert_cmd` to test CLI flag parsing and behavior
- **Plugin roundtrip**: `crush plugins test gpu-deflate` (already exists)
- **Config tests**: Verify `gpu.*` keys set/get/list correctly
- **No-GPU fallback**: Test that GPU-compressed files decompress on systems without GPU
