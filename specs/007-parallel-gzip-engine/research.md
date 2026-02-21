# Research: Parallel DEFLATE Engine & File Format Design

**Feature**: `007-parallel-gzip-engine`
**Date**: 2026-02-21
**Status**: Complete

---

## Topic 1: Parallel DEFLATE / Block Compression Strategy

### How pigz Achieves Parallel Compression While Remaining Gzip-Compatible

pigz (parallel gzip) is the canonical reference implementation of parallel DEFLATE compression. Its design is constrained by one hard requirement: the output must be a valid gzip file readable by any standard `gzip` decompressor.

#### Block Splitting

pigz divides the input into chunks of a fixed configurable size, defaulting to **128 KB** per block. Each block is handed to a separate worker thread which compresses it independently using zlib's raw DEFLATE interface. The compression of all blocks proceeds concurrently on the available cores.

#### The Dictionary Sync Trick

Standard DEFLATE uses a 32 KB sliding window (the "dictionary") that accumulates history across the entire input. A block compressed at position N in the stream can back-reference bytes up to 32 KB earlier. This means blocks are not truly independent — each block's compression quality depends on the content of the preceding block.

pigz's trick: **load the last 32 KB of the previous (already-compressed) block as a preset dictionary** into zlib before compressing the current block. This is done via `deflateSetDictionary()`. The compressed stream produced is still valid DEFLATE — it just means the decompressor must also have that 32 KB of context available to decode the references. Since gzip is a sequential format, by the time a decompressor reaches block N, it has already decompressed block N-1 and the sliding window contains exactly that dictionary. The trick costs nothing at decompression time and preserves compression ratio close to that of single-threaded gzip.

This feature can be disabled with pigz's `-i` / `--independent` flag. With `--independent`, each block is compressed with a cold dictionary (empty context). This enables partial error recovery and random access at the cost of a 5–15% ratio penalty on typical data.

#### The Z_SYNC_FLUSH Byte-Alignment Trick

DEFLATE bit-streams do not end on byte boundaries. When concatenating compressed blocks from different threads, you cannot simply join the raw bit streams — the bit offset at the end of block N must be known to parse block N+1.

pigz solves this by flushing each partial raw DEFLATE stream to a byte boundary using `Z_SYNC_FLUSH`. This inserts an **empty stored block** (4–5 bytes of overhead) at the end of each worker's output. The result is that each block's compressed bytes end exactly on a byte boundary and can be concatenated with the next block's bytes as a simple byte-wise append. A single writer thread concatenates all blocks in order and wraps them in a gzip header/trailer.

The CRC32 and total uncompressed-size fields in the gzip trailer are computed by combining per-block values, also in parallel.

#### What This Means for Crush (No Gzip Compatibility Required)

Since Crush does not need to produce gzip-compatible output, **both tricks are unnecessary overhead**:

- The **preset dictionary trick** exists only to maintain compression ratio across the arbitrary gzip block boundary. We can instead choose a block size large enough that the cold-dictionary penalty is minimal, or we can use a different strategy entirely (see Block Size section below).
- The **Z_SYNC_FLUSH padding** exists only to byte-align blocks so they can be concatenated into a valid DEFLATE bit-stream. Since we own the format, we can store each block's compressed bytes as an opaque blob with an explicit length prefix and start each block with a fresh DEFLATE stream. No padding required.

The simplified Crush approach:

1. Split input into fixed-size blocks (configurable, default 1 MB — see spec assumption A-004).
2. Compress each block independently using a **fresh DEFLATE context** (no preset dictionary, no cross-block references).
3. Store the compressed bytes for each block contiguously in the output, preceded by a length field.
4. Build a block index recording the compressed offset and uncompressed size of every block.

This is equivalent to pigz's `--independent` mode, but without the gzip framing overhead.

### Optimal Block Sizes: Throughput vs Compression Ratio

The block size is the single most important tuning parameter. The tradeoffs are:

| Block Size | Cold-Dictionary Penalty | Parallelism Granularity | Per-Block Overhead | Use Case |
|------------|------------------------|-------------------------|--------------------|----------|
| 32 KB      | ~15–20% worse ratio    | Fine (many small jobs)  | High (relative)    | Random access, small seeks |
| 64 KB      | ~10–12% worse ratio    | Fine                    | Medium             | BGZF default |
| 128 KB     | ~7–10% worse ratio     | Medium                  | Low                | pigz default |
| 256 KB     | ~5–7% worse ratio      | Medium                  | Very low           | Good balance |
| 512 KB     | ~3–5% worse ratio      | Coarser                 | Negligible         | High-throughput pipelines |
| 1 MB       | ~2–3% worse ratio      | Coarsest                | Negligible         | Constitution / spec default |
| 4 MB       | ~1–2% worse ratio      | Very coarse             | None               | Maximum ratio |

