# Feature Specification: GDeflate GPU Decompression

**Feature Branch**: `009-gdeflate-gpu-decompression`
**Created**: 2026-03-01
**Status**: Draft
**Input**: User description: "Replace the current LZ77 GPU decompression algorithm with GDeflate - an open standard GPU-optimized compression format developed by Microsoft/NVIDIA for DirectStorage. GDeflate reformats DEFLATE streams to extract 32-way parallelism per 64KB tile, matching our existing tile architecture. The HLSL reference decompressor shader should be ported to WGSL for cross-platform wgpu support. CPU-side compression should produce GDeflate-formatted bitstreams. The existing tile format, file headers, and engine orchestration should be preserved where possible. This should dramatically improve GPU decompression throughput from ~130 MiB/s to multi-GB/s range while maintaining DEFLATE-equivalent compression ratios."

## User Scenarios & Testing

### User Story 1 - GPU-Accelerated Decompression with GDeflate (Priority: P1)

A user compresses a large file using Crush and later decompresses it. When a compatible GPU is available, the decompression path uses the GDeflate algorithm running as a compute shader on the GPU. The user experiences dramatically faster decompression compared to the previous LZ77-based GPU path, achieving throughput in the multi-GB/s range rather than the current ~130 MiB/s. The decompressed output is byte-for-byte identical to the original input.

**Why this priority**: This is the core value proposition. Without a working GDeflate decompression shader, the feature delivers no improvement. The GPU decompression path is where the 10-40x speedup is realized.

**Independent Test**: Can be fully tested by compressing test data with the GDeflate compressor, then decompressing on a GPU-equipped machine and verifying byte-for-byte roundtrip correctness and throughput improvement over the old LZ77 path.

**Acceptance Scenarios**:

1. **Given** a file compressed with Crush's GDeflate format, **When** decompressing on a machine with a compatible GPU, **Then** the GPU decompression path is selected automatically and produces byte-for-byte identical output to the original file.
2. **Given** a 10 MB test file, **When** decompressing via the GPU GDeflate path, **Then** decompression throughput exceeds 1 GB/s.
3. **Given** a GDeflate-compressed file, **When** decompressing on a machine without a GPU, **Then** the system falls back to CPU-based GDeflate decompression and produces correct output.

---

### User Story 2 - GDeflate Compression (CPU-Side Encoder) (Priority: P2)

A user compresses a file using Crush and the system produces a GDeflate-formatted bitstream. Compression happens on the CPU (GDeflate compression is not GPU-accelerated). The compressed output uses the GDeflate sub-stream layout so that it can be decompressed in parallel on the GPU. The compression ratio is equivalent to standard DEFLATE for the same data.

**Why this priority**: Without a compressor that emits GDeflate-formatted output, there is no data for the GPU decompressor to consume. This is a direct prerequisite for US1, but is separated because compression and decompression are independently testable.

**Independent Test**: Can be tested by compressing data, then decompressing with the CPU-only GDeflate decoder and verifying roundtrip correctness and compression ratio.

**Acceptance Scenarios**:

1. **Given** arbitrary input data, **When** compressing with Crush's GDeflate compressor, **Then** the output is a valid GDeflate bitstream with 64KB tile granularity.
2. **Given** English text input, **When** comparing GDeflate compression ratio to the existing DEFLATE-based compression, **Then** the GDeflate ratio is within 5% of the DEFLATE ratio.
3. **Given** a file larger than 64KB, **When** compressing, **Then** the output contains multiple independently-decompressible 64KB tiles.

---

### User Story 3 - Backward Compatibility and Format Coexistence (Priority: P3)

A user who has files compressed with the existing LZ77-based GPU format can still decompress them. The system detects the format version in the file header and routes to the appropriate decompressor (LZ77 for old files, GDeflate for new files). New compressions default to GDeflate when GPU decompression is the target.

**Why this priority**: Users may have existing compressed archives. Breaking backward compatibility would force re-compression of all existing data, which is unacceptable for a compression tool.

**Independent Test**: Can be tested by decompressing files created with the previous LZ77 GPU format and verifying they still produce correct output.

**Acceptance Scenarios**:

1. **Given** a file compressed with the old LZ77 GPU format (format version 1), **When** decompressing, **Then** the system detects the version and decompresses correctly using the LZ77 path.
2. **Given** a file compressed with the new GDeflate format (format version 2), **When** decompressing, **Then** the system detects the version and decompresses via the GDeflate path.
3. **Given** mixed format files, **When** decompressing each, **Then** each produces correct output regardless of format version.

---

### User Story 4 - Performance Benchmarking and Validation (Priority: P4)

A developer runs the existing benchmark suite and sees comparative results for the old LZ77 GPU path versus the new GDeflate GPU path. The benchmarks confirm that GDeflate provides a substantial throughput improvement for decompression while maintaining comparable compression ratios.

**Why this priority**: Benchmarks validate the performance claims and prevent regressions. They are not user-facing but are essential for development confidence and ongoing quality.

