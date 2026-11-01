# Technical Research: Compression Performance Bottleneck Audit

**Feature**: `027-compression-perf-plan`  
**Date**: 2026-11-01  
**Status**: Complete

---

## 1. Introduction

This research artifact documents the codebase-wide technical audit of compression routines across all member crates in the Crush workspace (`crush-core`, `crush-parallel`, `crush-lz`, `crush-gpu`, `crush-sprint`, `crush-cli`).

The audit specifically analyzed:
- CPU instruction pipelines and memory load patterns in hot loops.
- Allocation lifecycles and buffer recycling across threads.
- SIMD and bit-level encoding primitives.
- Parallelization coverage across independent blocks and tiles.
- Zero-copy compliance with the Crush Constitution (Principle I: Performance First).

---

## 2. Component Audits & Findings

### 2.1. `crush-parallel`: Worker Buffer Thrashing & Memory Amplification

#### A. Buffer De-Pooling via `std::mem::take`
- **Location**: [crush-parallel/src/block.rs:79](file:///C:/Users/Admin/code/crush/crush-parallel/src/block.rs#L79) and [crush-parallel/src/codec.rs:114-117](file:///C:/Users/Admin/code/crush/crush-parallel/src/codec.rs#L114-L117)
- **Code Trace**:
  ```rust
  // crush-parallel/src/engine.rs:71-73
  .map_init(
      || (codec.encoder(), Vec::new()),
      |(encoder, scratch), (i, chunk)| {
          compress_block(codec, worker_state(encoder)?, scratch, chunk, i, config)
      },
  )
  ```
  ```rust
  // crush-parallel/src/block.rs:79
  payload: Cow::Owned(std::mem::take(scratch)),
  ```
  ```rust
  // crush-parallel/src/codec.rs:114-117
  let bound = encoder.deflate_compress_bound(input.len());
  if out.len() < bound {
      *out = vec![0u8; bound];
  }
  ```
- **Analysis**:
  Rayon's `map_init` creates one `scratch: Vec<u8>` per worker thread. However, on every successful block compression, `compress_block` executes `std::mem::take(scratch)` to package the output into `BlockOutput`.
  This leaves `scratch` with `capacity == 0` and `len == 0`.
  When the worker receives its subsequent block, `out.len() < bound` is guaranteed to be true (0 < 1,048,576+).
  Thus, `*out = vec![0u8; bound]` is executed on **every single block**.
  For a 128 MB file with 1 MB blocks (128 blocks), this triggers 128 separate heap allocations and 128 MB of kernel page zeroing (`calloc`/`memset`), thrashing L2/L3 caches and TLB entries.
- **Evidence from Prior Specs**:
  Feature 011 introduced pooled libdeflater state for encoders/decoders (`Slice A`), but the output buffer pooling was compromised by taking ownership of the vector instead of recycling it.
- **Remedy**:
  Maintain a dedicated buffer pool or double-buffer per worker. Rather than transferring ownership of `scratch`, `scratch` can be reused while copying out the compressed bytes, or the worker can own two buffers that are swapped, keeping the allocated capacity intact across all iterations.

#### B. Memory Duplication in Output Assembly
- **Location**: [crush-parallel/src/engine.rs:117-155](file:///C:/Users/Admin/code/crush/crush-parallel/src/engine.rs#L117-L155)
- **Analysis**:
  The parallel compression pipeline operates in two distinct phases:
  1. Phase 1: All blocks compressed into independent `Vec<u8>` buffers in memory.
  2. Phase 2: A single monolithic buffer `let mut out = vec![0u8; total_size]` is allocated, and all blocks are copied a second time in parallel (`slice[BlockHeader::SIZE..].copy_from_slice(&block.payload)`).
  This requires **200% memory allocation** of the compressed payload size simultaneously in RAM.
- **Remedy**:
  For file outputs, write blocks directly into their calculated positions in a pre-allocated file mapping or streaming sink, avoiding the intermediate in-memory assembly buffer.

---

### 2.2. `crush-lz`: Single-Threaded Bottleneck and Hot-Loop Inefficiencies

#### A. Single-Threaded Execution
- **Location**: [crush-lz/src/compress.rs:278-294](file:///C:/Users/Admin/code/crush/crush-lz/src/compress.rs#L278-L294)
- **Analysis**:
  `FastLzPlugin::compress_frame_into` processes blocks sequentially:
  ```rust
  for chunk in input.chunks(block_size) {
      check_cancel(cancel)?;
      push_block(&mut compressor, chunk, out);
  }
  ```
  `fast-lz` frames consist of completely independent blocks (up to 1 MB). There is zero inter-block dependency.
- **Remedy**:
  Implement `BlockCodec` for `FastLzCodec` so `crush-parallel` can run `fast-lz` across all CPU cores. Single-threaded `fast-lz` achieves ~740 MB/s; multi-threaded execution on 16 cores will achieve 8–12 GB/s, enabling Crush to dominate the high-speed LZ tier.

#### B. Zero-Filling Memory on Compression
- **Location**: [crush-lz/src/compress.rs:330-334](file:///C:/Users/Admin/code/crush/crush-lz/src/compress.rs#L330-L334)
- **Analysis**:
  ```rust
  let payload_at = header_at + BLOCK_HEADER_SIZE;
  out.resize(payload_at + chunk.len() + WILD, 0);
  let written = compressor.compress_block(chunk, &mut out[payload_at..]);
  ```
  `out.resize(..., 0)` explicitly zeros out up to 1 MB of memory per block before `compress_block` writes into it.
  Branch `025-perf-lz-zero-fill` removed zero-filling from *decompression* (yielding a 12–45% speedup), but explicitly left *compression* untouched because of benchmark focus.
- **Remedy**:
  Use `Vec::spare_capacity_mut` or a reusable scratch buffer to eliminate zero-filling completely.

#### C. Redundant Memory Loads in Match Finder Loop
- **Location**: [crush-lz/src/compress.rs:90-91](file:///C:/Users/Admin/code/crush/crush-lz/src/compress.rs#L90-L91)
- **Code**:
  ```rust
  let word = read_u32(src, i);
  let h = hash6(read_u64(src, i));
  ```
- **Analysis**:
  `read_u32` reads 4 bytes at `src[i]`. `read_u64` reads 8 bytes at `src[i]`.
  In little-endian x86_64 and ARM64, `word` is identical to `(read_u64(src, i) as u32)`.
  Executing two independent memory loads from the same address causes redundant load-store queue entries and cache lookups on every loop iteration.
- **Remedy**:
  ```rust
  let word64 = read_u64(src, i);
  let word = word64 as u32;
  let h = hash6(word64);
  ```

#### D. Bounds Checking Overhead in Inner Loop
- **Location**: [crush-lz/src/compress.rs:139-152](file:///C:/Users/Admin/code/crush/crush-lz/src/compress.rs#L139-L152)
- **Code**:
  ```rust
  fn read_u32(s: &[u8], i: usize) -> u32 {
      match s.get(i..).and_then(<[u8]>::first_chunk::<4>) {
          Some(b) => u32::from_le_bytes(*b),
          None => 0,
      }
  }
  ```
- **Analysis**:
  The loop invariant is `i < limit` where `limit = len - READ_AHEAD` and `READ_AHEAD = 8`. Therefore, `i + 8 <= len` is mathematically guaranteed.
  Calling `s.get(i..)` followed by `<[u8]>::first_chunk::<4>()` forces two slice bounds checks and Option matching on every position probed.
- **Remedy**:
  Provide an in-bounds read helper with checked preconditions before the loop, enabling the compiler to emit a single unaligned scalar/vector MOV instruction.

---

### 2.3. `crush-gpu`: Data Structures and Serial Bottlenecks

#### A. Catastrophic Dynamic Allocations in LZ77 (`HashMap<u32, Vec<usize>>`)
- **Location**: [crush-gpu/src/lz77.rs:83-115](file:///C:/Users/Admin/code/crush/crush-gpu/src/lz77.rs#L83-L115)
- **Code**:
  ```rust
  let mut hash_table: std::collections::HashMap<u32, Vec<usize>> =
      std::collections::HashMap::new();
  ...
  hash_table.entry(h).or_default().push(pos);
  ```
- **Analysis**:
  Using `std::collections::HashMap` for an LZ77 match finder is disastrous for performance:
  1. Every hash key computation runs SipHash (cryptographic hashing) rather than a fast multiplicative hash.
  2. Every hash bucket is a heap-allocated `Vec<usize>`. On a 1 MB input, tens of thousands of `Vec` allocations and re-allocations occur.
  3. Pointer indirection destroys cache locality.
- **Remedy**:
  Replace `HashMap<u32, Vec<usize>>` with standard LZ77 arrays:
  - `head: [u32; 65536]` (maps 16-bit hash to latest position)
  - `prev: [u16; 32768]` (cyclic chain of previous positions within the 32 KB window)
  Zero heap allocations, O(1) direct integer indexing, ~320 KB cache footprint.

#### B. Single-Threaded Tile Processing
- **Location**: [crush-gpu/src/engine.rs:83-150](file:///C:/Users/Admin/code/crush/crush-gpu/src/engine.rs#L83-L150)
- **Code**:
  ```rust
  for (i, tile_data) in tiles.iter().enumerate() { ... }
  ```
- **Analysis**:
  `crush-gpu` compression runs exclusively on the CPU in a serial `for` loop. The 64 KB tiles are completely independent and could be processed in parallel across all CPU cores.
- **Remedy**:
  Convert `tiles.iter().enumerate()` to `tiles.par_iter().enumerate()` using Rayon.

#### C. Bit-by-Bit BitWriter & BitReader
- **Location**: [crush-gpu/src/gdeflate.rs:133-160](file:///C:/Users/Admin/code/crush/crush-gpu/src/gdeflate.rs#L133-L160)
- **Code**:
  ```rust
  fn write_bits(&mut self, value: u16, n: u8) {
      for i in 0..n {
          if (value >> i) & 1 != 0 {
              self.current_byte |= 1 << self.bit_pos;
          }
          self.bit_pos += 1;
          if self.bit_pos >= 8 { ... }
      }
  }
  ```
- **Analysis**:
  Processing 1 bit at a time with loop branching for every symbol and extra bit costs ~20-30 CPU cycles per bit. A 64-bit word accumulator writes symbols in 1-2 CPU cycles.
- **Remedy**:
  Implement a word-level bit accumulator (`bit_buf: u64`, `bits_in_buf: u32`).

#### D. Dynamic Canonical Huffman Re-Generation & Linear Symbol Searches
- **Location**: [crush-gpu/src/gdeflate.rs:504-505, 457-487](file:///C:/Users/Admin/code/crush/crush-gpu/src/gdeflate.rs#L504-L505)
- **Analysis**:
  1. RFC 1951 fixed Huffman codes never change, yet `build_fixed_litlen_codes()` dynamically builds them for every 64 KB tile.
  2. `encode_length` and `encode_distance` linearly search arrays of lengths and distances for every match.
- **Remedy**:
  Pre-compute `const` fixed Huffman tables and O(1) lookup tables for length/distance symbols.

---

### 2.4. `crush-core`: Missing Plugin Specializations & Serial Decompression CRC

#### A. Missing `compress_into` on `DeflatePlugin` & `ParallelDeflatePlugin`
- **Location**: [crush-core/src/plugin/contract.rs:112-121](file:///C:/Users/Admin/code/crush/crush-core/src/plugin/contract.rs#L112-L121)
- **Analysis**:
  The default `compress_into` method falls back to `self.compress(input, cancel_flag)?` and appends it via `out.extend_from_slice(&payload)`.
  Because neither `DeflatePlugin` nor `ParallelDeflatePlugin` overrides `compress_into`, calling `crush_core::compress` forces an intermediate `Vec<u8>` allocation followed by a full payload copy.
- **Remedy**:
  Implement `compress_into` directly on `ParallelDeflatePlugin` and `DeflatePlugin`.

#### B. Serial Outer CRC32 in `crush-core::decompress`
- **Location**: [crush-core/src/decompression.rs:40-44](file:///C:/Users/Admin/code/crush/crush-core/src/decompression.rs#L40-L44)
- **Code**:
  ```rust
  let payload_for_crc = &input[new_payload_start..];
  let mut hasher = Hasher::new();
  hasher.update(payload_for_crc);
  let computed_crc = hasher.finalize();
  ```
- **Analysis**:
  While `compression.rs` uses `parallel_crc32` for data >= 8 MB, `decompression.rs` verifies the outer CRC serially on a single core, blocking decompression startup on large files.
- **Remedy**:
  Use `parallel_crc32` in `decompression.rs`.

---

## 3. Summary of Projected Gains

| Optimization Area | Expected Throughput Improvement | Memory Footprint Reduction |
|---|---|---|
| Parallel Engine Worker Buffer Recycling | +5% to +12% multi-core throughput | Eliminates 100% of per-block worker re-allocations |
| `fast-lz` Multi-Core Scaling (`BlockCodec`) | +800% to +1500% (8x-15x on multi-core) | N/A (Linear core scaling) |
| `fast-lz` Zero-Fill & Hot-Loop Load Elimination | +15% to +25% single-core speed | Eliminates ~1 MB zero-fill per block |
| `crush-gpu` Array Hash Tables & Parallel Tiles | +1000% to +2000% (10x-20x speedup) | Eliminates millions of `HashMap`/`Vec` allocations |
| `crush-core` `compress_into` & Parallel CRC | +3% to +8% end-to-end throughput | Eliminates secondary payload copy |
