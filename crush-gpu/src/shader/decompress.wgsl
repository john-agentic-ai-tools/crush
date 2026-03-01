// GPU decompression compute shader for crush-gpu
//
// Each workgroup processes one tile.  Each thread in the workgroup (32 threads)
// decodes one LZ77-encoded sub-stream.
//
// Tile payload layout (in `compressed_data`):
//   [32 × u32 LE]  offset table  (offsets relative to sub-stream data start)
//   [sub-stream 0 compressed bytes]
//   [sub-stream 1 compressed bytes]
//   ...
//   [sub-stream 31 compressed bytes]
//
// LZ77 token format:
//   0x00 <byte>                       — literal
//   0x01 <length: u16 LE> <dist: u16 LE> — match (copy from output history)
//   0xFF                              — end of stream
//
// After each thread decodes its sub-stream into `decompressed_data`, the host
// reads back the per-sub-stream buffers and de-interleaves on CPU.

// Bind group 0: tile metadata
struct TileMeta {
    compressed_offset: u32,    // byte offset into compressed_data for this tile
    compressed_size: u32,      // compressed payload size in bytes
    uncompressed_size: u32,    // expected uncompressed tile size
    sub_stream_count: u32,     // number of sub-streams (typically 32)
    output_offset: u32,        // byte offset into decompressed_data for this tile
    tile_index: u32,           // which tile we are processing
    _pad0: u32,
    _pad1: u32,
};

@group(0) @binding(0) var<storage, read>       tile_meta:         TileMeta;
@group(0) @binding(1) var<storage, read>       compressed_data:   array<u32>;
@group(0) @binding(2) var<storage, read_write> decompressed_data: array<u32>;
// Per-sub-stream output length (one u32 per sub-stream so host knows how many bytes)
@group(0) @binding(3) var<storage, read_write> sub_stream_lengths: array<u32>;

// Helper: read a single byte from the compressed_data buffer at byte offset `byte_off`.
fn read_byte(byte_off: u32) -> u32 {
    let word_idx = byte_off / 4u;
    let shift = (byte_off % 4u) * 8u;
    return (compressed_data[word_idx] >> shift) & 0xFFu;
}

// Helper: read a u16 LE from compressed_data at byte offset `byte_off`.
fn read_u16(byte_off: u32) -> u32 {
    let lo = read_byte(byte_off);
    let hi = read_byte(byte_off + 1u);
    return lo | (hi << 8u);
}

// Helper: read a u32 LE from compressed_data at byte offset `byte_off`.
fn read_u32_le(byte_off: u32) -> u32 {
    let b0 = read_byte(byte_off);
    let b1 = read_byte(byte_off + 1u);
    let b2 = read_byte(byte_off + 2u);
    let b3 = read_byte(byte_off + 3u);
    return b0 | (b1 << 8u) | (b2 << 16u) | (b3 << 24u);
}

// Helper: write a single byte to decompressed_data at byte offset `byte_off`.
fn write_byte(byte_off: u32, value: u32) {
    let word_idx = byte_off / 4u;
    let shift = (byte_off % 4u) * 8u;
    let mask = 0xFFu << shift;
    let old = decompressed_data[word_idx];
    // Atomic not needed because each sub-stream writes to a disjoint region.
    decompressed_data[word_idx] = (old & ~mask) | ((value & 0xFFu) << shift);
}

// Helper: read a byte we previously wrote to decompressed_data.
fn read_output_byte(byte_off: u32) -> u32 {
    let word_idx = byte_off / 4u;
    let shift = (byte_off % 4u) * 8u;
    return (decompressed_data[word_idx] >> shift) & 0xFFu;
}

// Each sub-stream gets up to this many bytes of output space.
// 64KB tile / 32 sub-streams = 2048 bytes max per sub-stream, but the
// output region is sized by the host to accommodate uncompressed_size.
// We use per-sub-stream output offsets to write into non-overlapping regions.

const TOKEN_LITERAL: u32 = 0x00u;
const TOKEN_MATCH:   u32 = 0x01u;
const TOKEN_END:     u32 = 0xFFu;

@compute @workgroup_size(32, 1, 1)
fn main(@builtin(local_invocation_id) local_id: vec3<u32>,
        @builtin(workgroup_id)        wg_id:    vec3<u32>) {
    let thread_id = local_id.x;
    let n = tile_meta.sub_stream_count;

    // If this thread is beyond the sub-stream count, do nothing.
    if (thread_id >= n) {
        return;
    }

    let tile_compressed_base = tile_meta.compressed_offset;
    let offset_table_size = n * 4u;

    // Read this sub-stream's start offset from the offset table.
    let ss_offset = read_u32_le(tile_compressed_base + thread_id * 4u);

    // Determine the end of this sub-stream's data.
    var ss_end: u32;
    if (thread_id + 1u < n) {
        ss_end = read_u32_le(tile_compressed_base + (thread_id + 1u) * 4u);
    } else {
        ss_end = tile_meta.compressed_size - offset_table_size;
    }

    // Absolute byte position of this sub-stream's compressed data.
    let ss_data_base = tile_compressed_base + offset_table_size + ss_offset;
    let ss_data_end  = tile_compressed_base + offset_table_size + ss_end;

    // Output region: each sub-stream gets a separate slice.
    // Max bytes per sub-stream = ceil(uncompressed_size / n).
    let max_per_ss = (tile_meta.uncompressed_size + n - 1u) / n;
    let out_base = tile_meta.output_offset + thread_id * max_per_ss;

    var read_pos = ss_data_base;
    var write_pos = 0u;

    // Decode LZ77 tokens.
    loop {
        if (read_pos >= ss_data_end) {
            break;
        }

        let token = read_byte(read_pos);
        read_pos = read_pos + 1u;

        if (token == TOKEN_END) {
            break;
        } else if (token == TOKEN_LITERAL) {
            if (read_pos >= ss_data_end) {
                break;
            }
            let byte_val = read_byte(read_pos);
            read_pos = read_pos + 1u;
            write_byte(out_base + write_pos, byte_val);
            write_pos = write_pos + 1u;
        } else if (token == TOKEN_MATCH) {
            if (read_pos + 3u >= ss_data_end) {
                break;
            }
            let length   = read_u16(read_pos);
            let distance = read_u16(read_pos + 2u);
            read_pos = read_pos + 4u;

            // Guard against invalid distance (underflow or zero) from corrupt data.
            if (distance == 0u || distance > write_pos) {
                break;
            }
            let copy_start = write_pos - distance;
            for (var i = 0u; i < length; i = i + 1u) {
                let src_byte = read_output_byte(out_base + copy_start + i);
                write_byte(out_base + write_pos, src_byte);
                write_pos = write_pos + 1u;
            }
        }
        // Unknown tokens are silently skipped (defensive).
    }

    // Store the actual number of bytes written by this sub-stream.
    // Each tile is dispatched independently with its own buffer, so index by thread_id only.
    sub_stream_lengths[thread_id] = write_pos;
}
