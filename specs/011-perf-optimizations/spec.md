# Feature Specification: Hot-Path Performance Optimizations

**Feature Branch**: `011-perf-optimizations`
**Created**: 2026-04-17
**Status**: Draft
**Input**: User description: "review application and recommend refactors to improve performance" — findings from the 2026-04-17 code review of [crush-parallel/src/engine.rs](../../crush-parallel/src/engine.rs), [crush-parallel/src/block.rs](../../crush-parallel/src/block.rs), [crush-parallel/src/index.rs](../../crush-parallel/src/index.rs), and [crush-core/src/compression.rs](../../crush-core/src/compression.rs).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Faster whole-file compression on multi-core machines (Priority: P1)

A developer compressing a multi-gigabyte dataset with the Crush CLI on an 8-core workstation today sees throughput bottlenecked by per-block allocator traffic, redundant input copies, and a serial output-assembly loop. After this feature, the same command finishes measurably faster on the same hardware, using the same CRSH output format, with no visible change to command-line arguments or on-disk output.

**Why this priority**: This is the single biggest user-facing benefit and directly supports the constitution's core principle of matching or exceeding pigz throughput.

**Independent Test**: Run the existing [crush-parallel/benches/throughput.rs](../../crush-parallel/benches/throughput.rs) benchmark on a representative workload before and after the change and observe a measurable throughput improvement; round-trip tests and the existing CRSH format test continue to pass unchanged.

**Acceptance Scenarios**:

1. **Given** a 1 GB input file and the default compression level, **When** the user compresses the file with the Crush CLI on an 8-core machine, **Then** wall-clock time is lower than the pre-change baseline by a measurable, statistically significant margin, and the output file decompresses back to the original input byte-for-byte.
2. **Given** a compressed CRSH file produced by the previous version, **When** the user decompresses it with the new version, **Then** the output is byte-for-byte identical to the original input.
3. **Given** a compressed CRSH file produced by the new version, **When** the user decompresses it with the previous version, **Then** the output is byte-for-byte identical to the original input (forward compatibility of the format is preserved).

---

### User Story 2 - Lower peak memory use during compress and decompress (Priority: P2)

A user compressing or decompressing a large file on a memory-constrained system (CI runner, container with tight limits) currently sees peak resident memory roughly twice the uncompressed size during decompression, because the decoded blocks are allocated individually and then copied into a single output buffer. After this feature, peak memory for the same operation is measurably lower because the output is allocated once and written directly by worker threads, and the compression path avoids a full-sized pre-compression copy of the input.

**Why this priority**: Memory pressure is a real failure mode (OOM-kills) before it becomes a throughput issue; fixing it expands the set of environments where Crush can run a given workload.

**Independent Test**: Measure peak RSS during a full-file compress and decompress of a reference input (e.g. 1 GB) before and after the change; confirm a measurable reduction and that all existing round-trip and property tests still pass.

**Acceptance Scenarios**:

1. **Given** a 1 GB reference input, **When** the user runs compress and then decompress with the new version, **Then** peak resident memory reported by the OS is measurably lower than the pre-change baseline on both paths.
2. **Given** an input stream delivered via a pipe (rather than a file on disk), **When** the user compresses it, **Then** the operation completes without loading the entire stream into memory before compression begins.

---

### User Story 3 - Predictably fast random-access block lookups (Priority: P3)

A downstream library or tool that opens a CRSH file and fetches many blocks by uncompressed-byte offset (for example, to implement partial-file extraction) currently pays a linear scan of the block index on each lookup. After this feature, index lookups take effectively constant or logarithmic time regardless of file size, so tools that perform many random-access reads no longer scale poorly with block count.

**Why this priority**: Random-access is an advertised property of the format but is used by a small fraction of callers today; the fix is cheap but lower-leverage than P1 and P2.

**Independent Test**: On a compressed file with a large number of blocks (e.g. 10,000), perform many random-access lookups before and after the change; confirm the new version's per-lookup cost is effectively independent of block count while producing identical results.

**Acceptance Scenarios**:

1. **Given** a CRSH file with 10,000 blocks, **When** a caller performs 10,000 random-access block lookups by uncompressed offset, **Then** total elapsed time is dramatically lower than the pre-change baseline.
2. **Given** the same file, **When** a caller queries `uncompressed_offset(n)` or `block_for_offset(off)`, **Then** the returned value is identical to the pre-change result for every valid input.

---

### Edge Cases