Key insight: the cold-dictionary penalty diminishes rapidly with block size because the LZ77 sliding window is only 32 KB wide. A 1 MB block gives the compressor 968 KB of warm-up before the window fills — only the first 32 KB of each block suffers from the cold start. At 1 MB blocks, the penalty is under 3% for typical data.

**Recommendation**: Default to **1 MB blocks** (matching spec assumption A-004). Expose block size as a configurable parameter (FR-007). Document the tradeoff: smaller blocks improve random-access granularity and reduce per-seek I/O, larger blocks improve compression ratio and reduce index overhead.

For the incompressible-data edge case (encrypted, already-compressed data): the spec already requires the engine to store such blocks uncompressed when the compressed output would exceed a threshold (edge case in spec). Each block must carry a flag indicating whether it is stored or compressed.

### How flate2 (Rust) Exposes Raw DEFLATE Block Compression

The `flate2` crate (the allowed dependency — see constitution) provides three distinct compression modes:

- `flate2::write::GzEncoder` / `flate2::read::GzEncoder` — gzip framing (header + trailer)
- `flate2::write::ZlibEncoder` / `flate2::read::ZlibEncoder` — zlib framing (Adler-32 header)
- `flate2::write::DeflateEncoder` / `flate2::read::DeflateEncoder` — **raw DEFLATE, no framing**

For independent block compression, the correct approach is `flate2::write::DeflateEncoder` (or its `read`/`bufread` variants). Each block gets its own `DeflateEncoder` instance, which starts with a fresh compression context (cold dictionary, empty sliding window).

The lower-level `flate2::Compress` struct is also available:

```rust
use flate2::{Compress, Compression, FlushCompress};

let mut compressor = Compress::new(Compression::default(), false); // false = raw deflate (no zlib header)
let mut output = Vec::new();
compressor.compress_vec(input_block, &mut output, FlushCompress::Finish)?;
```

The `Compress::new(level, zlib_header: bool)` constructor with `zlib_header = false` gives raw DEFLATE output. `FlushCompress::Finish` terminates the block cleanly, producing a complete DEFLATE stream that can be decompressed standalone.

`Compress` also exposes `set_dictionary(&mut self, dict: &[u8]) -> Result<u32, DeflateError>` which would enable the pigz preset-dictionary trick if we ever needed it. We do not need it for Crush.

### Can Blocks Compress 100% Independently?

Yes. With `zlib_header = false` and `FlushCompress::Finish`, each block is a self-contained DEFLATE stream. The decompressor for block N has zero dependency on any other block. This is the basis for correct parallel decompression.

The only subtlety: DEFLATE's LZ77 algorithm within a block can reference bytes up to 32 KB earlier **within the same block**. This is not a cross-block dependency — the compressor and decompressor for a given block both operate on the same 32 KB window, which is fully contained within the block. Blocks larger than 32 KB benefit from intra-block back-references and compress better than 32 KB blocks.

---

## Topic 2: Parallel-Decompression-Friendly File Format Design

### Survey of Existing Formats

#### BGZF (Blocked GNU Zip Format)

Used in bioinformatics (BAM, VCF) by htslib/samtools. Design constraints: must be a valid gzip file; standard `gzip -d` must work on it.

**Structure**:
- A sequence of independent gzip members, each at most **65,535 bytes** of compressed data.
- Each gzip member uses the gzip Extra Field to embed the field identifier `BC` followed by the compressed block size (including all headers).
- The compressed block size stored in the Extra Field allows a reader to skip forward through the file finding block boundaries without decompressing.
- An external Tabix index file (`.tbi`) stores pairs of `(compressed_offset, uncompressed_offset)` for genomic coordinate-based random access.
- The final block is an empty gzip member (28 bytes of zero-filled EOF marker) so readers can detect end-of-file.

