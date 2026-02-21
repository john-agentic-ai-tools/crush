# Data Model: Parallel Compression Engine (007)

**Branch**: `007-parallel-gzip-engine`
**Date**: 2026-02-21
**Derived from**: spec.md (Key Entities), research.md (Format Design decisions)

---

## Entities

### FileHeader

Represents the fixed 64-byte header at the start of every `.crsh` file.

| Field | Rust Type | Bytes | Description |
|---|---|---|---|
| `magic` | `[u8; 4]` | 4 | `[0x43, 0x52, 0x53, 0x48]` ("CRSH") |
| `format_version` | `u32` | 4 | Format version. Currently `1`. Engine rejects files where this ≠ current. |
| `engine_version` | `EngineVersion` | 8 | Packed semver of the producing engine (see below). |
| `block_size` | `u32` | 4 | Nominal uncompressed block size in bytes. |
| `compression_level` | `u8` | 1 | DEFLATE level 0–9. |
| `flags` | `FileFlags` | 1 | Bitfield (see below). |
| `reserved` | `[u8; 2]` | 2 | Must be `0x00`. |
| `uncompressed_size` | `u64` | 8 | Total uncompressed bytes. `u64::MAX` = unknown (streaming). |
| `block_count` | `u64` | 8 | Total block count. `u64::MAX` = unknown (streaming). |
| `_reserved` | `[u8; 24]` | 24 | Future use; must be zero. |
| **Total** | | **64** | |

**`EngineVersion`** (packed into 8 bytes):

| Field | Type | Bytes |
|---|---|---|
| `major` | `u16` | 2 |
| `minor` | `u16` | 2 |
| `patch` | `u16` | 2 |
| `pre` | `u8` | 1 |
| `build` | `u8` | 1 |

**`FileFlags`** (1 byte, bitfield):

| Bit | Name | Meaning |
|---|---|---|
| 0 | `checksums_enabled` | Per-block CRC32 checksums are present and validated |
| 1 | `streaming` | File was produced from a stream (uncompressed_size/block_count may be `u64::MAX`) |
| 2–7 | reserved | Must be 0 |

**Invariants**:
- `magic` must equal `[0x43, 0x52, 0x53, 0x48]` or the file is rejected.
- `format_version` must equal the engine's supported version or the file is rejected with a `VersionMismatch` error carrying both versions.
- `block_size` must be in range `[65536, 268435456]` (64 KB – 256 MB).

---

### BlockHeader

Fixed 16-byte header preceding each compressed block's payload. Stored inline in the file.

| Field | Rust Type | Bytes | Description |
|---|---|---|---|
| `compressed_size` | `u32` | 4 | Byte length of the payload following this header. 0 if `stored` flag is set. |
| `uncompressed_size` | `u32` | 4 | Byte length of the original input data for this block. |
| `checksum` | `u32` | 4 | CRC32 (IEEE) of the **uncompressed** block data. 0 if `FileFlags::checksums_enabled` is unset. |
| `flags` | `BlockFlags` | 1 | Bitfield (see below). |
| `_reserved` | `[u8; 3]` | 3 | Must be `0x00`. |
| **Total** | | **16** | |

**`BlockFlags`** (1 byte, bitfield):

| Bit | Name | Meaning |
|---|---|---|
| 0 | `stored` | Block is stored raw (uncompressed). Payload is original bytes verbatim. |
| 1 | `gpu_compressed` | Block was compressed by GPU path. Output is identical to CPU path; bit is informational. |
| 2–7 | reserved | Must be 0 |

**Invariants**:
- `uncompressed_size` must be ≤ `FileHeader::block_size` (except the final block, which may be smaller).
- If `stored = 1`, `compressed_size` must equal `uncompressed_size`.
- If `checksums_enabled = 1`, `checksum` must match `crc32fast::hash(decompressed_block)` or the block is rejected as corrupt.

---

### BlockIndexEntry

One entry in the trailing block index. 20 bytes each.

