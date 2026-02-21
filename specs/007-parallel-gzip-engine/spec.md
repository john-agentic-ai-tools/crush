# Feature Specification: Parallel Compression Engine

**Feature Branch**: `007-parallel-gzip-engine`
**Created**: 2026-02-21
**Status**: Draft
**Input**: User description: "Create new crate that implements the pigz inspired multithreaded gzip implementation. Our version should learn as much as possible from the pigz implementation but since we do not need to have a zip compatible binary we are free to change the file format to be better suited for parallel processing. We also should look at ways to use GPU instead of or in addition to CPU"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Multi-Core CPU Compression (Priority: P1)

A developer integrating Crush into a data pipeline wants to compress large files and streams as fast as possible, using all available CPU cores. They invoke the compression API and observe near-linear speedup with core count compared to single-threaded gzip.

**Why this priority**: This is the core value proposition — parallelising the compression work across all available cores. Without this, no other story delivers meaningful value. Matches the constitution's performance target of matching/exceeding pigz on 4+ cores.

**Independent Test**: Can be fully tested by compressing a multi-gigabyte file with varying thread counts (1, 2, 4, 8) and measuring wall-clock throughput. Delivers the primary user value: fast compression.

**Acceptance Scenarios**:

1. **Given** a file larger than 64 MB and 4 available CPU cores, **When** compression is invoked with default settings, **Then** throughput is at least 3× that of a single-threaded run on the same hardware.
2. **Given** a stream of arbitrary size with no known end, **When** streaming compression is invoked, **Then** the engine compresses data as it arrives without buffering the full input.
3. **Given** a single-core machine, **When** compression is invoked, **Then** the engine falls back to sequential compression and produces a valid output file.
4. **Given** the compressed output, **When** decompressed by this engine, **Then** the output is byte-for-byte identical to the original input.

---

### User Story 2 - Parallel Decompression (Priority: P2)

A developer decompressing files produced by the engine wants decompression to also benefit from multiple CPU cores, unlike standard gzip whose sequential DEFLATE stream prevents parallelisation. The custom file format encodes independent block boundaries so any block can be decompressed without reading preceding blocks.

**Why this priority**: Parallel decompression is only possible because we own the format. This is a key differentiator from pigz, which cannot parallelise decompression of standard gzip output. Unlocks symmetric performance for both halves of the pipeline.

**Independent Test**: Can be fully tested by compressing a file with this engine, then decompressing it with varying thread counts (1, 2, 4, 8) and measuring throughput. Independent of GPU story.

**Acceptance Scenarios**:

1. **Given** a file compressed by this engine and 4 available CPU cores, **When** decompression is invoked, **Then** throughput is at least 3× that of a single-threaded decompression run on the same hardware.
2. **Given** a compressed file, **When** a consumer requests decompression starting at block N (random access), **Then** the engine decompresses from that block without reading or decompressing any earlier block.
3. **Given** a compressed file with a corrupted block, **When** decompression is invoked, **Then** the engine reports exactly which block is corrupted and halts, leaving all successfully decompressed blocks intact.

---

### User Story 3 - GPU-Accelerated Compression (Priority: P3)

A developer running on hardware with a discrete GPU wants to offload compression work to the GPU to achieve throughput beyond what the CPU alone can deliver. When no GPU is available, the engine silently falls back to CPU-only processing.

**Why this priority**: GPU acceleration is a significant differentiator but depends on P1 delivering a working parallel block model first. It is additive — the system works fully without it, making it a safe P3.

**Independent Test**: Can be fully tested by compressing the same file with CPU-only and GPU-enabled modes and comparing throughput and output correctness. Does not require P2 (decompression) to be present.

**Acceptance Scenarios**:

1. **Given** a machine with a supported GPU and a large input file, **When** GPU acceleration is enabled, **Then** end-to-end compression throughput exceeds the CPU-only throughput on the same machine by at least 20%.
2. **Given** a machine with no supported GPU, **When** GPU acceleration is requested, **Then** the engine logs a warning and proceeds with CPU-only compression without error.
3. **Given** a GPU-compressed file, **When** decompressed by any compliant implementation of this format, **Then** the output is byte-for-byte identical regardless of whether compression used CPU or GPU.
4. **Given** a GPU failure mid-compression, **When** the error is detected, **Then** the engine falls back to CPU completion for the remaining blocks and reports the partial GPU failure.

