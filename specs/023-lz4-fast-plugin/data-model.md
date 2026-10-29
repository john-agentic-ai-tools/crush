# Data Model: LZ4-Class Fast Compression Plugin

Phase 1 of [plan.md](plan.md). Byte layout details are in [contracts/fast-lz-format.md](contracts/fast-lz-format.md).

## FastLz stream (plugin payload)

What `fast-lz`'s `compress` returns and `decompress` accepts. crush-core wraps it in the standard Crush outer format (16-byte header with magic `43 52 01 04`, original size, CRC32 of payload, optional file metadata).

| Field | Type | Rules |
|---|---|---|
| frame header | `FrameHeader` | first 8 bytes |
| blocks | `Block`* | in input order; each covers `block_size` input bytes except the last |
| end marker | 4 zero bytes | required; nothing may follow it |

Validation on decode: total decoded length must equal the outer header's original size; any trailing bytes, missing end marker, or size mismatch is `ValidationError::CorruptedData`.

## FrameHeader

| Field | Type | Rules |
|---|---|---|
| `magic` | `[u8; 2]` = `"LZ"` | rejects anything else |
| `version` | `u8` | `1`; other values rejected (no backward compat on branches, per constitution) |
| `flags` | `u8` | `0` in v1; non-zero rejected |
| `block_size_log2` | `u8` | 16..=20 (64 KiB .. 1 MiB); default 20 |
| `reserved` | `[u8; 3]` | must be zero |

## Block

| Field | Type | Rules |
|---|---|---|
| `compressed_len` | `u32` LE, low 31 bits | 1 ..= bound(block_size); 0 only as end marker |
| `stored` | top bit of the same `u32` | 1 = payload is raw input bytes |
| `uncompressed_len` | `u32` LE | 1 ..= block_size; equals `compressed_len` when stored |
| `payload` | `compressed_len` bytes | sequences (below) or raw bytes |

State: none across blocks (each block decodes on its own; back-references never cross a block boundary).

## Sequence (inside a compressed block)

| Part | Encoding | Rules |
|---|---|---|
| token | 1 byte: high nibble literal length L (0–15), low nibble match length M−4 (0–15) | |
| extended literal length | if L = 15: bytes added while byte = 255, final byte < 255 | total must fit remaining block input |
| literals | L bytes | copied verbatim |
| offset | `u16` LE | 1 ..= bytes already produced in this block |
| extended match length | if M−4 = 15: same scheme | match must not exceed the block's declared `uncompressed_len` |

The last sequence of a block has literals only (no offset/match), ending exactly at `compressed_len`.

## Compressor state (per worker, reused across blocks and inputs)

| Item | Size | Notes |
|---|---|---|
| hash table | 2^14 × `u32` = 64 KiB | positions relative to a running base; never cleared between blocks |
| output scratch | ≤ bound(1 MiB) ≈ 1.004 MiB | reused |

Total ≈ 1.1 MiB per thread, well below the 32 MB limit (FR-012).

## Selection estimate

| Item | Rule |
|---|---|
| sample count / size | `n = clamp(input_len / 64 / 16 KiB, 4, 16)` samples of `min(16 KiB, input_len / n)` bytes, evenly spaced over the whole input |
| estimate | `Σ sample_len / Σ compressed_sample_len` using the plugin's own block compressor |
| inputs < 4 × 4 KiB | estimate = whole-input compression (cheap at this size) |
| eligibility threshold | ≥ 1.10 (`FAST_LZ_MIN_ESTIMATED_RATIO`) |

## Plugin metadata (registry)

| Field | Value |
|---|---|
| name | `fast-lz` |
| magic | `[0x43, 0x52, 0x01, 0x04]` |
| throughput | measured single-core codec MB/s after implementation |
| compression_ratio | compressed/original on the reference corpus (≈ 0.44) |
| content_aware | `true` |