| Field | Rust Type | Bytes | Description |
|---|---|---|---|
| `block_offset` | `u64` | 8 | Absolute byte offset of the `BlockHeader` from start of file. Enables O(1) seek. |
| `compressed_size` | `u32` | 4 | Mirrors `BlockHeader::compressed_size`. |
| `uncompressed_size` | `u32` | 4 | Mirrors `BlockHeader::uncompressed_size`. |
| `checksum` | `u32` | 4 | Mirrors `BlockHeader::checksum`. |
| **Total** | | **20** | |

**Notes**:
- The index redundantly stores `compressed_size`, `uncompressed_size`, and `checksum` from each `BlockHeader`. This avoids seeking into block headers to compute uncompressed offset ranges for random access.
- `uncompressed_offset` of block `N` = `sum(entry[0..N].uncompressed_size)`. This is O(N) but only needed when the caller requests a byte-offset seek rather than a block-index seek.

---

### IndexHeader

8-byte header immediately before the block index entries.

| Field | Rust Type | Bytes | Description |
|---|---|---|---|
| `entry_count` | `u32` | 4 | Number of `BlockIndexEntry` records that follow. |
| `index_flags` | `u32` | 4 | Reserved, must be 0. |
| **Total** | | **8** | |

---

### FileFooter

Fixed 24-byte record at the very end of every `.crsh` file.

| Field | Rust Type | Bytes | Description |
|---|---|---|---|
| `index_offset` | `u64` | 8 | Absolute byte offset of the `IndexHeader` from start of file. |
| `index_size` | `u32` | 4 | Byte length of the index region: `8 + 20 * block_count`. |
| `footer_checksum` | `u32` | 4 | CRC32 of bytes `[0..12]` of this footer. Detects footer corruption. |
| `format_version` | `u32` | 4 | Redundant copy of `FileHeader::format_version`. Enables footer-only parsing. |
| `magic` | `[u8; 4]` | 4 | `[0x43, 0x52, 0x53, 0x48]` ("CRSH"). Validates file is not truncated. |
| **Total** | | **24** | |

**Read algorithm**:
1. Seek to `file_size - 24`.
2. Read `FileFooter`.
3. Validate `magic` and `format_version`.
4. Validate `footer_checksum == crc32fast::hash(&footer_bytes[0..12])`.
5. Seek to `index_offset` and read `index_size` bytes (the `IndexHeader` + all `BlockIndexEntry` records).
6. Any block `N` can now be decompressed by seeking to `entry[N].block_offset`.

---

### EngineConfiguration

Builder-pattern configuration struct passed to compression and decompression entry points.

| Field | Rust Type | Default | Description |
|---|---|---|---|
| `workers` | `usize` | `0` (= num_cpus) | Number of rayon worker threads. 0 = use rayon default (logical CPU count). |
| `block_size` | `u32` | `1_048_576` (1 MB) | Uncompressed block size in bytes. Range: 64 KB – 256 MB. |
| `compression_level` | `u8` | `6` | DEFLATE level 0–9. |
| `max_expansion_ratio` | `f64` | `1.0` | If `compressed_size / uncompressed_size > max_expansion_ratio`, store block raw. Set to `f64::INFINITY` to disable. |
| `max_decompression_ratio` | `f64` | `1024.0` | During decompression, if total decompressed bytes would exceed `compressed_file_size * max_decompression_ratio`, halt with `ExpansionLimitExceeded`. Set to `f64::INFINITY` to disable. |
| `checksums` | `bool` | `true` | Enable per-block CRC32 checksums. |
| `gpu` | `bool` | `false` | Attempt GPU-accelerated compression (no-op if `gpu` feature disabled or no adapter found). |
| `progress` | `Option<Arc<Mutex<ProgressCallback>>>` | `None` | Optional progress callback. |

**Builder pattern**:
```rust
let config = EngineConfiguration::builder()
    .workers(8)
    .block_size(2 * 1024 * 1024)
    .compression_level(6)
    .max_decompression_ratio(512.0)
    .progress(Arc::new(Mutex::new(my_callback)))
    .build()?;
```

