# Data Model: GDeflate GPU Decompression

**Date**: 2026-03-01
**Feature**: 009-gdeflate-gpu-decompression

## Entities

### GpuFileHeader (64 bytes) — Modified

Existing structure with `format_version` field extended to accept both v1 (LZ77) and v2 (GDeflate).

| Field | Type | Size | Notes |
|-------|------|------|-------|
| magic | `[u8; 4]` | 4B | `"CGPU"` (unchanged) |
| format_version | `u32` | 4B | **Changed**: now 1 (LZ77) or 2 (GDeflate) |
| engine_version | `EngineVersion` | 8B | Unchanged |
| tile_size | `u32` | 4B | 65536 (unchanged) |
| sub_stream_count | `u8` | 1B | 32 (unchanged) |
| flags | `GpuFileFlags` | 1B | Unchanged |
| _reserved | `[u8; 2]` | 2B | Unchanged |
| uncompressed_size | `u64` | 8B | Unchanged |
| tile_count | `u64` | 8B | Unchanged |
| _reserved2 | `[u8; 24]` | 24B | Unchanged |

### TileHeader (32 bytes) — Modified

The `version` byte now distinguishes per-tile encoding format.

| Field | Type | Size | Notes |
|-------|------|------|-------|
| version | `u8` | 1B | **Changed**: 1=LZ77, 2=GDeflate |
| flags | `TileFlags` | 1B | Unchanged |
| sub_stream_count | `u8` | 1B | 32 (unchanged) |
| _reserved | `u8` | 1B | Unchanged |
| compressed_size | `u32` | 4B | Unchanged |
| uncompressed_size | `u32` | 4B | Unchanged |
| checksum | `u32` | 4B | CRC32 of uncompressed data |
| sub_stream_offsets_size | `u32` | 4B | 0 for GDeflate (offsets embedded in bitstream) |
| _reserved2 | `[u8; 12]` | 12B | Unchanged |

### GDeflate Tile Payload (Variable Size)

For GDeflate tiles (version=2), the payload follows the GDeflate bitstream specification:

```
[128 bytes: initial state for 32 sub-streams (32 × u32)]
[variable: interleaved compressed bitstream data]
```

The payload is self-describing — the bitstream contains DEFLATE block headers with Huffman tables inline. No external offset table is needed (unlike the LZ77 format which has a sub-stream offset table prefix).

### GPU Shader Buffers (New)

GDeflate dispatch uses different buffer bindings than LZ77:

| Binding | WGSL Type | Purpose |
|---------|-----------|---------|
| @binding(0) | `var<storage, read> compressed: array<u32>` | Compressed input data |
| @binding(1) | `var<storage, read_write> control: array<u32>` | Stream metadata: count, per-stream offsets |
| @binding(2) | `var<storage, read_write> output: array<u32>` | Decompressed output |
| @binding(3) | `var<storage, read_write> scratch: array<u32>` | Work-stealing tile indices |

### Workgroup Shared Memory

| Variable | Size | Purpose |
|----------|------|---------|
| `g_tmp` | 128B (32 × u32) | Scratch for prefix sums, broadcasts |
| `g_buf` | 256B (64 × u32) | Code length storage (4-bit packed) |
| `g_lut` | 1280B (320 × u32) | Huffman symbol lookup table |

**Total**: ~1.7 KB shared memory per workgroup

## Relationships

```
GpuFileHeader (1) ──contains──> format_version (1=LZ77, 2=GDeflate)
                 ──references──> TileHeader[0..N]

TileHeader (1)  ──contains──> version (1=LZ77, 2=GDeflate)
                ──followed-by──> Tile Payload (LZ77 or GDeflate format)

TileIndexEntry (N) ──points-to──> TileHeader[i] (via tile_offset)

GpuFileFooter (1) ──points-to──> TileIndexHeader (via index_offset)
                   ──contains──> format_version (redundant copy)
```

## Validation Rules

- `format_version` MUST be 1 or 2 (reject anything else with `VersionMismatch`)
- `tile_header.version` MUST match file-level `format_version`
- GDeflate tile payload MUST be at least 128 bytes (32 × u32 initial state)
- `sub_stream_count` MUST be 32 for GDeflate tiles
- `uncompressed_size` per tile MUST NOT exceed 65536 (64KB)
- `checksum` validation unchanged (CRC32 of uncompressed data)

## State Transitions

```
Compression:  Raw Data → Split into 64KB tiles → GDeflate encode per tile → Write v2 header + tiles + index + footer
Decompression (GPU):  Read v2 header → Load tiles → GPU dispatch (GDeflate shader) → Verify checksums → Assemble output
Decompression (CPU):  Read v2 header → Load tiles → CPU GDeflate decode per tile → Verify checksums → Assemble output
Decompression (v1):   Read v1 header → Load tiles → CPU/GPU LZ77 decode (existing path) → Verify checksums → Assemble output
```
