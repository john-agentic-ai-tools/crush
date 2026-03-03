# Plugin API Contract: GPU Compression Engine

**Feature**: `008-gpu-compression-engine`
**Date**: 2026-02-23

## Plugin Registration

The `crush-gpu` crate registers itself into the crush-core plugin system at compile-time using `linkme` distributed slices, following the exact pattern established by `crush-parallel`.

### Magic Number

```text
Plugin ID: 0x03
Magic:     [0x43, 0x52, 0x01, 0x03]
           "C"    "R"   v1    gpu-deflate
```

### PluginMetadata

```text
name:              "gpu-deflate"
version:           <CARGO_PKG_VERSION>
magic_number:      [0x43, 0x52, 0x01, 0x03]
throughput:        2000.0 (MB/s, estimated GPU decompression)
compression_ratio: 0.65 (same as DEFLATE — format is compatible)
description:       "GPU-accelerated tile-based compression with 32-way parallel decompression"
```

## CompressionAlgorithm Trait Implementation

### `fn name() -> &'static str`

Returns `"gpu-deflate"`.

### `fn metadata() -> PluginMetadata`

Returns metadata above. `throughput` reflects GPU decompression speed (primary performance advantage).

### `fn compress(&self, input: &[u8], cancel_flag: Arc<AtomicBool>) -> Result<Vec<u8>>`

**Preconditions**:
- `input.len() > 0`
- Plugin has already been selected (eligibility check passed)

**Behavior**:
1. Discover GPU and select backend
2. Split input into 64KB tiles
3. For each tile: LZ77 match → Huffman encode → interleave 32 sub-streams
4. Write GpuFileHeader + tiles (128-byte aligned) + TileIndex + GpuFileFooter
5. Check `cancel_flag` after each tile batch; return `Err(CrushError::Cancelled)` if set

**Postconditions**:
- Returns `Vec<u8>` containing complete GPU archive (without Crush outer header — crush-core adds that)
- Archive is decompressible by this plugin's `decompress` method
- Archive is decompressible by CPU fallback (no GPU required)

**Errors**:
- `CrushError::Cancelled` — cancel flag was set
- `PluginError::OperationFailed` — GPU error, compression failure
- `CrushError::Io` — file I/O error

### `fn decompress(&self, input: &[u8], cancel_flag: Arc<AtomicBool>) -> Result<Vec<u8>>`

**Preconditions**:
- `input` contains a valid GPU archive (GpuFileHeader verified by magic)
- Crush outer header has already been stripped by crush-core

**Behavior**:
1. Read GpuFileFooter (last 24 bytes of input)
2. Read TileIndex
3. Read GpuFileHeader
4. If GPU available: decompress tiles on GPU (32 threads per tile)
5. If no GPU: decompress tiles on CPU using rayon (1 tile per CPU thread)
6. Validate CRC32 per tile (if checksums enabled)
7. Check `cancel_flag` after each tile; return `Err(CrushError::Cancelled)` if set

**Postconditions**:
- Returns `Vec<u8>` of original uncompressed data
- Output is byte-for-byte identical regardless of GPU/CPU decompression path

**Errors**:
- `CrushError::Cancelled` — cancel flag was set
- `ValidationError::CorruptedData` — CRC32 mismatch, invalid tile version
- `CrushError::InvalidFormat` — malformed header, unsupported format version
- `PluginError::OperationFailed` — GPU error

### `fn detect(&self, file_header: &[u8]) -> bool`

**Behavior**:
Returns `true` if `file_header` starts with the CGPU magic bytes `[0x43, 0x47, 0x50, 0x55]`.

**Note**: This method is called during compression-time file type detection. For decompression routing, crush-core uses the Crush outer header's magic number `[0x43, 0x52, 0x01, 0x03]`.

## Eligibility Scoring Contract

The GPU plugin overrides default scoring behavior. When the plugin selector queries candidates, the GPU plugin's score depends on three runtime checks:

```text
score = 0.0  if file_size <= 100MB
score = 0.0  if no compatible GPU (Vulkan 1.2/Metal 2 + 2GB VRAM)
score = 0.0  if Shannon entropy > 7.5 bits/byte
score = 0.95 if all three conditions pass (high score to prefer GPU when eligible)
```

The high score (0.95) ensures the GPU plugin is preferred over CPU-based plugins for eligible files, since its throughput metadata (2000 MB/s) will dominate the plugin selector's scoring algorithm.

## ComputeBackend Trait Contract

Internal trait (not exposed via crush-core):

```text
trait ComputeBackend: Send + Sync {
    fn name(&self) -> &str;
    fn gpu_info(&self) -> &GpuInfo;
    fn decompress_tiles(&self, tiles: &[CompressedTile], cancel: &AtomicBool) -> Result<Vec<Vec<u8>>>;
    fn release(&self);
}
```

### WgpuBackend

- Creates `wgpu::Device` and `wgpu::Queue` on initialization
- Loads WGSL compute shaders for decompression
- Dispatches one workgroup (32 threads) per tile
- Transfers compressed tiles to GPU buffer → dispatches → reads back decompressed tiles
- GPU memory budget: 256MB maximum (batches tiles if total exceeds budget)

### CudaBackend (feature-gated: `cuda`)

- Creates CUDA context via `cudarc`
- Compiles PTX kernel at runtime via nvrtc
- Dispatches one block (32 threads) per tile
- Uses CUDA streams for async memory transfer
- GPU memory budget: 256MB maximum
