# Contract: Sprint Format

**Status**: Draft. This format may change until the freeze in the delivery order. Sprint file compatibility is not promised before then.

All integers are little-endian unless stated otherwise.

## Layers

```text
[Crush outer header, magic 43 52 01 05]
[CRSH file header, format_version = 2, byte 22 = codec id 1]
[Block 0: 16-byte block header + payload] ... [Block N]
[Index] [Footer]
```

CRSH v2 differs from v1 only in the version value and in byte 22. A stored block uses the existing stored flag, and its payload is raw.

## Block payload

A compressed block's payload is a version byte (currently 1) followed by segments, in order, until the payload ends.

```text
[u8 sprint_version] [Segment 0] [Segment 1] ...
```

## Segment

```text
u24  header: bits 0-1 type, bits 2-23 size
```

| Type | Meaning | Size field | Body |
|---|---|---|---|
| 0 | Raw | Regenerated size | That many bytes |
| 1 | RLE | Regenerated size | 1 byte |
| 2 | Compressed | Compressed body size | u24 regenerated size, literals section, sequences section |
| 3 | Reserved | | Error |

Regenerated size is at most 131,072 bytes.

## Literals section

```text
u8   bits 0-1 mode (0 raw, 1 RLE, 2 Huffman, 3 repeat Huffman)
u24  regenerated size (at most 131,072)
```

- **Raw**: The literal bytes follow.
- **RLE**: One byte follows.
- **Huffman**: u24 total size, a table description, three u16 stream sizes (the fourth is implied), and four streams. The description is a u8 last symbol, then a 4-bit code length per symbol up to it, low nibble first.
- **Repeat Huffman**: As Huffman, with no table description.

Code lengths are at most 11 bits. Streams are read backward from their final byte, which holds a sentinel bit. Each of the first three streams decodes `ceil(regenerated / 4)` symbols, and the fourth decodes the remainder.

## Sequences section

```text
varint  sequence count (0 means literals only, nothing further)
u8      modes: bits 0-1 literal length, 2-3 offset, 4-5 match length
[tables for explicit or RLE modes, in that order]
[backward bitstream]
```

Modes: 0 predefined, 1 RLE, 2 explicit, 3 repeat.

Literal and match length codes, and the predefined distributions, are those of RFC 8878. Offset value `v` has code `floor(log2(v))` with that many extra bits. Values above 3 carry `offset + 3`. Values 1 to 3 select the first to third repeat offsets, or with a literal length of 0, the second, the third, and the first less one. Maximum table logs: 9 for literal length, 8 for offset, 9 for match length. A count of 0 is an error.

An explicit table is a u8 table log (at least 5), a u8 symbol count, a u8 stream size, and a backward stream of `count + 1` fields, each as wide as the slots still free require.

The bitstream holds the three opening states, then for each sequence the offset, match length and literal length extra bits, then, except after the last, the literal length, match length and offset state bits. States open in the order literal length, offset, match length.

An RLE or predefined table is also the current table for repeat mode.

## State reset

Huffman table, tANS tables and repeat offsets reset at each block start. Repeat offsets start as 1, 4, 8.

## Open items before freeze

- Separate per-field bit readers versus one shared stream (R8 experiment).
- Persistence across segments (R6).
- Predefined distributions tuned on the benchmark corpora, with care not to over-fit `real`.
