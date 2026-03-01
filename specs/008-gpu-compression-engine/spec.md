# Feature Specification: GPU Compression Engine

**Feature Branch**: `008-gpu-compression-engine`
**Created**: 2026-02-23
**Status**: Draft
**Input**: User description: "Create new crate for a GPU powered parallel compression engine similar to Microsoft GDeflate but with a pure Rust implementation. File format and data structures optimized for GPU. Direct integration with Nvidia developer frameworks with fallback for AMD and Apple chips. Plugin activates when file is over 100MB, GPU is present, and data would benefit from GPU processing. Research string vectorization, only use if it results in smaller files."

## User Scenarios & Testing

### User Story 1 - GPU-Accelerated Compression of Large Files (Priority: P1)

A user compresses a file larger than 100MB on a system with a compatible GPU. The compression engine automatically detects the GPU, evaluates that the data is suitable for GPU processing, and compresses the file using GPU-parallel tile-based blocks. The resulting file is a valid Crush archive that can be decompressed on any system — with or without a GPU.

**Why this priority**: This is the core value proposition. Without GPU-accelerated compression and decompression working end-to-end on at least one GPU vendor, no other story delivers value.

**Independent Test**: Can be fully tested by compressing a 200MB file on a system with a GPU, verifying the output is a valid archive, and decompressing it back to an identical copy. Delivers the primary performance benefit.

**Acceptance Scenarios**:

1. **Given** a 200MB binary file and a system with an NVIDIA GPU, **When** the user compresses the file via the crush GPU plugin, **Then** the file is compressed using GPU-parallel tile processing and produces a valid Crush archive.
2. **Given** a valid GPU-compressed Crush archive, **When** the user decompresses it on any system (with or without GPU), **Then** the output is byte-for-byte identical to the original file.
3. **Given** a 200MB file and a system with a GPU, **When** the user compresses the file, **Then** compression throughput exceeds 2 GB/s on an NVIDIA GPU with 2048+ CUDA cores.
4. **Given** a 200MB file and a system with a GPU, **When** the user compresses the file, **Then** the compression ratio is within 5% of standard DEFLATE for the same data.

---

### User Story 2 - Cross-Platform GPU Support with Vendor Fallback (Priority: P2)

A user on an AMD or Apple Silicon system compresses a large file. The engine detects the available GPU vendor and uses the appropriate compute backend (Vulkan for AMD, Metal for Apple, CUDA-optimized path for NVIDIA). If no supported GPU is detected, the operation falls back gracefully to the existing CPU-based parallel compression plugin.

**Why this priority**: Cross-platform GPU support dramatically expands the user base beyond NVIDIA-only systems. Without this, the plugin is limited to a fraction of users.

**Independent Test**: Can be tested by running compression on systems with different GPU vendors (or emulators) and verifying correct backend selection and successful output.

**Acceptance Scenarios**:

1. **Given** a system with an AMD GPU supporting Vulkan compute, **When** the user compresses a large file, **Then** the engine uses the Vulkan compute backend and produces a valid archive.
2. **Given** a macOS system with Apple Silicon, **When** the user compresses a large file, **Then** the engine uses the Metal compute backend and produces a valid archive.
3. **Given** a system with an NVIDIA GPU, **When** the user compresses a large file, **Then** the engine uses the CUDA-optimized path for maximum performance.
4. **Given** a system with no supported GPU, **When** the user compresses a large file, **Then** the operation falls back to CPU-based parallel compression transparently, with an informational message.
5. **Given** archives produced by different GPU backends, **When** any of them is decompressed, **Then** the output is identical regardless of which backend compressed it.

---

### User Story 3 - Automatic GPU Eligibility Detection (Priority: P3)

When a user compresses a file through the crush plugin system, the GPU plugin automatically evaluates three eligibility criteria: (1) the file exceeds 100MB, (2) a compatible GPU is present, and (3) the file's data characteristics would benefit from GPU processing. Only when all three conditions are met does the GPU plugin claim the compression task. Otherwise, the file is handled by a more suitable plugin.