**What Crush can learn**:
- The per-block size field in the header is the minimal metadata needed for parallel decompression.
- The external index is a clean separation of concerns — but for Crush, we embed the index inside the same file for single-file portability.
- BGZF's 64 KB block limit is too small for Crush's throughput targets; it was chosen to fit within a `u16` field in the gzip Extra Field. We have no such constraint.

#### zstd Seekable Format

Facebook's optional extension on top of zstd (`contrib/seekable_format`). Design: place independently compressed zstd frames in sequence, append a seek table at the **end** of the file.

**Structure**:
- A sequence of standard zstd frames, each independently decompressible.
- A **seek table** packed into a zstd Skippable Frame appended at the end:
  - `Number_Of_Frames` (4 bytes, little-endian)
  - Per-frame entries: `Compressed_Size` (4 bytes) + `Decompressed_Size` (4 bytes) + optional `Checksum` (4 bytes, low 32 bits of xxHash64 of uncompressed frame data)
  - `Seek_Table_Footer`: `Number_Of_Frames` (4 bytes) + `Seek_Table_Descriptor` (1 byte, bit 7 = checksum flag) + `Seekable_Magic_Number` (4 bytes = `0x8F92EAB1`)
- The magic number is at the **very end** of the file. A reader checks the last 9 bytes to find the seek table footer, then uses `Backward_Size` to seek to the seek table entries.

**Index placement**: **End of file**. This is the key architectural choice. It avoids any two-pass write (no need to pre-allocate space for an index whose size is unknown before compression finishes). The tradeoff is that a reader must seek to the end of the file to load the index before it can decompress any block.

**What Crush can learn**:
- End-of-file index placement is the dominant industry choice for streaming-write scenarios.
- The Skippable Frame wrapper is elegant — standard zstd tools silently skip it.
- The checksum flag in the descriptor byte is a clean extensibility mechanism.
- Using cumulative compressed sizes (offsets computed by summing `Compressed_Size` entries rather than storing absolute offsets) saves 4 bytes per block at the cost of O(N) index scan to seek to block N. Crush should store **absolute offsets** to enable O(1) random access.

#### XZ Format

Used by `xz`, `7-zip`. The XZ format stores an **Index** section between the last Block and the Stream Footer.

**Structure per Stream**:
- `Stream Header` (12 bytes): magic `\xFD7zXZ\x00`, Stream Flags (2 bytes, encoding compression filters), CRC32.
- Zero or more `Block` sections, each with a Block Header (filter config, compressed size, uncompressed size).
- `Index`: a sequence of `(Unpadded_Size, Uncompressed_Size)` pairs, one per Block, encoded with variable-length integers. Padded to a multiple of 4 bytes. CRC32-protected.
- `Stream Footer` (12 bytes): CRC32, `Backward_Size` (size of the Index as a u32 multiple-of-4), Stream Flags, Footer Magic `YZ`.

**Index placement**: **End of stream** (before the footer). A reader checks the last 12 bytes, finds `Backward_Size`, seeks back to the Index, loads it, then can jump to any Block.

**What Crush can learn**:
- The `Backward_Size` pattern (footer carries a field pointing back to the index size) is clean and compact.
- XZ separates the per-block filter configuration from the stream-level configuration, supporting heterogeneous blocks. Crush can adopt a simpler model where all blocks use the same configuration, stored once in the file header.

#### dictzip

A gzip-compatible format that stores chunks of less than 64 KB, with chunk sizes stored in the gzip Extra Field (`RA` identifier). Designed for dictionary server random access. The Extra Field contains `CHCNT` (chunk count) followed by `CHCNT` 16-bit little-endian compressed sizes.

**What Crush can learn**: dictzip pioneered the per-block size field in the gzip header for random access. The design is now superseded by BGZF (which adds the full block size including headers) and the zstd seekable format. Crush does not need gzip compatibility, so dictzip's approach is of historical interest only.

#### MiGz (LinkedIn)

A Java library that produces valid gzip files with embedded block-size metadata in the Extra Field (same approach as BGZF). Uses the Extra Field to record compressed block sizes, enabling multithreaded decompression.

**What Crush can learn**: confirms the Extra Field pattern works in production at scale. Not directly applicable since Crush is not gzip-compatible.

---

### Format Design Decision

#### Index Placement: End of File

All modern parallel-decompress-friendly formats (zstd seekable, XZ) place the index at the **end** of the compressed file. This is the correct choice for Crush:

