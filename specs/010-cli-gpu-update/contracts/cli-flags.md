# CLI Flags Contract: GPU Integration

**Feature**: 010-cli-gpu-update
**Date**: 2026-03-02

## New Flags

### `crush compress`

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--gpu-device <INDEX>` | `u32` | Auto-select | Select specific GPU device by index |

**Existing flag that enables GPU**: `--plugin gpu-deflate` (selects the GPU compression engine explicitly).

**Validation rules**:
- `--gpu-device` without `--plugin gpu-deflate`: emit warning "GPU device selection has no effect without `--plugin gpu-deflate`"
- `--gpu-device` with invalid index: error listing available devices

### `crush decompress`

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--force-cpu` | `bool` | `false` | Force CPU-only decompression, bypass GPU even for CGPU-format files |
| `--gpu-device <INDEX>` | `u32` | Auto-select | Select specific GPU device by index |

**Validation rules**:
- `--force-cpu` with non-CGPU file: silently ignored (CPU is already the default path)
- `--force-cpu` and `--gpu-device` together: `--force-cpu` takes precedence, `--gpu-device` ignored

### `crush plugins info gpu-deflate`

No new flags. Behavior change: when the plugin name is `gpu-deflate`, the info output includes GPU device details (name, vendor, VRAM, API backend) or a message indicating no GPU is available.

## Output Format Changes

### `crush plugins list` (human format)

GPU plugin appears in the list alongside existing plugins:

```
Available plugins:

  Name              Throughput    Ratio    Description
  ─────────────────────────────────────────────────────
  default           100 MB/s      0.65     Standard DEFLATE compression
  parallel-deflate  500 MB/s      0.65     Multi-threaded parallel DEFLATE
  gpu-deflate       2000 MB/s     0.65     GPU-accelerated GDeflate compression
```

### `crush plugins info gpu-deflate` (with GPU available)

```
Plugin: gpu-deflate
  Version:     1.0.0
  Throughput:  2000 MB/s
  Ratio:       0.65
  Description: GPU-accelerated GDeflate compression

  GPU Device:
    Name:      NVIDIA GeForce RTX 4090
    Vendor:    Nvidia
    VRAM:      24576 MB
    Backend:   Vulkan
```

### `crush plugins info gpu-deflate` (no GPU available)

```
Plugin: gpu-deflate
  Version:     1.0.0
  Throughput:  2000 MB/s
  Ratio:       0.65
  Description: GPU-accelerated GDeflate compression

  GPU Device:  Not available (no compatible GPU detected)
  Note:        CPU fallback will be used for decompression of GPU-compressed files.
```

## Exit Codes

No new exit codes. GPU errors map to existing exit code 1 (operational error).

## Help Text Additions

### `crush compress --help`

```
      --gpu-device <INDEX>  Select GPU device by index (use with --plugin gpu-deflate)
```

### `crush decompress --help`

```
      --force-cpu           Force CPU-only decompression (bypass GPU for CGPU files)
      --gpu-device <INDEX>  Select GPU device by index for decompression
```