**Validation** (enforced in `build()`):
- `block_size` in `[65536, 268435456]`
- `compression_level` in `[0, 9]`
- `max_expansion_ratio` > 0.0
- `max_decompression_ratio` > 0.0

---

### ProgressEvent

Data payload delivered to the progress callback after each block completes.

| Field | Rust Type | Description |
|---|---|---|
| `bytes_processed` | `u64` | Cumulative uncompressed bytes processed so far. |
| `blocks_completed` | `u64` | Number of blocks fully compressed/decompressed. |
| `total_blocks` | `Option<u64>` | Total blocks in operation. `None` for streaming (unknown total). |
| `phase` | `ProgressPhase` | `Compressing` or `Decompressing`. |

**`ProgressCallback` type alias**:
```rust
pub type ProgressCallback = Box<dyn FnMut(ProgressEvent) -> bool + Send>;
```
Returning `false` signals cancellation. The engine sets the shared `AtomicCancellationToken` and returns `CrushError::Cancelled` once all in-flight blocks complete.

---

### CompressionBlock (runtime, not persisted)

In-memory representation of a block during compression/decompression. Not stored on disk.

| Field | Rust Type | Description |
|---|---|---|
| `index` | `usize` | Block ordinal (0-based). Used to write output in order. |
| `input` | `&[u8]` | Slice of the input data for this block. |
| `compressed` | `Vec<u8>` | Output of `compress_block`. Empty until compression completes. |
| `checksum` | `u32` | CRC32 of `input`. Computed before compression. |
| `stored` | `bool` | True if compressed size exceeds expansion limit. |

---

## State Transitions

### Compression Operation

```
[Idle]
  │
  ├─ validate config ──────────────────────────────► [Error: InvalidConfig]
  │
  ▼
[Reading Input]
  │  split into blocks
  ▼
[Compressing Blocks] ──── cancel signal ──────────► [Cancelled]
  │  rayon par_iter                  (at next block boundary)
  │  per-block: compress → checksum → check expansion
  ▼
[Writing Output]
  │  blocks written in order
  │  index built
  │  footer written
  ▼
[Complete] → Ok(compressed_bytes or output_path)
```

### Decompression Operation

```
[Idle]
  │
  ├─ read footer → validate magic/version ────────► [Error: VersionMismatch | InvalidFormat]
  │
  ▼
[Index Loaded]
  │
  ├─ check expansion ratio ───────────────────────► [Error: ExpansionLimitExceeded]
  │
  ▼
[Decompressing Blocks] ──── cancel signal ────────► [Cancelled]
  │  rayon par_iter over index entries
  │  per-block: seek → read → decompress → verify checksum
  ▼
[Complete] → Ok(decompressed_bytes or output_path)
```

### Random Access (Seek + Single Block)

```
[Index Loaded]
  │
  ├─ lookup entry[N].block_offset ─────────────────► O(1)
  │
  ▼
[Seek to block_offset]
  │
  ▼
[Read BlockHeader + Payload]
  │
  ▼
[Decompress single block] ──► verify checksum ────► [Error: ChecksumMismatch(block_index)]
  │
  ▼
[Return decompressed bytes]
```

---

## File Layout (Complete)

```
Offset 0:
  FileHeader (64 bytes)

Offset 64:
  Block 0
    BlockHeader (16 bytes)
    Payload     (compressed_size bytes)

Offset 64 + 16 + block0.compressed_size:
  Block 1
    BlockHeader (16 bytes)
    Payload     (compressed_size bytes)

  ... (N blocks total) ...

Offset X:
  IndexHeader (8 bytes)
  BlockIndexEntry[0]  (20 bytes)
  BlockIndexEntry[1]  (20 bytes)
  ...
  BlockIndexEntry[N-1] (20 bytes)

Offset X + 8 + 20*N:
  FileFooter (24 bytes)   ← last 24 bytes of file

Total index region size: 8 + 20 * N bytes
Total file overhead: 64 (header) + N*16 (block headers) + 8 + 20*N (index) + 24 (footer)
                   = 96 + 36*N bytes
For N=1000 blocks (1 GB at 1 MB/block): 36,096 bytes overhead (~0.0034%)
```