**Independent Test**: Can be tested by running `cargo bench` and comparing GDeflate throughput numbers against LZ77 baseline and CPU-only paths.

**Acceptance Scenarios**:

1. **Given** the benchmark suite, **When** running decompression benchmarks, **Then** the GDeflate GPU path shows at least 5x throughput improvement over the old LZ77 GPU path.
2. **Given** the benchmark suite, **When** running compression ratio benchmarks, **Then** GDeflate compression ratio is within 5% of the existing DEFLATE-based ratio for the same test corpus.

---

### Edge Cases

- What happens when compressed data is truncated mid-tile? The decompressor must detect corruption and return an error rather than producing garbage output.
- How does the system handle a tile with fewer than 32 active sub-streams? The GDeflate shader must correctly handle tiles where some sub-streams are empty (tail tiles smaller than 64KB).
- What happens when GPU memory is insufficient to hold all tiles for a batch dispatch? The engine must split into smaller batches that fit within the GPU memory budget.
- What happens when the GPU device becomes lost during decompression? The system must catch the failure and fall back to CPU decompression (existing safety mechanism).
- How are zero-length files handled? Compression and decompression of empty input must produce empty output without errors.

## Requirements

### Functional Requirements

- **FR-001**: System MUST decompress GDeflate-formatted bitstreams on the GPU using a compute shader with 32-way parallelism per tile.
- **FR-002**: System MUST compress input data into GDeflate-formatted bitstreams on the CPU, producing 64KB tiles with 32 sub-streams each.
- **FR-003**: System MUST produce byte-for-byte identical output when decompressing a GDeflate-compressed file, regardless of whether GPU or CPU decompression is used.
- **FR-004**: System MUST detect the compression format version from file headers and route to the appropriate decompressor (LZ77 for version 1, GDeflate for version 2).
- **FR-005**: System MUST fall back to CPU-based GDeflate decompression when no compatible GPU is available.
- **FR-006**: System MUST maintain compression ratios within 5% of standard DEFLATE for equivalent input data.
- **FR-007**: System MUST support random-access decompression at tile granularity (64KB boundaries) in the GDeflate format.
- **FR-008**: System MUST handle tail tiles (final tile smaller than 64KB) correctly in both compression and decompression.
- **FR-009**: System MUST batch GPU tile dispatches within the existing GPU memory budget to avoid out-of-memory conditions.
- **FR-010**: System MUST preserve the existing file header structure, tile index, and footer format, updating only the format version field to distinguish GDeflate from LZ77.
- **FR-011**: System MUST validate GDeflate bitstream integrity using CRC32 checksums per tile, consistent with the existing checksum mechanism.

### Key Entities

- **GDeflate Tile**: A 64KB page of data compressed into 32 independent sub-streams using the GDeflate bitstream format. Each tile is independently decompressible and randomly accessible.
- **Sub-stream**: One of 32 independent bitstreams within a GDeflate tile. Each sub-stream is assigned to one GPU compute thread (SIMD lane) for parallel decoding.
- **Format Version**: A field in the file header distinguishing LZ77 (version 1) from GDeflate (version 2) compressed data.
- **GDeflate Bitstream**: A reformatted DEFLATE stream where variable-length codes are distributed round-robin across 32 sub-streams, enabling parallel GPU parsing.

## Success Criteria

### Measurable Outcomes

- **SC-001**: GPU decompression throughput exceeds 1 GB/s for files larger than 1 MB on a mid-range GPU (e.g., GTX 1060 or equivalent).
- **SC-002**: GDeflate compression ratio is within 5% of the existing DEFLATE-based compression ratio for the standard benchmark corpus.
- **SC-003**: All existing roundtrip tests pass without modification when using GDeflate format.
- **SC-004**: Files compressed with the previous LZ77 format continue to decompress correctly (zero regressions).
- **SC-005**: GPU decompression throughput is at least 5x faster than the previous LZ77 GPU path (~130 MiB/s baseline).
- **SC-006**: CPU fallback decompression of GDeflate data produces identical output to GPU decompression within acceptable performance (no worse than 50% of current CPU decompression speed).
- **SC-007**: No system-wide crashes or GPU device loss events during normal operation, including sustained batch processing.

## Assumptions

- The GDeflate IETF draft specification and Microsoft's Apache 2.0 reference implementation provide sufficient detail to build a compliant compressor and WGSL decompressor without proprietary dependencies.
- The existing 64KB tile size in crush-gpu aligns with GDeflate's page size, so no tile-size changes are needed.
- The HLSL reference decompressor from Microsoft's DirectStorage repository can be ported to WGSL without fundamental incompatibilities (WGSL supports the required 32-bit integer operations, bitwise manipulation, and workgroup shared memory).
- GPU compression is out of scope; compression remains CPU-only. GDeflate's value is in GPU decompression throughput, not compression speed.
- The existing cached GPU backend singleton (OnceLock) and catch_unwind safety mechanisms from the 008 feature will be preserved and reused.
- Cross-platform support (Vulkan, Metal, DX12) is provided by wgpu's abstraction layer; no platform-specific GPU code is needed beyond the WGSL shader.