**Arguments for end-of-file index**:
1. **Single-pass write**: During compression, block sizes are unknown until each block is compressed. Placing the index at the end allows the compressor to stream blocks to disk as they complete and append the index only when all blocks are finished. No two-pass I/O, no pre-allocation, no seeking back to update a header.
2. **Streaming-friendly compression**: A streaming input source (pipe, network socket) has no known total size, so a front-loaded index cannot be fully written before compression begins.
3. **Industry precedent**: zstd seekable format, XZ, and (via external index files) BGZF all demonstrate this works correctly in production.
4. **Random access still O(1)**: A reader opens the file, seeks to the last N bytes to read the footer, seeks to the index, loads it into memory, then seeks directly to any block. The extra two seeks at open time are negligible for any file large enough to benefit from parallel decompression.

**Arguments for front-loaded index** (rejected):
1. Enables sequential reading without seeking — but sequential reading of a block-indexed format for streaming decompression works fine by reading the index at the end first, then reading blocks forward. The only genuine advantage is for non-seekable output streams (e.g., pipes), which is not a Crush requirement.
2. Simplifies some decompressor implementations — this is a minor engineering convenience that does not outweigh the compression-time cost of a two-pass write.

**Decision: Block index at the end of file**, using a fixed-size footer whose last field is a backward pointer to the start of the index, mirroring XZ's design.

---

## Decisions

### Decision 1: Block Splitting — Fully Independent Blocks

**Approach**: Split input into fixed-size blocks (default 1 MB, configurable). Compress each block with a fresh `flate2::write::DeflateEncoder` (raw DEFLATE, no zlib header, no gzip framing, no preset dictionary). Blocks are 100% independent — decompressing block N requires only the bytes of block N, not any preceding block.

**Rationale**:
- Eliminates the pigz preset-dictionary trick, which exists solely to maintain gzip compatibility under sequential DEFLATE rules.
- At 1 MB block size, the cold-dictionary compression ratio penalty is under 3% for typical data — within the specification's SC-006 requirement of within 5% of single-threaded gzip.
- True independence enables both parallel decompression (US2) and random access (US4) without any sequential dependency chain.
- Incompressible blocks (compressed size >= uncompressed size) are stored raw with a flag bit in the block header, satisfying the spec's edge-case requirement.

**Alternative considered and rejected**: Use pigz's preset-dictionary trick to improve ratio.
- Rejected because it creates a sequential decompression dependency: to decompress block N, you must first decompress block N-1 to obtain its last 32 KB. This is fatal to the parallel decompression design (US2).

**Rust implementation path**:
```rust
use flate2::{Compress, Compression, FlushCompress};

fn compress_block(input: &[u8], level: Compression) -> Vec<u8> {
    let mut c = Compress::new(level, false); // false = no zlib header = raw DEFLATE
    let mut out = Vec::with_capacity(input.len());
    c.compress_vec(input, &mut out, FlushCompress::Finish)
        .expect("compress_vec is infallible for in-memory buffers");
    out
}
```

Parallel execution via rayon:
```rust
use rayon::prelude::*;

let compressed_blocks: Vec<Vec<u8>> = input_chunks
    .par_iter()
    .map(|chunk| compress_block(chunk, compression_level))
    .collect();
```

---

### Decision 2: File Format Structure

#### Magic Bytes and Format Version

```
Magic:          [u8; 8]  = b"CRUSH\x01\x00\x00"
                            ^^^^^ ^^
                            name  format_version = 1 (u16 LE in bytes 5-6)
                            (bytes 7-8 reserved, must be 0x00 0x00)
```

Alternatively, keep magic and version as separate fields for clarity:

```
Magic:          [u8; 6]  = b"CRUSH\x00"   (null-terminated ASCII identifier)
Format version: u32 LE                    (bumped on breaking format changes)
```

**Decision**: Use a 4-byte magic followed by a separate u32 format version field. The 4-byte magic `CRSH` is compact, unique, and leaves no ambiguity:

```
File Header (fixed 64 bytes):
  [0..4]   magic:            [u8; 4]  = [0x43, 0x52, 0x53, 0x48]  ("CRSH")
  [4..8]   format_version:   u32 LE   = 1
  [8..16]  engine_version:   [u8; 8]  (SemVer packed: major u16, minor u16, patch u16, pre u8, build u8)
  [16..20] block_size:        u32 LE   (uncompressed block size in bytes)
  [20..21] compression_level: u8       (0–9, matching DEFLATE levels)
  [21..22] flags:             u8       (bit 0: checksum_enabled, bits 1-7: reserved)
  [22..24] reserved:          [u8; 2]  (must be 0x00)
  [24..32] uncompressed_size: u64 LE   (total uncompressed input bytes, u64::MAX if unknown/streaming)
  [32..40] block_count:       u64 LE   (total number of blocks, u64::MAX if unknown/streaming)
  [40..64] reserved:          [u8; 24] (future use, must be 0x00)
```