**Why this priority**: Automatic selection ensures users get the best compression strategy without manual configuration. However, the GPU engine must work correctly (P1) and across platforms (P2) before intelligent routing adds value.

**Independent Test**: Can be tested by presenting the plugin selector with files of varying sizes and types and verifying the GPU plugin only claims work when all three eligibility criteria are met.

**Acceptance Scenarios**:

1. **Given** a 50MB file on a system with a GPU, **When** the plugin selector evaluates candidates, **Then** the GPU plugin does NOT claim the file (below 100MB threshold).
2. **Given** a 200MB file on a system without a GPU, **When** the plugin selector evaluates candidates, **Then** the GPU plugin does NOT claim the file (no GPU available).
3. **Given** a 200MB file with highly random/encrypted data on a system with a GPU, **When** the plugin selector evaluates candidates, **Then** the GPU plugin does NOT claim the file (data not suitable for GPU advantage).
4. **Given** a 200MB file with compressible data on a system with a GPU, **When** the plugin selector evaluates candidates, **Then** the GPU plugin claims the file and compresses it.
5. **Given** a 200MB file with compressible data, **When** the GPU plugin claims and compresses it, **Then** the resulting throughput is measurably higher than what the CPU parallel plugin would achieve for the same file.

---

### User Story 4 - GPU-Optimized Tile-Based File Format (Priority: P4)

The GPU compression engine produces archives using a tile-based binary format where each tile decompresses to a fixed-size block (64KB), tiles are independent and randomly accessible, and memory layout is aligned for GPU-friendly access patterns. This format enables massively parallel decompression where each GPU thread group processes a tile independently.

**Why this priority**: The format is essential infrastructure for P1, but is a separate user story because its design (tile independence, alignment, random access) provides value beyond basic compression — enabling partial decompression and streaming use cases.

**Independent Test**: Can be tested by compressing a file, then decompressing only specific tiles by index, verifying each tile decompresses correctly in isolation.

**Acceptance Scenarios**:

1. **Given** a compressed archive, **When** the user requests decompression of tile N, **Then** only tile N is read and decompressed without reading other tiles.
2. **Given** a compressed archive, **When** the format is inspected, **Then** all tile boundaries are aligned to 128-byte boundaries for GPU memory coalescing.
3. **Given** a compressed archive with a tile index, **When** any tile offset is looked up, **Then** the lookup completes in constant time (O(1)) via the index.
4. **Given** a file compressed with the GPU plugin, **When** the archive header is read, **Then** it contains a valid Crush magic number, original file size, tile count, and tile index offset.

---

### User Story 5 - Vectorized Pattern Matching for Improved Compression (Priority: P5)

During compression, the engine optionally applies vectorized (SIMD/GPU-parallel) string matching to find longer and more effective LZ dictionary matches across the input data. This feature activates only when analysis of the input data indicates it would produce a smaller compressed output than standard matching. If vectorized matching would not improve the compression ratio, it is skipped entirely to avoid unnecessary overhead.

**Why this priority**: This is an optimization on top of a working GPU compression engine. It adds value only after the core engine, cross-platform support, and format are solid. It is conditional by design — only used when it demonstrably shrinks files.

**Independent Test**: Can be tested by compressing a corpus of files with and without vectorized matching, comparing output sizes, and verifying the feature only activates when it reduces size.

**Acceptance Scenarios**:

1. **Given** a 200MB file with repetitive string patterns (e.g., log files, CSV data), **When** compressed with vectorized matching enabled, **Then** the output is at least 3% smaller than compression without vectorized matching.
2. **Given** a 200MB file with low string redundancy (e.g., binary/media data), **When** the engine evaluates vectorized matching, **Then** it skips vectorized matching and uses standard matching instead.
3. **Given** any file, **When** compressed with vectorized matching considered, **Then** the output is never larger than it would be without vectorized matching.
4. **Given** a file where vectorized matching activates, **When** decompressed, **Then** the output is byte-for-byte identical to the original (lossless guarantee preserved).

