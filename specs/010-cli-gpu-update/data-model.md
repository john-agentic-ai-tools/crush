# Data Model: CLI GPU Integration Update

**Feature**: 010-cli-gpu-update
**Date**: 2026-03-02

## Entities

### GpuPluginConfig (new — crush-gpu crate)

Process-global configuration for the GPU plugin, set once at CLI startup via `crush_gpu::configure()`.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `force_cpu` | `bool` | `false` | Bypass GPU, use CPU fallback for all GPU operations |
| `device_index` | `Option<u32>` | `None` | Specific GPU device to use. `None` = auto-select best available |

**Lifecycle**: Created at CLI startup, immutable after initialization (`OnceLock`).

**Relationships**: Read by `GpuDeflatePlugin::compress()` and `GpuDeflatePlugin::decompress()` to construct `EngineConfig`.

---

### GpuConfig (new — crush-cli crate)

Configuration section for GPU settings in the user's TOML config file (`~/.config/crush/config.toml`).

| Field | Type | Default | TOML Key | Env Var |
|-------|------|---------|----------|---------|
| `enabled` | `bool` | `false` | `gpu.enabled` | `CRUSH_GPU_ENABLED` |
| `device` | `i32` | `-1` | `gpu.device` | `CRUSH_GPU_DEVICE` |
| `force_cpu` | `bool` | `false` | `gpu.force-cpu` | `CRUSH_GPU_FORCE_CPU` |

**Lifecycle**: Loaded from TOML, merged with env vars, overridden by CLI flags.

**Relationships**: Merged into `GpuPluginConfig` at startup. Part of the `Config` struct in crush-cli.

---

### Config (modified — crush-cli crate)

Extended with a `gpu` field.

| Field | Type | Status |
|-------|------|--------|
| `compression` | `CompressionConfig` | Existing |
| `output` | `OutputConfig` | Existing |
| `logging` | `LoggingConfig` | Existing |
| `gpu` | `GpuConfig` | **NEW** |

---

### CompressArgs (modified — crush-cli crate)

Extended with GPU control flags.

| Field | Type | Status | Flag |
|-------|------|--------|------|
| `input` | `Vec<PathBuf>` | Existing | positional |
| `output` | `Option<PathBuf>` | Existing | `-o` |
| `stdout` | `bool` | Existing | `--stdout` |
| `plugin` | `Option<String>` | Existing | `-p, --plugin` |
| `level` | `CompressionLevel` | Existing | `-l, --level` |
| `force` | `bool` | Existing | `-f, --force` |
| `timeout` | `Option<u64>` | Existing | `--timeout` |
| `gpu_device` | `Option<u32>` | **NEW** | `--gpu-device` |

---

### DecompressArgs (modified — crush-cli crate)

Extended with GPU control flags.

| Field | Type | Status | Flag |
|-------|------|--------|------|
| `input` | `Vec<PathBuf>` | Existing | positional |
| `output` | `Option<PathBuf>` | Existing | `-o` |
| `force` | `bool` | Existing | `-f, --force` |
| `stdout` | `bool` | Existing | `--stdout` |
| `block` | `Option<u64>` | Existing | `--block` |
| `force_cpu` | `bool` | **NEW** | `--force-cpu` |
| `gpu_device` | `Option<u32>` | **NEW** | `--gpu-device` |

---

## State Transitions

### GPU Plugin Configuration Flow

```
CLI Startup
  │
  ├─ load_config() → Config { gpu: GpuConfig { ... } }
  ├─ merge_env_vars() → override gpu.* from CRUSH_GPU_* env vars
  ├─ merge_cli_args() → override with --force-cpu, --gpu-device flags
  │
  ├─ crush_gpu::configure(GpuPluginConfig { ... })  ← SET ONCE (OnceLock)
  │
  └─ Command dispatch
       ├─ compress: GPU plugin reads GpuPluginConfig internally
       ├─ decompress: GPU plugin reads GpuPluginConfig internally
       └─ plugins info: calls discover_gpu() directly
```

### Configuration Precedence (highest to lowest)

1. CLI flags (`--force-cpu`, `--gpu-device`)
2. Environment variables (`CRUSH_GPU_FORCE_CPU`, `CRUSH_GPU_DEVICE`)
3. Config file (`~/.config/crush/config.toml` → `[gpu]` section)
4. Defaults (`enabled=false`, `device=-1`, `force-cpu=false`)
