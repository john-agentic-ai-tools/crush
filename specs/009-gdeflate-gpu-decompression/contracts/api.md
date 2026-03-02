# API Contracts: GDeflate GPU Decompression

**Date**: 2026-03-01
**Feature**: 009-gdeflate-gpu-decompression

## Public API (crush-gpu crate)

### Unchanged Entry Points

The public API surface does not change. Users call the same `compress()` and `decompress()` functions:

```rust
// crush_gpu::engine
pub fn compress(input: &[u8], config: &EngineConfig, cancel: &AtomicBool) -> Result<Vec<u8>>;
pub fn decompress(input: &[u8], config: &EngineConfig, cancel: &AtomicBool) -> Result<Vec<u8>>;
pub fn load_tile_index(archive: &[u8]) -> Result<TileIndex>;
pub fn decompress_tile_by_index(archive: &[u8], tile_index: &TileIndex, index: usize, config: &EngineConfig) -> Result<Vec<u8>>;
```

### New Module: `gdeflate`

```rust
// crush_gpu::gdeflate — GDeflate compressor/decompressor

/// Compress a single 64KB tile into GDeflate bitstream format.
/// Returns the GDeflate-encoded payload (32 interleaved sub-streams).
pub fn gdeflate_compress_tile(tile_data: &[u8]) -> Result<Vec<u8>>;

/// Decompress a single GDeflate tile on CPU (fallback path).
/// Input is the raw GDeflate bitstream payload.
pub fn gdeflate_decompress_tile(payload: &[u8], uncompressed_size: usize) -> Result<Vec<u8>>;
```

### Modified: `format.rs` Constants

```rust
// crush_gpu::format

/// Format version for LZ77 encoding (original).
pub const FORMAT_VERSION_LZ77: u32 = 1;

/// Format version for GDeflate encoding (new).
pub const FORMAT_VERSION_GDEFLATE: u32 = 2;

/// Current default format version for new compressions.
pub const FORMAT_VERSION: u32 = FORMAT_VERSION_GDEFLATE;

/// Tile header version for LZ77 tiles.
pub const TILE_VERSION_LZ77: u8 = 1;

/// Tile header version for GDeflate tiles.
pub const TILE_VERSION_GDEFLATE: u8 = 2;
```

### Modified: `GpuFileHeader::from_bytes()`

```rust
// Accept both v1 (LZ77) and v2 (GDeflate)
pub fn from_bytes(b: &[u8; Self::SIZE]) -> Result<Self> {
    // ...
    let format_version = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
    if format_version != FORMAT_VERSION_LZ77 && format_version != FORMAT_VERSION_GDEFLATE {
        return Err(CrushError::VersionMismatch { ... });
    }
    // ...
}
```

### Modified: `ComputeBackend` trait

```rust
// crush_gpu::backend::ComputeBackend

pub trait ComputeBackend: Send + Sync {
    fn name(&self) -> &str;
    fn gpu_info(&self) -> &GpuInfo;

    /// Decompress LZ77-encoded tiles (format version 1).
    fn decompress_tiles(
        &self,
        tiles: &[CompressedTile],
        cancel: &AtomicBool,
    ) -> Result<Vec<Vec<u8>>>;

    /// Decompress GDeflate-encoded tiles (format version 2).
    fn decompress_tiles_gdeflate(
        &self,
        tiles: &[CompressedTile],
        cancel: &AtomicBool,
    ) -> Result<Vec<Vec<u8>>>;

    fn release(&self);
}
```

### Modified: `WgpuBackend`

The backend creates two compute pipelines at initialization:
- `lz77_pipeline`: Existing LZ77 decompression shader
- `gdeflate_pipeline`: New GDeflate decompression shader

The engine selects which pipeline to use based on the file's `format_version`.

## Internal Contracts

### Engine Dispatch Logic

```rust
// In decompress():
match header.format_version {
    FORMAT_VERSION_LZ77 => {
        // Existing LZ77 path (GPU or CPU)
    }
    FORMAT_VERSION_GDEFLATE => {
        // New GDeflate path (GPU or CPU)
        if !config.force_cpu {
            if let Ok(Some(backend)) = discover_gpu() {
                match backend.decompress_tiles_gdeflate(&tiles, cancel) {
                    Ok(output) => return Ok(output),
                    Err(e) => eprintln!("GPU failed: {e}"),
                }
            }
        }
        decompress_tiles_cpu_gdeflate(input, &header, &entries, config, cancel)
    }
    _ => Err(CrushError::VersionMismatch { ... })
}
```

### Compression Output Contract

```rust
// In compress():
// Always produce GDeflate format (v2) for new compressions
let header = GpuFileHeader {
    format_version: FORMAT_VERSION_GDEFLATE,
    // ...
};

// Each tile:
let tile_header = TileHeader {
    version: TILE_VERSION_GDEFLATE,
    // ...
};
let payload = gdeflate::gdeflate_compress_tile(tile_data)?;
```