---

### User Story 4 - Seekable Random Access (Priority: P4)

A data engineer working with large compressed archives needs to read a specific region of the data (e.g., row group 47 out of 10,000) without decompressing the entire file. The format's block index enables seeking directly to the relevant compressed block.

**Why this priority**: Enables columnar / analytics workloads where random access into compressed data is essential. Builds directly on the block-indexed format introduced in P1/P2 with no new compression logic required.

**Independent Test**: Can be tested by compressing a known dataset, then extracting a specific byte range and verifying it matches the original without reading the rest of the file. Independent of GPU story.

**Acceptance Scenarios**:

1. **Given** a compressed file with a known block index, **When** a consumer requests the data at a specific byte offset, **Then** the engine reads at most 2 blocks to fulfil the request (the containing block and its boundary neighbour).
2. **Given** a very large compressed file (> 1 GB), **When** random access is performed on the last block, **Then** the time to first byte is under 100 ms regardless of file size.

---

### Edge Cases

- What happens when input data is already highly compressed or incompressible (e.g., encrypted data)? The engine must not produce output larger than a configurable threshold above raw input size; if so, it should store the block uncompressed.
- How does the engine handle input that arrives slower than the compression rate (backpressure)? Worker threads must block cleanly without spinning.
- What if block size is set larger than available memory per thread? The engine must reject the configuration with a clear error before starting.
- What happens if the compressed file's block index is missing or truncated? The engine must detect this and refuse to decompress rather than silently producing corrupt output.
- What if the number of CPU cores changes during compression (e.g., cgroups adjustment)? The engine must complete with the thread pool it started with and not crash.
- What if the caller cancels a compression or decompression in progress? The engine must halt at the next block boundary, discard partial output, and return a distinct `Cancelled` result — not an error — so callers can distinguish intentional cancellation from failures.
- What if the engine encounters a file produced by a different engine version? The engine must refuse to process it and emit an error naming both the file's producer version and the current engine version, so the user knows exactly which version to use.
- What if decompressed output would exceed the caller's configured expansion limit? The engine must halt immediately with a clear error identifying the offending block, not silently truncate output.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The engine MUST divide input data into independently compressible blocks and compress each block concurrently across available processing units.
- **FR-002**: The compressed output format MUST embed a block index that records the byte offset and uncompressed size of every block, enabling decompression to start at any block without reading preceding blocks.
- **FR-003**: The engine MUST produce output that decompresses to a byte-for-byte identical copy of the original input, verified by a per-block integrity checksum stored in the format.
- **FR-004**: The engine MUST expose a streaming compression interface that accepts data in chunks of arbitrary size and does not require the full input to be available before compression begins.
- **FR-005**: The engine MUST expose a streaming decompression interface that can decompress any block in isolation given only the compressed file and the target block's offset from the index.
- **FR-006**: The engine MUST detect and report corrupt or truncated compressed data before emitting any decompressed output for the affected block.
- **FR-007**: The engine MUST allow the calling application to configure the number of parallel workers, block size, and compression level independently.
- **FR-008**: When GPU acceleration is enabled, the engine MUST automatically detect whether compatible GPU hardware is present and fall back to CPU processing if not.
- **FR-009**: The engine MUST expose its functionality as a library with a stable public API, usable independently of any command-line interface.
- **FR-010**: The engine MUST record per-block checksums in the format and validate them on decompression, reporting the exact block index of any integrity failure.
- **FR-011**: The engine MUST enforce a configurable maximum decompression expansion ratio with a safe default. If decompressed output would exceed the caller-configured limit, the engine MUST halt and return an error before emitting further output. Callers may raise or explicitly disable this limit.
- **FR-014**: The compressed format MUST store the engine version that produced it in the file header. When the engine encounters a file produced by a different version, it MUST refuse to decompress and emit an error that identifies both the file's producer version and the current engine version, so the user knows which version to install to read the file.
- **FR-012**: The engine MUST accept an optional progress callback in its configuration. The callback is invoked after each block completes and receives: bytes processed so far, total blocks completed, and total blocks if known. The callback returns a boolean; returning `false` signals the engine to abort. The engine MUST halt at the next block boundary, discard any partial output, and return a `Cancelled` result to the caller. The callback is optional; omitting it incurs no overhead.
- **FR-013**: The crush-cli crate MUST provide a working reference implementation of the progress callback, rendering a progress indicator to the terminal during compression and decompression operations.

