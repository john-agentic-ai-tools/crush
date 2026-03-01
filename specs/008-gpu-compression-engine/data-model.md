# Data Model: GPU Compression Engine

**Feature**: `008-gpu-compression-engine`
**Date**: 2026-02-23

## Entity Relationship Overview

```text
GpuArchive (file on disk)
├── GpuFileHeader (1:1)
├── GpuTile (1:N)
│   ├── TileHeader (1:1)
│   └── SubStream[32] (1:32)
├── TileIndex (1:1)
│   └── TileIndexEntry (1:N)
└── GpuFileFooter (1:1)

ComputeBackend (runtime)
├── WgpuBackend (variant)
└── CudaBackend (variant, feature-gated)

EligibilityScorer (stateless)
├── SizeCheck
├── GpuCheck
└── EntropyCheck
```

## File Format Entities

### GpuFileHeader (64 bytes)

Fixed header at the start of every GPU-compressed archive.

| Field | Type | Offset | Size | Description |
|-------|------|--------|------|-------------|
| `magic` | `[u8; 4]` | 0 | 4 | `[0x43, 0x47, 0x50, 0x55]` = "CGPU" |
| `format_version` | `u32` | 4 | 4 | Format version (initially 1) |
| `engine_version` | `EngineVersion` | 8 | 8 | Packed semver of producing engine |
| `tile_size` | `u32` | 16 | 4 | Uncompressed tile size (65536 = 64KB) |
| `sub_stream_count` | `u8` | 20 | 1 | Sub-streams per tile (32) |
| `flags` | `GpuFileFlags` | 21 | 1 | Bitfield (see below) |
| `reserved_1` | `[u8; 2]` | 22 | 2 | Must be zero |
| `uncompressed_size` | `u64` | 24 | 8 | Original file size in bytes |
| `tile_count` | `u64` | 32 | 8 | Number of tiles |
| `reserved_2` | `[u8; 24]` | 40 | 24 | Must be zero (future extension) |

**GpuFileFlags** (bitfield, u8):
- Bit 0: `CHECKSUMS_ENABLED` — per-tile CRC32 present
- Bit 1: `VECTORIZE_USED` — vectorized matching was applied
- Bit 2: `ENTROPY_CHECKED` — entropy was validated before compression
- Bits 3-7: Reserved

**Validation Rules**:
- `magic` must equal `[0x43, 0x47, 0x50, 0x55]`
- `format_version` must equal current supported version
- `tile_size` must be 65536 (64KB)
- `sub_stream_count` must be 32
- `uncompressed_size` must be > 0

### TileHeader (32 bytes)

Precedes each compressed tile's payload.

| Field | Type | Offset | Size | Description |
|-------|------|--------|------|-------------|
| `version` | `u8` | 0 | 1 | Tile format version (initially 1) |
| `flags` | `TileFlags` | 1 | 1 | Bitfield (see below) |
| `sub_stream_count` | `u8` | 2 | 1 | Number of sub-streams (32) |
| `reserved_1` | `u8` | 3 | 1 | Must be zero |
| `compressed_size` | `u32` | 4 | 4 | Size of compressed payload (bytes) |
| `uncompressed_size` | `u32` | 8 | 4 | Size after decompression (≤64KB) |
| `checksum` | `u32` | 12 | 4 | CRC32 of uncompressed data (0 if disabled) |
| `sub_stream_offsets_size` | `u32` | 16 | 4 | Size of sub-stream offset table (bytes) |
| `reserved_2` | `[u8; 12]` | 20 | 12 | Must be zero (future extension) |

**TileFlags** (bitfield, u8):
- Bit 0: `STORED` — tile is stored uncompressed (incompressible data)
- Bit 1: `LAST_TILE` — this is the final tile (may be <64KB)
- Bits 2-7: Reserved

**Validation Rules**:
- `version` must be recognized (reject unknown versions per clarification)
- `compressed_size` must be > 0
- `uncompressed_size` must be > 0 and ≤ 65536
- If `STORED` flag set: `compressed_size` must equal `uncompressed_size`

### TilePayload (variable size)

Follows the TileHeader. Structure depends on `STORED` flag.

**Compressed tile**:
```text
sub_stream_offsets: [u32; sub_stream_count]  — byte offset of each sub-stream within payload
sub_stream_data: [u8; ...]                   — interleaved compressed sub-streams
padding: [0x00; ...]                         — pad to 128-byte boundary
```

**Stored tile** (uncompressed):
```text
raw_data: [u8; uncompressed_size]
padding: [0x00; ...]                         — pad to 128-byte boundary
```

### TileIndexEntry (24 bytes)

One entry per tile in the trailing index.

