# Data Model: Hybrid High-Speed Codec

Byte layouts are in [contracts/sprint-format.md](contracts/sprint-format.md). This file covers entities, relationships and validation.

## Entities

### Container (CRSH v2)

- **Fields**: file header (format version 2, codec id, block size, sizes, block count), blocks, index, footer.
- **Relationships**: Wrapped by Crush's outer header. Holds 1..N blocks and one index.
- **Validation**:
  - Format version must be 1 or 2. Codec id must be known, and v1 implies DEFLATE.
  - The sum of index uncompressed sizes must equal the header total and pass the expansion limit before any allocation.
  - Each block offset and size must lie inside the file.

### Block

- **Fields**: compressed size, uncompressed size, CRC32, flags (stored).
- **Relationships**: Owned by the container, and has one index entry. Holds 1..N segments. Has no dependency on any other block.
- **Validation**:
  - Uncompressed size must be at most the header block size.
  - A stored block's payload length must equal its uncompressed size.
  - Segment sizes must sum exactly to the block's uncompressed size.

### Segment

- **Fields**: type (raw, RLE or compressed), regenerated size, compressed size.
- **Relationships**: Sits inside a block in order. Holds a literals section and a sequences section when compressed.
- **Validation**:
  - Regenerated size must be at most 128 KiB.
  - Section lengths must fit in the remaining payload.
  - Trailing bytes after the last segment are an error.

### Literals Section

- **Fields**: mode (raw, RLE, explicit Huffman, repeat Huffman), regenerated size, four stream sizes when Huffman coded.
- **Validation**:
  - Stream sizes must fit the section and each must be non-empty when used.
  - Code lengths must form a complete code with a maximum of 11 bits.
  - Repeat mode with no earlier table in the block is an error.

### Sequences Section

- **Fields**: sequence count, a mode per field (predefined, RLE, explicit, repeat) for literal length, match length and offset.
- **Validation**:
  - Table logs must stay within fixed maximums. Normalized counts must sum to the table size.
  - Repeat mode with no earlier table in the block is an error.
  - Bitstreams must end exactly at their initial position.
  - Literal lengths must not exceed the remaining literals.
  - Match offsets must not exceed bytes already produced in the block, and must be non-zero.
  - Output must never exceed the segment's regenerated size.

### Block Entropy State (per block, in memory)

- **Fields**: current Huffman table, three tANS tables, three repeat offsets.
- **State transitions**:
  - `Empty` at block start → `Set` when a table is first declared → `Replaced` by each later explicit table.
  - Repeat offsets begin at the fixed starting values at each block, and update after each sequence.
  - Any reference to an `Empty` table is corruption.

### Worker State

- **Fields**: match tables, sequence buffers, scratch output, histogram and table workspace.
- **Relationships**: One per rayon worker through `map_init`. Reused across blocks, with match tables reset at each block. Counts toward the 32 MB limit.

### Gate Result

- **Fields**: metric, corpus, core set, Crush and competitor values, multiple, spread, build id, machine, competitor version, verdict.
- **Validation**: A win under 10% needs results from several separate builds (SC-011).

### Unsafe Exception Record

- **Fields**: location, gate missed, safe alternatives tried with numbers, gain measured with unsafe code, safety conditions, fuzz target, memory-check result.
- **Location**: `specs/026-hybrid-speed-codec/unsafe-exceptions.md`, created only if an exception is requested.
- **State transitions**: `Proposed` → `Approved` by the maintainer → `Implemented` → `Verified`.