Note: `uncompressed_size` and `block_count` are written as `u64::MAX` during streaming compression and patched to their real values after all blocks are written. For streaming outputs where seeking back is impossible, they remain `u64::MAX` and the reader infers them from the block index.

#### Per-Block Layout

Each block is stored contiguously:

```
Block Header (16 bytes):
  [0..4]   compressed_size:   u32 LE   (bytes of compressed payload; 0 if stored raw)
  [4..8]   uncompressed_size: u32 LE   (bytes of original input for this block)
  [8..12]  checksum:          u32 LE   (xxHash32 of the uncompressed block data, or 0 if flags.checksum_enabled=0)
  [12..13] block_flags:       u8       (bit 0: stored=1 means block is uncompressed raw; bits 1-7: reserved)
  [13..16] reserved:          [u8; 3]  (must be 0x00)

Block Payload:
  [16 .. 16+compressed_size]  raw DEFLATE bytes (or raw uncompressed bytes if stored flag set)
```

#### Block Index (at end of file)

```
Index Entry (20 bytes each, one per block):
  [0..8]   block_offset:      u64 LE   (absolute byte offset of this block's Block Header from start of file)
  [8..12]  compressed_size:   u32 LE   (mirrors Block Header field; avoids seeking to read)
  [12..16] uncompressed_size: u32 LE   (mirrors Block Header field)
  [16..20] checksum:          u32 LE   (mirrors Block Header field)

Index Header (8 bytes, precedes index entries):
  [0..4]   index_entry_count: u32 LE
  [4..8]   index_flags:       u32 LE   (reserved, must be 0x00)
```

#### File Footer (fixed 24 bytes, always at end of file)

```
File Footer:
  [0..8]   index_offset:      u64 LE   (absolute byte offset of Index Header from start of file)
  [8..12]  index_size:        u32 LE   (byte length of the index region: 8 + 20 * block_count)
  [12..16] footer_checksum:   u32 LE   (xxHash32 of bytes [0..12] of this footer)
  [16..20] format_version:    u32 LE   (must match File Header format_version; redundant but enables footer-only parsing)
  [20..24] magic:             [u8; 4]  = [0x43, 0x52, 0x53, 0x48]  ("CRSH", same as file header)
```

The trailing magic and format version in the footer allow a reader to:
1. Open the file and seek to the last 24 bytes.
2. Verify magic and format version match expectations.
3. Read `index_offset` and `index_size` to locate and load the index.
4. Verify `footer_checksum` before trusting the index pointers.
5. Jump to any block in O(1) using `block_offset` from the index.

This mirrors the XZ `Backward_Size` and zstd `Seekable_Magic_Number` patterns.

#### Complete File Layout

```
┌────────────────────────────────────┐
│  File Header (64 bytes)            │  magic, format/engine version, config
├────────────────────────────────────┤
│  Block 0                           │
│    Block Header (16 bytes)         │  compressed_size, uncompressed_size, checksum, flags
│    Block Payload (variable)        │  raw DEFLATE or raw bytes
├────────────────────────────────────┤
│  Block 1                           │
│    Block Header (16 bytes)         │
│    Block Payload (variable)        │
├────────────────────────────────────┤
│  ...                               │
├────────────────────────────────────┤
│  Block N-1                         │
│    Block Header (16 bytes)         │
│    Block Payload (variable)        │
├────────────────────────────────────┤
│  Index Header (8 bytes)            │  entry_count, index_flags
│  Index Entry 0 (20 bytes)          │  block_offset, compressed_size, uncompressed_size, checksum
│  Index Entry 1 (20 bytes)          │
│  ...                               │
│  Index Entry N-1 (20 bytes)        │
├────────────────────────────────────┤
│  File Footer (24 bytes)            │  index_offset, index_size, footer_checksum, format_version, magic
└────────────────────────────────────┘
```

---

### Decision 3: Checksum Algorithm — xxHash32

**Decision**: Use **xxHash32** for per-block checksums.