### Key Entities

- **CompressionBlock**: An independently compressible unit of the input stream. Has a fixed (configurable) uncompressed size and produces a self-contained compressed payload. Each block carries its own integrity checksum.
- **BlockIndex**: A manifest embedded in the compressed output that maps block number to its byte offset in the output and its uncompressed size. Enables O(1) seek to any block.
- **CompressionJob**: A unit of work dispatched to a worker (CPU thread or GPU kernel). Contains a reference to an input block and a destination buffer for the compressed output.
- **EngineConfiguration**: The set of parameters controlling engine behaviour: worker count, block size, compression level, GPU enablement flag, maximum decompression expansion ratio, and an optional progress callback.
- **CompressedStream**: The complete output artefact. Consists of a file header (producer engine version, format configuration), a sequence of compressed blocks, and a trailing BlockIndex. The producer version is used solely for error reporting when a version mismatch is detected.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On an 8-core CPU machine, compression throughput exceeds 500 MB/s for inputs larger than 128 MB at the default compression level.
- **SC-002**: Compression throughput scales at least linearly from 1 to 4 cores, with no more than 15% efficiency loss at 8 cores versus linear projection.
- **SC-003**: Decompression throughput matches compression throughput within 20% on the same hardware and thread count.
- **SC-004**: Random access to any block in a compressed file (seek + decompress single block) completes in under 100 ms for files up to 10 GB.
- **SC-005**: When GPU acceleration is active on supported hardware, end-to-end compression throughput is at least 20% higher than CPU-only on the same machine.
- **SC-006**: Compressed output size is within 5% of equivalent single-threaded gzip output at the same nominal compression level for compressible data.
- **SC-007**: 100% of decompressed outputs are byte-for-byte identical to their inputs, verified across all block sizes, thread counts, and hardware paths (CPU and GPU).
- **SC-008**: The engine correctly detects and reports every injected block corruption in fuzz testing over a minimum of 100,000 iterations.

## Assumptions

- **A-001**: The primary use case is files and streams large enough to benefit from parallelisation (> 10 MB). Small inputs may show no speedup; this is acceptable and documented.
- **A-002**: The custom format is not required to be forward-compatible with standard gzip decompressors. Interoperability with gzip is explicitly out of scope.
- **A-003**: GPU acceleration targets discrete GPU hardware capable of general-purpose compute. Integrated graphics are out of scope for P3.
- **A-004**: Block size defaults to 1 MB, consistent with pigz's default, as a starting point. This is tunable at runtime.
- **A-005**: The engine is a library, not a standalone binary. CLI integration is handled by a separate crate (crush-cli) already in the workspace.
- **A-006**: Thread safety is required: multiple callers may create independent engine instances concurrently; shared state between instances is not assumed or required.
- **A-007**: Multi-file archive support is explicitly out of scope. The engine operates on a single byte stream; callers are responsible for any multi-file bundling upstream (e.g., piping a tar stream into the engine).

## Clarifications

### Session 2026-02-21

- Q: Should the engine natively support storing multiple named files in one compressed container? → A: Out of scope — engine operates on a single byte stream; callers handle multi-file bundling upstream.
- Q: Should the engine enforce a decompression expansion limit? → A: Configurable limit with a safe default — engine enforces a max expansion ratio; callers can raise or disable it explicitly.
- Q: Should the engine expose progress reporting? → A: Optional callback per completed block (bytes processed, blocks done/total); crush-cli must provide a reference terminal progress implementation.
- Q: What is the format versioning / compatibility strategy? → A: No backward compatibility — each engine version only reads files it produced; version mismatch must emit a clear error naming both the file's producer version and the current engine version.
- Q: How should callers cancel an in-progress operation? → A: Callback return value — callback returns false to abort; engine halts at next block boundary, discards partial output, and returns a distinct Cancelled result.
