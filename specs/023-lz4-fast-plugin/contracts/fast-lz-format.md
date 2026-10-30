# Contract: `fast-lz` compressed format, v1

Applies to the payload inside the Crush outer format (outer magic `43 52 01 04`). All integers little-endian.

```
payload    := frame_header block* end_marker
frame_header (8 bytes):
  0..2  "LZ"
  2     version = 1
  3     flags   = 0
  4     block_size_log2 ∈ [16, 20]
  5..8  reserved = 0
block:
  0..4  u32 c = compressed_len (bits 0..30) | stored (bit 31)
  4..8  u32 u = uncompressed_len
  8..   payload[compressed_len]
end_marker := u32 0
```

Block payload when not stored — a sequence list:

```
sequence := token [lit_ext] literals [offset match_ext]
token    := (lit_len_nibble << 4) | match_len_nibble
lit_ext  := 255* byte(<255)        present iff lit_len_nibble = 15
offset   := u16, 1 ≤ offset ≤ produced_so_far
match_ext:= 255* byte(<255)        present iff match_len_nibble = 15
match_len = 4 + match_len_nibble (+ ext)
```

The final sequence of a block carries literals only and ends exactly at the end of the payload.

## Decoder obligations (FR-005)

The decoder MUST return `ValidationError::CorruptedData` (never panic, hang, or allocate more than declared) when:
- frame magic, version, flags, block_size_log2 or reserved bytes are invalid;
- a block's `uncompressed_len` is 0 or exceeds `2^block_size_log2`, or `compressed_len` exceeds the compress bound of `uncompressed_len`;
- a stored block's `compressed_len ≠ uncompressed_len`;
- a length extension runs past the payload, literals run past the payload, an offset is 0 or larger than the bytes produced in this block, or a match would exceed `uncompressed_len`;
- a block decodes to a length other than `uncompressed_len`;
- the running total would exceed the outer header's original size, the end marker is missing, or bytes follow it.

Allocation: at most `uncompressed_len` per block, and at most the outer original size in total.

## Encoder obligations

- Output decodes to the input exactly (FR-002), for every input including empty (payload = header + end marker).
- A block whose compressed form is not smaller than its input is emitted stored.
- Compress bound: `8 + Σ blocks (8 + u + u/255 + 16) + 4`; callers may rely on it for buffer sizing.

## Compatibility

v1 is new in this feature. Readers reject other versions. The format may change until the first release that ships it (constitution: compatibility only at release boundaries).
