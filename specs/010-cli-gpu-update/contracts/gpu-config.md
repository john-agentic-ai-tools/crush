# GPU Configuration Contract

**Feature**: 010-cli-gpu-update
**Date**: 2026-03-02

## TOML Configuration

### Schema

```toml
[gpu]
enabled = false     # Enable GPU in auto-selection (default: false)
device = -1         # GPU device index (-1 = auto-select, 0+ = specific device)
force-cpu = false   # Force CPU fallback for GPU-format files (default: false)
```

### Location

`~/.config/crush/config.toml` (via `dirs::config_dir()`)

### Keys

| Key | Type | Default | Valid Values | Description |
|-----|------|---------|-------------|-------------|
| `gpu.enabled` | bool | `false` | `true`, `false` | When true, GPU is considered during auto-selection for files >= 25 MB |
| `gpu.device` | integer | `-1` | `-1` (auto), `0`, `1`, ... | GPU device index. -1 means auto-select best available |
| `gpu.force-cpu` | bool | `false` | `true`, `false` | When true, always use CPU fallback for GPU-format files |

### Environment Variables

| Variable | Maps To | Example |
|----------|---------|---------|
| `CRUSH_GPU_ENABLED` | `gpu.enabled` | `CRUSH_GPU_ENABLED=true` |
| `CRUSH_GPU_DEVICE` | `gpu.device` | `CRUSH_GPU_DEVICE=0` |
| `CRUSH_GPU_FORCE_CPU` | `gpu.force-cpu` | `CRUSH_GPU_FORCE_CPU=true` |

### CLI Commands

```bash
# Set GPU preferences
crush config set gpu.enabled true
crush config set gpu.device 0
crush config set gpu.force-cpu true

# Read GPU preferences
crush config get gpu.enabled
crush config get gpu.device

# List all config (includes GPU section)
crush config list
```

### Validation Rules

| Key | Rule | Error on Violation |
|-----|------|--------------------|
| `gpu.enabled` | Must be valid boolean | "Invalid value for gpu.enabled: expected true or false" |
| `gpu.device` | Must be integer >= -1 | "Invalid value for gpu.device: expected integer >= -1" |
| `gpu.force-cpu` | Must be valid boolean | "Invalid value for gpu.force-cpu: expected true or false" |

### Precedence

CLI flags > Environment variables > Config file > Defaults

When conflicts occur:
- `--force-cpu` flag overrides `gpu.force-cpu` config
- `--gpu-device` flag overrides `gpu.device` config
- `--plugin gpu-deflate` flag overrides auto-selection regardless of `gpu.enabled`

## Inter-Crate API Contract

### crush-gpu public API additions

```rust
/// Process-global GPU plugin configuration.
/// Set once at CLI startup via `configure()`.
pub struct GpuPluginConfig {
    pub force_cpu: bool,
    pub device_index: Option<u32>,
}

/// Configure the GPU plugin with CLI/config-derived settings.
/// Must be called before any compression/decompression operations.
/// Can only be called once (uses OnceLock internally).
pub fn configure(config: GpuPluginConfig);

/// Get the current GPU plugin configuration.
/// Returns default config if `configure()` was never called.
pub fn get_config() -> &'static GpuPluginConfig;

/// Re-export of GPU device discovery for CLI `plugins info` command.
pub use backend::{discover_gpu, GpuInfo, GpuVendor};
```

### Usage from crush-cli

```rust
// In main.rs, after config loading:
crush_gpu::configure(crush_gpu::GpuPluginConfig {
    force_cpu: config.gpu.force_cpu || args.force_cpu(),
    device_index: args.gpu_device().or_else(|| {
        if config.gpu.device >= 0 {
            Some(config.gpu.device as u32)
        } else {
            None
        }
    }),
});

// In plugins.rs, for `plugins info gpu-deflate`:
match crush_gpu::discover_gpu() {
    Ok(Some(backend)) => display_gpu_info(backend.gpu_info()),
    Ok(None) => println!("  GPU Device:  Not available"),
    Err(e) => println!("  GPU Device:  Error: {}", e),
}
```