---

### Edge Cases

- What happens when the GPU runs out of memory during compression of a very large file? The engine must segment the file into manageable batches, processing each batch on the GPU sequentially without exceeding GPU memory limits.
- What happens when the GPU driver crashes or becomes unresponsive during compression? The engine must detect the failure, report it to the user, and fall back to CPU-based compression for the remaining data.
- What happens when a file is exactly 100MB? The GPU plugin does NOT claim it — the threshold is "over 100MB" (strictly greater than).
- What happens when the system has multiple GPUs? The engine selects the most capable GPU based on compute capability and available memory. It does not split work across multiple GPUs in the initial implementation.
- What happens when compression is cancelled mid-operation? The engine must respect the existing crush cancellation token, release GPU resources, and leave no partial output.
- What happens when tile-aligned padding would make the archive larger than the original file? The engine detects this case during compression and falls back to storing the data uncompressed within the GPU format, preserving the tile structure for consistency.

## Requirements

### Functional Requirements

- **FR-001**: System MUST compress files using GPU-parallel tile-based processing when the GPU plugin is selected.
- **FR-002**: System MUST decompress GPU-compressed archives on any system, with or without a GPU present (CPU fallback decompression required).
- **FR-003**: System MUST produce byte-for-byte identical output when decompressing, regardless of which backend (NVIDIA, AMD, Apple, CPU) performs the decompression.
- **FR-004**: System MUST use 64KB tile size for GPU-optimized blocks, with each tile independently decompressible.
- **FR-005**: System MUST align tile data to 128-byte boundaries for GPU memory coalescing.
- **FR-006**: System MUST include a tile index in the archive format enabling O(1) random access to any tile.
- **FR-007**: System MUST detect GPU vendor and select the appropriate compute backend: CUDA-optimized for NVIDIA, Vulkan compute for AMD, Metal compute for Apple Silicon. Minimum requirements: Vulkan 1.2 / Metal 2 compute support and 2GB VRAM. GPUs below this threshold are treated as "no GPU available."
- **FR-008**: System MUST fall back to CPU-based parallel compression when no compatible GPU is available.
- **FR-009**: System MUST expose a scoring function to the crush plugin selector that evaluates file size (>100MB), GPU availability, and data suitability.
- **FR-010**: System MUST sample input data to assess GPU compression suitability — files with Shannon entropy above 7.5 bits/byte are classified as unsuitable (random/encrypted data).
- **FR-011**: System MUST support cooperative cancellation via the existing crush cancellation token, releasing GPU resources on cancel.
- **FR-012**: System MUST report progress during compression and decompression via the existing crush progress callback mechanism.
- **FR-013**: System MUST batch processing to stay within GPU memory limits, segmenting files that exceed available GPU memory.
- **FR-014**: System MUST register as a crush plugin using the existing `CompressionAlgorithm` trait and compile-time registration mechanism.
- **FR-015**: System MUST apply vectorized string matching only when a pre-compression sample analysis confirms it would reduce output size compared to standard matching.
- **FR-016**: System MUST use 32-way parallelism within each tile, splitting the compressed bitstream into 32 sub-streams for parallel decoding.

### Key Entities

- **GPU Tile**: A fixed-size (64KB decompressed) independent compression unit. Contains a self-describing header with a format version byte, 32 interleaved sub-streams, and CRC32 checksum. The decompressor rejects tiles with unrecognized version numbers rather than producing corrupt output. The fundamental unit of parallel processing.
- **Tile Index**: A compact lookup table stored in the archive footer mapping tile number to byte offset and compressed size. Enables O(1) random access to any tile.
- **GPU Archive**: A Crush-format file containing a Crush header, a sequence of GPU tiles, and a tile index. Self-describing and decompressible without GPU.
- **Compute Backend**: An abstraction over GPU vendor APIs (CUDA, Vulkan compute, Metal compute) that provides a uniform interface for dispatching compression/decompression kernels.
- **Eligibility Scorer**: A component that evaluates file size, GPU availability, and data entropy to determine whether GPU compression would benefit a given file. Returns a score to the plugin selector.

