# Feature Specification: Parallel Fast Compression

**Feature Branch**: `027-compression-perf-plan`
**Created**: 2026-11-01
**Status**: Implemented. SC-002 missed and accepted by the project owner on 2026-11-01, see `perf/log/2026-11-parallel-fast-compression.md`
**Input**: Rescoped from a repository-wide review of compression performance. Only the changes that survived measurement remain. See [research.md](research.md).

## User Scenarios & Testing

### User Story 1 - Fast compression uses every core (Priority: P1)

A user who picks the fast plugin on a large file gets a result several times faster on a multi-core machine, and the file is the same as before.

**Why this priority**: The fast plugin is single-threaded today, so it is the largest measured gain available.

**Independent Test**: Compress one large file with 1 and with all cores, compare the times, and compare the output bytes.

**Acceptance Scenarios**:

1. **Given** a file of many blocks on a machine with 4 or more cores, **When** the user compresses it with the fast plugin, **Then** throughput meets SC-001 and SC-002.
2. **Given** the same input, **When** it is compressed with any number of cores, **Then** the output bytes are identical to today's.
3. **Given** an existing fast archive, **When** it is decompressed, **Then** the result is byte for byte the original.
4. **Given** a running compression, **When** the user cancels it, **Then** it stops within one batch of blocks and leaves no partial file.
5. **Given** one core, **When** the user compresses a file, **Then** speed does not drop by more than 5%.

### User Story 2 - Tile compression is usable on the CPU (Priority: P2)

A user preparing an archive for GPU decompression gets it at least ten times faster, with the same output.

**Why this priority**: Fewer people use this path, but the gain is large and the change is contained.

**Independent Test**: Compress a large file through the tile engine and compare time and output bytes with today's build.

**Acceptance Scenarios**:

1. **Given** a large file, **When** it is compressed through the tile engine on all cores, **Then** throughput meets SC-003.
2. **Given** the same input, **When** it is compressed with any number of cores, **Then** the output bytes are identical to today's.
3. **Given** a running compression, **When** the user cancels it, **Then** it reports cancellation.

### Edge Cases

- Empty input, and input smaller than one block or tile.
- Incompressible blocks, which are stored.
- The last block or tile being short.
- Every match length and distance the format allows.

## Requirements

### Functional Requirements

- **FR-001**: Fast compression MUST use all available cores for inputs of more than one block.
- **FR-002**: The fast archive format MUST NOT change.
- **FR-003**: Output bytes MUST NOT depend on the number of cores, and MUST match the previous release for the same input.
- **FR-004**: Fast compression to a stream MUST keep memory bounded by a fixed number of blocks, not the input size.
- **FR-005**: Cancellation MUST remain effective.
- **FR-006**: Tile compression MUST use all available cores.
- **FR-007**: Tile archive bytes MUST NOT change.
- **FR-008**: No `unsafe` code and no new third-party dependency may be added.

### Key Entities

- **Block**: an independent unit of input, compressed without reference to other blocks.
- **Tile**: an independent 64 KB unit of input to the tile engine.

## Success Criteria

Reference machine and corpus: i9-12900K, 128 MiB `real`.

- **SC-001**: Fast compression in memory reaches at least 5,000 MB/s at all cores (today about 717 MB/s).
- **SC-002**: Fast compression through the command line beats live `lz4 -1` at 4 cores and at all cores, on the same run.
- **SC-003**: Tile compression is at least 10x faster at all cores (today about 47 MB/s).
- **SC-004**: Single-core speed of both paths does not regress by more than 5%.
- **SC-005**: All output bytes match the previous build, for every core count tested.
- **SC-006**: No other plugin slows by more than 5%, and all existing archives decompress byte for byte.

## Assumptions

- Parallel decompression of fast archives is a separate feature.
- The tile match finder stays as it is, so tile ratio is unchanged.
- Work that measurement rejected stays out: worker buffer recycling, parallel outer CRC, mapped compress output.
- The fast plugin's output path may hold one batch of blocks in memory.