**Rationale**:
- xxHash32 produces 4-byte checksums that fit into a single `u32` field without consuming excessive index space.
- At block sizes of 1 MB, hardware-accelerated CRC32 (SSE 4.2) runs at ~20 GB/s on modern x86_64, making it slightly faster than software xxHash. However, the `crc32fast` crate (an **allowed** dependency in the constitution) provides hardware-accelerated CRC32 without adding a new dependency.
- The constitution's allowed dependencies include `crc32fast` explicitly. xxHash would require adding a new dependency (e.g., `xxhash-rust`), which requires justification.

**Revised decision**: Use **CRC32 (IEEE)** via the `crc32fast` crate, which is an explicitly allowed constitution dependency. This avoids any dependency variance request.

- `crc32fast::hash(block_data)` returns a `u32`.
- Hardware-accelerated on x86_64 (SSE 4.2) and ARM (CRC32 extension).
- Sufficient collision resistance for data integrity at 1 MB block granularity.
- The `block_flags` field in the Block Header carries a `stored` bit, not the checksum type — checksum type is implicitly CRC32 for format version 1. A future format version could introduce xxHash64 (64-bit, 8-byte field) via the index_flags or a new format version number.

---

## Rationale and Alternatives Considered

### Why Not Place the Index at the Start of the File?

A front-loaded index requires knowing all block offsets and sizes before any block is written, which means either:
1. A two-pass write: compress all blocks to a temporary buffer, build the index, write header+index to the real output, then copy all blocks. This doubles I/O and requires temporary storage equal to the full compressed output size.
2. Pre-allocate space for a maximum-size index and seek back to fill it. This requires knowing the maximum number of blocks in advance and leaves a gap if the actual block count is smaller.

Neither approach works for streaming inputs (pipes, network sockets). The end-of-file index is strictly superior for a write-once, seek-at-read model.

### Why Not a Separate External Index File?

BGZF uses an external Tabix index (`.tbi`) stored as a separate file. This keeps the compressed file itself as a valid gzip file, which is BGZF's primary constraint. Crush has no compatibility constraint — a single self-contained file is simpler to move, copy, and manage. The index is embedded in the same file.

### Why Not Use Zstd Instead of DEFLATE?

The constitution explicitly lists `flate2` (DEFLATE) as the allowed compression dependency for Phase 1 and restricts the allowed core dependencies. Zstd is not in the allowed list and would require a constitution amendment. Additionally, the spec's success criteria (SC-006) require output within 5% of gzip for compressible data — DEFLATE (the same algorithm as gzip) satisfies this trivially; zstd would give better ratios but is architecturally out of scope for this phase.

### Why 1 MB Default Block Size Instead of pigz's 128 KB?

At 128 KB blocks with no shared dictionary, the cold-start penalty on typical data reaches 7–10%. At 1 MB, it falls to under 3%, comfortably within SC-006's 5% budget. The spec assumption A-004 already specifies 1 MB as the default. For random access (US4), the granularity means a single-block read covers at most 1 MB of decompressed data — acceptable for the spec's SC-004 requirement of under 100 ms for last-block access on files up to 10 GB.

### Why Not Store Absolute Offsets + Sizes (Redundancy in Index Entries)?

The zstd seekable format stores only `Compressed_Size` per entry, computing absolute offsets by cumulative summation. This saves 4 bytes per block entry but requires O(N) summation to find block N's offset.

Crush stores absolute `block_offset` (8 bytes) in each index entry. For a file with 10,000 blocks (10 GB at 1 MB/block), the index is 10,000 × 20 bytes = 200 KB, which is negligible. The benefit is O(1) random access: `seek(index_entry[N].block_offset)` with no summation loop. This directly satisfies SC-004 and US4.

---

## Sources

