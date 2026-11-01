# Feature Specification: Comprehensive Compression Performance Optimization Plan

**Feature Branch**: `027-compression-perf-plan`  
**Created**: 2026-11-01  
**Status**: Draft  
**Input**: Repository-wide review of compression functions to identify performance bottlenecks and formulate a systematic, benchmark-driven optimization plan.

---

## 1. Executive Summary & Objective

Crush's primary constitutional mandate is **"Performance First (NON-NEGOTIABLE)"**:
> *"Crush exists to provide high-performance compression. Performance is not a feature - it is the product's fundamental value proposition."*

A thorough review of the compression functions across `crush-core`, `crush-parallel`, `crush-lz`, `crush-gpu`, and `crush-sprint` reveals major performance optimization opportunities spanning:
1. **Memory Allocation & Zero-Fill Overhead**: Buffer de-pooling, redundant zero-initialization, and multi-pass payload copying across engine boundaries.
2. **Algorithmic Inefficiencies & Data Structures**: Inefficient container choices (`HashMap<u32, Vec<usize>>` in LZ77), bit-by-bit I/O loops, linear searches in symbol encoders, and redundant memory loads in inner match-finding loops.
3. **Parallel Dispatch & Threading Gaps**: Strictly single-threaded execution in `crush-lz` and `crush-gpu` CPU compression, and missing `compress_into` / `compress_to` overrides in plugins causing serial materialization and secondary copying.
4. **I/O & File Pipeline**: Absence of direct memory-mapped file compression (reading files into RAM, compressing into RAM, then writing to disk sequentially).

This specification and its associated plan define the work packages, acceptance criteria, and quantitative performance targets to systematically eliminate these bottlenecks.

---

## 2. User Scenarios & Performance Stories

### User Story 1 - Parallel Engine Zero-Allocation Worker Buffering (Priority: P1)
As a user compressing large files (>25 MB) with `parallel-deflate`, I want Rayon worker threads to strictly reuse pre-allocated scratch buffers without per-block heap allocation or page zero-filling, so that memory bandwidth is conserved and compression throughput scales linearly with CPU cores.

- **Acceptance Criteria**:
  1. Rayon workers in `crush-parallel` maintain a persistent scratch buffer across blocks without dropping or replacing it on every block (`mem::take` replaced with reusable buffer ownership/swap).
  2. Heap allocations per worker thread drop from O(N_blocks) to O(1) after initial warm-up.
  3. Single-core and multi-core `parallel-deflate` throughput improves by 5–12% on the 128 MB benchmark corpus.

### User Story 2 - High-Throughput Multi-Core `fast-lz` Engine (Priority: P1)
As a user selecting `--level fast` or `--plugin fast-lz`, I want `fast-lz` to execute across all available CPU cores via `crush-parallel`'s `BlockCodec` abstraction and eliminate hot-loop memory redundancies, so that lightweight compression achieves ultra-high throughput (>8 GB/s on modern multi-core CPUs).

- **Acceptance Criteria**:
  1. `FastLzCodec` implements `BlockCodec`, enabling multi-threaded block compression through `crush-parallel`.
  2. Elimination of `out.resize(..., 0)` zero-fill in `push_block`.
  3. Inner match-finder loads 64-bit words once (`read_u64`), deriving both match word and hash without duplicate memory loads.
  4. Redundant bounds checking in hot match-finder loop eliminated within verified slice limits.
  5. Single-core `fast-lz` throughput increases by >=10%, and multi-core throughput scales with core count (target >= 8 GB/s on 16-core reference hardware).

### User Story 3 - Algorithmic Modernization of `crush-gpu` Compression (Priority: P2)
As a user running tile-based compression or preparing archives for GPU decompression, I want `crush-gpu` compression to run in parallel across CPU cores, replace dynamic hash maps with flat zero-allocation sliding-window arrays, and replace bit-by-bit writing with a 64-bit accumulator.

- **Acceptance Criteria**:
  1. `crush-gpu` tile compression dispatches independent 64 KB tiles across Rayon workers.
  2. `crush-gpu/src/lz77.rs` replaces `HashMap<u32, Vec<usize>>` with flat array hash tables `[u32; 65536]` and `[u16; 32768]`, eliminating all heap allocations in the match search loop.
  3. `BitWriter` and `BitReader` in `crush-gpu/src/gdeflate.rs` utilize a 64-bit word accumulator, eliminating bit-by-bit loops.
  4. RFC 1951 fixed Huffman tables pre-computed as `const` arrays, replacing per-tile dynamic table building.
  5. `encode_length` and `encode_distance` replace linear searches with O(1) direct table lookups.
  6. Overall `crush-gpu` compression speed improves by at least 10x-20x.

### User Story 4 - End-to-End Zero-Copy Plugin Pipeline (`crush-core`) (Priority: P2)
As a library or CLI consumer, I want all compression plugins to implement `compress_into` and streaming writer interfaces, so that compressed data is assembled directly into output buffers or destination files without redundant memory allocations and copies.

- **Acceptance Criteria**:
  1. `ParallelDeflatePlugin` and `DeflatePlugin` implement `compress_into`, avoiding intermediate payload copies.
  2. `read_crc32_block` in `crush-core/src/decompression.rs` uses multi-core `parallel_crc32` for payloads >= 8 MB, eliminating serial decompression stalls.
  3. CLI file compression avoids buffering complete compressed archives in RAM when writing to destination files.

---

## 3. Quantitative Success Criteria

| ID | Metric | Current Baseline | Target | Evaluation Command |
|---|---|---|---|---|
| **SC-001** | `parallel-deflate` 128MB compress (16c/24t) | ~1,048 MB/s | **>= 1,250 MB/s** (+20%) | `python .claude/skills/perf/bench.py ab --suite cpu-compress` |
| **SC-002** | `parallel-deflate` worker allocations | 1 alloc / 1MB block | **0 allocs** (pooled) | Profiler / Allocation tracker |
| **SC-003** | `fast-lz` single-core compress (128MB) | ~743 MB/s | **>= 900 MB/s** (+21%) | `cargo bench -p crush-lz --bench codec -- compress` |
| **SC-004** | `fast-lz` parallel compress (16c/24t) | N/A (1-core only: 743 MB/s) | **>= 6,000 MB/s** (8x+) | New benchmark suite |
| **SC-005** | `crush-gpu` CPU compression (10MB) | < 25 MB/s | **>= 250 MB/s** (10x+) | `cargo bench -p crush-gpu --bench throughput -- compress` |
| **SC-006** | Zero-fill in `crush-lz` compression | 100% of payload zeroed | **0 bytes** zero-filled | Source audit & callgrind |
| **SC-007** | Compression correctness | 100% roundtrip byte match | 100% byte match, zero regr. | `cargo test --workspace` + fuzzing |