| Field | Type | Offset | Size | Description |
|-------|------|--------|------|-------------|
| `tile_offset` | `u64` | 0 | 8 | Absolute byte offset of TileHeader from file start |
| `compressed_size` | `u32` | 8 | 4 | Compressed tile size (including TileHeader) |
| `uncompressed_size` | `u32` | 12 | 4 | Uncompressed tile size |
| `checksum` | `u32` | 16 | 4 | CRC32 of uncompressed tile data |
| `flags` | `u32` | 20 | 4 | Copy of TileFlags (for index-only access) |

### TileIndexHeader (8 bytes)

Precedes the index entries.

| Field | Type | Offset | Size | Description |
|-------|------|--------|------|-------------|
| `entry_count` | `u32` | 0 | 4 | Number of TileIndexEntry records |
| `index_flags` | `u32` | 4 | 4 | Reserved (must be 0) |

### GpuFileFooter (24 bytes)

Last 24 bytes of the file. Same structure as crush-parallel for consistency.

| Field | Type | Offset | Size | Description |
|-------|------|--------|------|-------------|
| `index_offset` | `u64` | 0 | 8 | Absolute byte offset of TileIndexHeader |
| `index_size` | `u32` | 8 | 4 | Byte length of index region |
| `footer_checksum` | `u32` | 12 | 4 | CRC32 of bytes [0..12] of this footer |
| `format_version` | `u32` | 16 | 4 | Redundant copy of GpuFileHeader::format_version |
| `magic` | `[u8; 4]` | 20 | 4 | `[0x43, 0x47, 0x50, 0x55]` = "CGPU" |

## Runtime Entities

### GpuInfo

Discovered GPU capabilities used for backend selection and eligibility.

| Field | Type | Description |
|-------|------|-------------|
| `vendor` | `GpuVendor` | NVIDIA, AMD, Apple, Intel, Unknown |
| `name` | `String` | Device name (e.g., "NVIDIA GeForce RTX 4090") |
| `vram_bytes` | `u64` | Available video memory in bytes |
| `compute_api` | `ComputeApi` | CUDA, Vulkan, Metal |
| `api_version` | `String` | API version string (e.g., "Vulkan 1.3") |
| `meets_minimum` | `bool` | True if Vulkan 1.2 / Metal 2 + 2GB VRAM |

### EligibilityResult

Output of the eligibility scorer.

| Field | Type | Description |
|-------|------|-------------|
| `eligible` | `bool` | True if all three criteria met |
| `file_size_ok` | `bool` | File > 100MB |
| `gpu_available` | `bool` | Compatible GPU detected |
| `entropy_ok` | `bool` | Shannon entropy ≤ 7.5 bits/byte |
| `entropy_value` | `f64` | Measured Shannon entropy |
| `gpu_info` | `Option<GpuInfo>` | GPU details if available |
| `score` | `f64` | Plugin score for selector (0.0 if ineligible) |

## State Transitions

### Compression Pipeline

```text
[Input File] → SizeCheck (>100MB?)
    ├── No → Decline (return score 0.0 to plugin selector)
    └── Yes → GpuCheck (compatible GPU?)
        ├── No → Decline
        └── Yes → EntropySample (read 1MB, compute Shannon entropy)
            ├── >7.5 bits/byte → Decline
            └── ≤7.5 bits/byte → Accept
                → AllocateGpuResources
                → SplitIntoTiles (64KB each)
                → [Optional] VectorizedMatchingCheck
                │   ├── Beneficial → Use vectorized LZ77
                │   └── Not beneficial → Use standard LZ77
                → CompressTiles (CPU: LZ77 + Huffman + interleave 32 sub-streams)
                → WriteTileHeaders + Payloads (128-byte aligned)
                → WriteTileIndex
                → WriteFooter
                → ReleaseGpuResources
                → [Output GPU Archive]
```

### Decompression Pipeline

```text
[GPU Archive] → ReadFooter (last 24 bytes)
    → ValidateFooter (magic, checksum)
    → ReadTileIndex (seek to index_offset)
    → ReadFileHeader (first 64 bytes)
    → ValidateHeader (magic, version, tile_size)
    → For each tile (parallel via GPU or rayon):
        → ReadTileHeader
        → ValidateTileVersion (reject unknown)
        → If STORED: copy raw data
        → If Compressed:
            → [GPU path] Dispatch 32-thread group per tile
            │   → Each thread decodes its sub-stream
            │   → Reconstruct decompressed tile
            → [CPU path] Sequentially decode 32 sub-streams
            │   → Reconstruct decompressed tile
        → ValidateCRC32 (if checksums enabled)
    → Concatenate tiles
    → [Output Original File]
```