## Success Criteria

### Measurable Outcomes

- **SC-001**: Users compressing files over 200MB achieve at least 4x higher throughput compared to CPU-only parallel compression on the same system.
- **SC-002**: Compression ratio is within 5% of standard DEFLATE for the same input data.
- **SC-003**: GPU-compressed archives decompress successfully on systems without a GPU, with CPU-fallback decompression throughput no worse than standard CPU parallel decompression.
- **SC-004**: GPU plugin correctly declines files under 100MB, files on systems without GPUs, and files with unsuitable data characteristics, with zero false claims in a test corpus of 50+ files.
- **SC-005**: Compression and decompression produce byte-for-byte identical round-trip results across all supported GPU backends and CPU fallback.
- **SC-006**: GPU memory usage during compression stays under 256MB regardless of input file size.
- **SC-007**: The engine supports at least three GPU vendors (NVIDIA, AMD, Apple Silicon) through appropriate compute backends.
- **SC-008**: Vectorized string matching, when activated, produces files at least 3% smaller than standard matching on string-heavy data (logs, CSV, JSON).
- **SC-009**: Random access decompression of a single tile completes without reading more than the tile index and the target tile's data from the archive.
- **SC-010**: Cancellation of an in-progress GPU operation releases all GPU resources within 1 second.

## Clarifications

### Session 2026-02-23

- Q: What entropy threshold defines "unsuitable data" for GPU compression (FR-010)? → A: Shannon entropy above 7.5 bits/byte (standard heuristic matching common compression tools).
- Q: What minimum GPU hardware is required for "compatible GPU"? → A: Vulkan 1.2 / Metal 2 compute support + minimum 2GB VRAM (~2019+ modern GPUs).
- Q: How should the GPU tile format handle versioning for forward compatibility? → A: Version byte in each tile header; decompressor rejects unknown versions.

## Assumptions

- **A-001**: The primary target GPU architecture is NVIDIA with CUDA, as it has the most mature ecosystem for GPU compression (nvCOMP/GDeflate). AMD and Apple are secondary targets via cross-platform compute APIs.
- **A-002**: The 64KB tile size follows the GDeflate specification and is optimal for balancing GPU parallelism with compression ratio. This is an industry-proven choice used by Microsoft DirectStorage and NVIDIA nvCOMP.
- **A-003**: 32-way sub-stream parallelism within tiles matches the SIMD width of modern GPU warp/wavefront architectures (NVIDIA warp = 32 threads, AMD wavefront = 64 threads, Apple SIMD group = 32 threads).
- **A-004**: The 100MB file size threshold is a reasonable default for when GPU overhead (initialization, data transfer) is amortized. Smaller files would not benefit from GPU processing due to fixed overhead costs.
- **A-005**: "Data suitability" for GPU compression is determined by Shannon entropy analysis — data with entropy above 7.5 bits/byte (encrypted, already-compressed, random) does not benefit from compression of any kind and should be rejected. This threshold matches standard compression tool heuristics and catches truly incompressible data while allowing structured binary formats through.
- **A-006**: The pure Rust implementation uses compute shaders (WGSL via wgpu for portability) rather than binding directly to vendor-specific SDKs. CUDA-specific optimizations may use cudarc for the NVIDIA fast path.
- **A-007**: Multi-GPU support is deferred to a future iteration. The initial implementation selects the single most capable GPU.
- **A-008**: String vectorization refers to using SIMD/GPU-parallel techniques for dictionary matching during the LZ compression phase, where processing multiple match candidates simultaneously can find longer matches and improve compression ratio on text-heavy data.
- **A-009**: The file format is a new GPU-optimized Crush format (not a GDeflate-compatible bitstream). It draws inspiration from GDeflate's tile and sub-stream architecture but uses Crush's own header, magic number, and plugin system.