- Empty input: compression must still produce a valid CRSH file with zero blocks; decompression of that file must return an empty byte slice.
- Incompressible input (random bytes): stored-block fallback must still work correctly and must not be slower than before.
- Single-block input (smaller than one `block_size`): parallel paths must handle the single-worker case without overhead or deadlock.
- Cancellation mid-operation: existing `Cancelled` behavior must be preserved; no change in how or when cancellation is observed from the caller's perspective.
- Corrupt payload, corrupt checksum, truncated footer, version mismatch: existing error types and error messages must remain observably identical.
- Streaming source with unknown total size: decompression must continue to work without assuming total size is known in advance.
- Input that exceeds `max_decompression_ratio`: expansion-limit guard must continue to fire before any large allocation occurs.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The public library API of `crush-core` and `crush-parallel` MUST NOT change in any breaking way; existing callers MUST compile and behave identically at the type and function-signature level.
- **FR-002**: The on-disk CRSH file format MUST remain byte-identical for inputs that were previously handled without fallback; in particular, a CRSH file produced by the previous version MUST decompress correctly with the new version, and vice versa.
- **FR-003**: All existing unit tests, integration tests, property-based round-trip tests, and fuzz harnesses MUST continue to pass without modification of their assertions.
- **FR-004**: Compression of a representative large input (size defined in `quickstart.md`) on a multi-core machine MUST show a measurable, statistically significant throughput improvement over the pre-change baseline.
- **FR-005**: Decompression of the same representative input MUST show a measurable, statistically significant throughput improvement over the pre-change baseline.
- **FR-006**: Peak resident memory during compression and decompression of the same representative input MUST be measurably lower than the pre-change baseline.
- **FR-007**: Per-block worker threads MUST reuse compression and decompression state across the blocks they process, rather than allocating fresh state per block.
- **FR-008**: The final compressed output buffer MUST be allocated once with a known capacity, not grown incrementally by repeated reallocation.
- **FR-009**: The final decompressed output buffer MUST be allocated once with the known total uncompressed size, and worker threads MUST write their per-block output directly into disjoint regions of that buffer rather than producing per-block allocations that are later copied into a final buffer.
- **FR-010**: The library MUST NOT copy the entire input buffer solely for the purpose of moving it into a worker thread; either scoped threads or another borrow-preserving mechanism MUST be used so that large inputs are not duplicated.
- **FR-011**: Block-index lookups that today require a linear scan over entries MUST be answerable in constant or logarithmic time, using a cumulative-offset table constructed at index-load time.
- **FR-012**: Observable error behavior — including error variants, error messages, and the conditions under which each error is returned — MUST remain unchanged.
- **FR-013**: Progress-callback observable behavior — specifically that the callback is invoked at least once per block and that returning `false` cancels the operation before the next block completes — MUST remain unchanged.
- **FR-014**: Any code that uses `unsafe` to avoid zero-initialization of buffers MUST assert, in debug builds, that the reported number of bytes written by the underlying library does not exceed the buffer's length, so that a library regression cannot silently expose uninitialized memory.
- **FR-015**: The streaming compression entry point MUST NOT require buffering the entire input in memory before any block is compressed. (If this requirement cannot be met within the scope of this feature, it MUST be explicitly deferred in the plan with a tracking note.)

### Key Entities

- **Engine configuration**: unchanged externally; represents the caller-provided knobs (block size, compression level, worker count, checksums flag, progress callback, cancellation token).
- **Block index**: unchanged on disk; internally gains a precomputed cumulative-offset table so that lookups by uncompressed byte offset no longer require a linear scan.
- **Compressed block**: unchanged on disk; internally may change its in-memory representation (for example, to borrow from the input rather than own a copy for stored-fallback blocks) without altering the caller-visible type.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On an 8-core reference machine, full-file compression of the representative 1 GB input completes in at least 15% less wall-clock time than the pre-change baseline at the default compression level.
- **SC-002**: On the same machine, full-file decompression of the same compressed file completes in at least 25% less wall-clock time than the pre-change baseline.
- **SC-003**: Peak resident memory during compression is at most 1.25× the uncompressed input size (versus the pre-change baseline which routinely exceeds 2×), and peak resident memory during decompression is at most 1.25× the uncompressed output size.
- **SC-004**: On a CRSH file with 10,000 blocks, a loop of 10,000 random-access lookups by uncompressed offset completes in at least 100× less total time than the pre-change baseline.
- **SC-005**: The existing benchmark suite shows no regression greater than 5% on any benchmark outside the targeted hot paths (compression and decompression). This satisfies the constitution's "< 5% slowdown" quality gate.
- **SC-006**: All existing tests pass, including round-trip, property-based, and fuzz-reachable paths; the 100,000-iteration fuzz run required by the constitution stays clean.
- **SC-007**: The feature ships without any change to the public API of `crush-core` or `crush-parallel`, so that no downstream caller needs to be modified to adopt the improvement.

## Assumptions

- The reference hardware for SC-001 through SC-004 is the machine specified in the feature's `quickstart.md`; percentages are relative to the baseline captured on that same machine, not an absolute wall-clock target.
- The representative 1 GB input is a mixed-entropy file; the specific fixture is defined in `quickstart.md`. Highly compressible synthetic inputs (all zeros) and fully incompressible inputs (random bytes) are covered by round-trip tests, not by the throughput success criteria.
- "libdeflater" remains the DEFLATE implementation for Phase 1; swapping it out is explicitly out of scope.
- `rayon` remains the parallel-dispatch runtime; adopting an async runtime is explicitly out of scope.
- Streaming (FR-015) may be partially addressed in this feature. If true end-to-end streaming requires a deeper architectural change (block-pipeline with bounded channels), it is acceptable to land the non-streaming optimizations first and defer streaming to a follow-up feature, provided that is recorded explicitly in the plan.