- [pigz: A parallel implementation of gzip](https://zlib.net/pigz/) — pigz home page, Mark Adler
- [PIGZ(1) General Commands Manual](https://zlib.net/pigz/pigz.pdf) — pigz man page PDF
- [Parallel Gzip - Pigz - Lei Mao's Log Book](https://leimao.github.io/blog/Parallel-Gzip-Pigz/) — technical walkthrough of pigz internals
- [BGZF - Wikipedia](https://en.wikipedia.org/wiki/BGZF) — BGZF format overview
- [Bio.bgzf module — Biopython documentation](https://biopython.org/docs/1.80/api/Bio.bgzf.html) — BGZF technical detail
- [bgzip(1) manual page](http://www.htslib.org/doc/bgzip.html) — htslib BGZF implementation
- [zstd seekable format specification](https://github.com/facebook/zstd/blob/dev/contrib/seekable_format/zstd_seekable_compression_format.md) — Facebook zstd repo
- [zeekstd: Rust implementation of the Zstandard Seekable Format](https://github.com/rorosen/zeekstd) — Rust reference implementation
- [xz-file-format.txt](https://tukaani.org/xz/xz-file-format.txt) — XZ format specification
- [MiGz for Compression and Decompression](https://engineering.linkedin.com/blog/2019/02/migz-for-compression-and-decompression) — LinkedIn Engineering blog
- [dictzip(1) - Linux man page](https://linux.die.net/man/1/dictzip) — dictzip format reference
- [DeflateEncoder in flate2::write - Rust](https://docs.rs/flate2/latest/flate2/write/struct.DeflateEncoder.html) — flate2 API documentation
- [Compress in flate2 - Rust](https://docs.rs/flate2/latest/flate2/struct.Compress.html) — flate2 low-level API
- [GitHub - rust-lang/flate2-rs](https://github.com/rust-lang/flate2-rs) — flate2 source and docs
- [GitHub - srijs/rust-crc32fast](https://github.com/srijs/rust-crc32fast) — crc32fast Rust crate
- [Use Fast Data Algorithms - Joey Lynch](https://jolynch.github.io/posts/use_fast_data_algorithms/) — xxHash vs CRC32 performance analysis
- [Parallel decompression of gzip-compressed files (arxiv)](https://arxiv.org/pdf/1905.07224) — research paper on parallel gzip decompression
- [Show HN: Rapidgzip](https://news.ycombinator.com/item?id=37378411) — rapidgzip parallel gzip decompressor
- [GitHub - klauspost/pgzip](https://github.com/klauspost/pgzip) — Go parallel gzip, block size benchmarks
- [Data Parallelism with Rust and Rayon | Shuttle](https://www.shuttle.dev/blog/2024/04/11/using-rayon-rust) — rayon parallel iterator patterns

---

## Topic 3: GPU Compute Framework

### Decision
**`wgpu`** (v28+) with WGSL compute shaders, exposed as an optional `gpu` Cargo feature in `crush-core`.

**Synchronous dispatch pattern**: `device.poll(PollType::Wait)` blocks the calling thread until GPU work completes. Adapter/device initialisation is wrapped with `pollster::block_on` (zero-dependency, no async runtime). No `tokio`/`async-std` enters `crush-core`.

### Rationale
- Only framework with first-class support on all three platforms: Linux (Vulkan), macOS (Metal), Windows (D3D12/Vulkan)
- 12.8M crates.io downloads, used by Firefox and Servo — most battle-tested Rust GPU compute crate
- Microsoft's **GDeflate** HLSL compute shaders (Apache 2.0) are a ready-to-port GPU-parallel DEFLATE implementation; porting HLSL → WGSL is mechanical
- `block_compression` crate (v0.6.0, 2025) proves byte-level transformation via wgpu compute works correctly

### Alternatives Considered
| Framework | Reason Rejected |
|---|---|
| CUDA (`cust`/`cudarc`) | NVIDIA-only; no macOS/Apple Silicon; no stable Rust bindings for nvCOMP |
| OpenCL (`opencl3`) | macOS deprecated at OpenCL 1.2; Apple actively removing it |
| Vulkano | Vulkan-only; MoltenVK on macOS requires external install; more boilerplate than wgpu |

### Implementation Path
1. `crush-core/Cargo.toml`: add `wgpu` + `pollster` behind `[features] gpu = [...]`
2. Port GDeflate HLSL to WGSL (or author a new block-parallel DEFLATE compute shader)
3. Wrap init + dispatch in synchronous `GpuWorker` struct; expose same `compress_block` interface as CPU path
4. Feature-gate the entire GPU module — when disabled, zero GPU symbols appear in the binary

---

## Topic 4: Progress Callbacks & Cancellation API

### Decision: Progress Callback Type
`Arc<Mutex<Box<dyn FnMut(ProgressEvent) -> bool + Send>>>` as an optional field in `EngineConfiguration`.

**Why `Arc<Mutex<FnMut>>`**: Rayon requires closures passed to parallel combinators to be `Fn` (shared reference). `Arc<Mutex<FnMut>>` satisfies this via interior mutability. `Mutex` contention at 1 MB block granularity (~500 events/second at 500 MB/s) is negligible — lock hold time is microseconds, block compression takes milliseconds.

The callback serves dual purpose: progress reporting and cancellation signal (returning `false` = abort). This matches FR-012 exactly.

### Decision: Cooperative Cancellation
`Arc<AtomicCancellationToken>` (reuse `crush-core/src/cancel.rs`) + `rayon::try_for_each` with `ControlFlow::Break`.

- Callback returning `false` stores `true` into the `AtomicBool`
- Each rayon worker checks the flag at the start of each block (before doing work)
- `try_for_each` propagates `ControlFlow::Break` — workers already in-flight complete their current block cleanly
- This satisfies FR-012: "halt at next block boundary, discard partial output"
- `Release`/`Acquire` ordering on the atomic ensures correct memory visibility

### Decision: Cancelled Result Type
`Result<T, CrushError>` with existing `CrushError::Cancelled` variant. Add `CrushError::is_cancelled() -> bool` helper.

**Why not a three-way enum**: Incompatible with `?` operator; breaks all existing call sites; no precedent in Rust standard library or major crates.

### CLI Reference Implementation (FR-013)
```rust
// crush-cli — indicatif belongs here, not in crush-core
let pb = ProgressBar::new(total_bytes);
let cb: ProgressCallback = Box::new(move |event: ProgressEvent| {
    pb.set_position(event.bytes_processed);
    !cancel_token.is_cancelled()  // Ctrl+C wires into AtomicCancellationToken
});
let config = EngineConfiguration {
    progress: Some(Arc::new(Mutex::new(cb))),
    ..Default::default()
};
```

### Alternatives Considered
| Pattern | Reason Not Chosen |
|---|---|
| `mpsc::Sender` channel | Splits progress (out) and cancel (in); poor fit for single-callback FR-012 |
| `Arc<AtomicU64>` polling | Cannot express cancellation-via-callback; requires separate render thread |
| Three-way `CompressResult<T>` | Incompatible with `?`; not idiomatic |

---

## Topic 5: New Crate Structure

### Decision
Add **`crush-parallel`** as a new workspace member.

### Rationale
- Distinct plugin implementing `CompressionAlgorithm` trait from `crush-core`
- GPU feature flag (`crush-parallel/gpu`) is entirely opt-in; does not pollute `crush-core`'s defaults
- Consistent with existing plugin architecture pattern

### Source Structure
```
crush-parallel/
├── Cargo.toml
├── src/
│   ├── lib.rs              # Public API, re-exports
│   ├── engine.rs           # ParallelEngine, compress/decompress entry points
│   ├── block.rs            # Block splitting, per-block compression, checksum
│   ├── format.rs           # FileHeader, BlockHeader, BlockIndex, IndexFooter (de)serialization
│   ├── index.rs            # BlockIndex random access, seek logic
│   ├── config.rs           # EngineConfiguration, ProgressEvent, ProgressCallback
│   └── gpu/
│       ├── mod.rs          # GpuWorker, feature-gated
│       ├── worker.rs       # wgpu device init, compute dispatch, sync readback
│       └── shaders/
│           └── deflate.wgsl
├── benches/
│   ├── throughput.rs       # Throughput vs thread count, block size sweep
│   └── random_access.rs    # Seek + single block decompress latency
└── fuzz/
    ├── Cargo.toml
    └── fuzz_targets/
        ├── fuzz_decompress.rs   # Arbitrary bytes → decompress → must not panic
        └── fuzz_roundtrip.rs    # Random data → compress → decompress → verify identical
```

Additional sources:
- [wgpu - crates.io](https://crates.io/crates/wgpu)
- [Microsoft GDeflate HLSL shader](https://github.com/microsoft/DirectStorage/blob/main/GDeflate/shaders/GDeflate.hlsl) — Apache 2.0
- [block_compression crate](https://lib.rs/crates/block_compression) — wgpu compute for byte manipulation
- [rayon try_for_each](https://docs.rs/rayon/latest/rayon/iter/trait.ParallelIterator.html#method.try_for_each)
- [ControlFlow in std::ops](https://doc.rust-lang.org/std/ops/enum.ControlFlow.html)
- [indicatif ParallelProgressIterator](https://docs.rs/indicatif/latest/indicatif/trait.ParallelProgressIterator.html)
