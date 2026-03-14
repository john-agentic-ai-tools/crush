// GPU decompression CUDA kernel for crush-gpu (LZ77, v1 format)
//
// Port of decompress.wgsl to CUDA C for nvrtc runtime compilation.
//
// Each block processes one tile.  Each thread in the block (32 threads)
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

struct TileMeta {
    unsigned int compressed_offset;
    unsigned int compressed_size;
    unsigned int uncompressed_size;
    unsigned int sub_stream_count;
    unsigned int output_offset;
    unsigned int tile_index;
    unsigned int _pad0;
    unsigned int _pad1;
};

#define TOKEN_LITERAL 0x00u
#define TOKEN_MATCH   0x01u
#define TOKEN_END     0xFFu

// Helper: read a single byte from the compressed_data buffer at byte offset.
__device__ unsigned int read_byte(const unsigned int* compressed_data,
                                  unsigned int byte_off) {
    unsigned int word_idx = byte_off / 4u;
    unsigned int shift = (byte_off % 4u) * 8u;
    return (compressed_data[word_idx] >> shift) & 0xFFu;
}

// Helper: read a u16 LE from compressed_data at byte offset.
__device__ unsigned int read_u16(const unsigned int* compressed_data,
                                 unsigned int byte_off) {
    unsigned int lo = read_byte(compressed_data, byte_off);
    unsigned int hi = read_byte(compressed_data, byte_off + 1u);
    return lo | (hi << 8u);
}

// Helper: read a u32 LE from compressed_data at byte offset.
__device__ unsigned int read_u32_le(const unsigned int* compressed_data,
                                    unsigned int byte_off) {
    unsigned int b0 = read_byte(compressed_data, byte_off);
    unsigned int b1 = read_byte(compressed_data, byte_off + 1u);
    unsigned int b2 = read_byte(compressed_data, byte_off + 2u);
    unsigned int b3 = read_byte(compressed_data, byte_off + 3u);
    return b0 | (b1 << 8u) | (b2 << 16u) | (b3 << 24u);
}

// Helper: write a single byte to decompressed_data at byte offset.
__device__ void write_byte(unsigned int* decompressed_data,
                           unsigned int byte_off, unsigned int value) {
    unsigned int word_idx = byte_off / 4u;
    unsigned int shift = (byte_off % 4u) * 8u;
    unsigned int mask = 0xFFu << shift;
    unsigned int old = decompressed_data[word_idx];
    // No atomic needed: each sub-stream writes to a disjoint region.
    decompressed_data[word_idx] = (old & ~mask) | ((value & 0xFFu) << shift);
}

// Helper: read a byte previously written to decompressed_data.
__device__ unsigned int read_output_byte(const unsigned int* decompressed_data,
                                         unsigned int byte_off) {
    unsigned int word_idx = byte_off / 4u;
    unsigned int shift = (byte_off % 4u) * 8u;
    return (decompressed_data[word_idx] >> shift) & 0xFFu;
}

extern "C" __global__ void lz77_decompress_tile(
    const TileMeta* __restrict__ tile_meta,
    const unsigned int* __restrict__ compressed_data,
    unsigned int* __restrict__ decompressed_data,
    unsigned int* __restrict__ sub_stream_lengths
) {
    unsigned int thread_id = threadIdx.x;
    unsigned int n = tile_meta->sub_stream_count;

    // If this thread is beyond the sub-stream count, do nothing.
    if (thread_id >= n) {
        return;
    }

    unsigned int tile_compressed_base = tile_meta->compressed_offset;
    unsigned int offset_table_size = n * 4u;

    // Read this sub-stream's start offset from the offset table.
    unsigned int ss_offset = read_u32_le(compressed_data,
                                         tile_compressed_base + thread_id * 4u);

    // Determine the end of this sub-stream's data.
    unsigned int ss_end;
    if (thread_id + 1u < n) {
        ss_end = read_u32_le(compressed_data,
                             tile_compressed_base + (thread_id + 1u) * 4u);
    } else {
        ss_end = tile_meta->compressed_size - offset_table_size;
    }

    // Absolute byte position of this sub-stream's compressed data.
    unsigned int ss_data_base = tile_compressed_base + offset_table_size + ss_offset;
    unsigned int ss_data_end  = tile_compressed_base + offset_table_size + ss_end;

    // Output region: each sub-stream gets a separate slice.
    unsigned int max_per_ss = (tile_meta->uncompressed_size + n - 1u) / n;
    unsigned int out_base = tile_meta->output_offset + thread_id * max_per_ss;

    unsigned int read_pos = ss_data_base;
    unsigned int write_pos = 0u;

    // Decode LZ77 tokens.
    while (read_pos < ss_data_end) {
        unsigned int token = read_byte(compressed_data, read_pos);
        read_pos += 1u;

        if (token == TOKEN_END) {
            break;
        } else if (token == TOKEN_LITERAL) {
            if (read_pos >= ss_data_end) {
                break;
            }
            unsigned int byte_val = read_byte(compressed_data, read_pos);
            read_pos += 1u;
            write_byte(decompressed_data, out_base + write_pos, byte_val);
            write_pos += 1u;
        } else if (token == TOKEN_MATCH) {
            if (read_pos + 3u >= ss_data_end) {
                break;
            }
            unsigned int length   = read_u16(compressed_data, read_pos);
            unsigned int distance = read_u16(compressed_data, read_pos + 2u);
            read_pos += 4u;

            // Guard against invalid distance from corrupt data.
            if (distance == 0u || distance > write_pos) {
                break;
            }
            unsigned int copy_start = write_pos - distance;
            for (unsigned int i = 0u; i < length; i++) {
                unsigned int src_byte = read_output_byte(decompressed_data,
                                                         out_base + copy_start + i);
                write_byte(decompressed_data, out_base + write_pos, src_byte);
                write_pos += 1u;
            }
        }
        // Unknown tokens are silently skipped (defensive).
    }

    // Store the actual number of bytes written by this sub-stream.
    sub_stream_lengths[thread_id] = write_pos;
}
