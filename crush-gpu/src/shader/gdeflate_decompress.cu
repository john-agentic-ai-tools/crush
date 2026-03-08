// GDeflate GPU Decompression CUDA kernel for crush-gpu
//
// Port of gdeflate_decompress.wgsl to CUDA C for nvrtc runtime compilation.
//
// Decompresses a single GDeflate-encoded tile using 32 cooperative threads.
// Each thread reads its own sub-stream (Huffman decode in parallel).
// Thread 0 coordinates sequential output writing (required for LZ back-refs).
//
// GDeflate payload layout in `compressed`:
//   [128 bytes] initial u32 state per stream (32 × 4 bytes)
//   [variable]  interleaved u32 words: stream0[1], stream1[1], ..., stream31[1],
//               stream0[2], stream1[2], ..., etc.
//
// Fixed Huffman only (BTYPE=01). Block header on stream 0.

struct GDeflateMeta {
    unsigned int payload_size;
    unsigned int uncompressed_size;
    unsigned int _pad0;
    unsigned int _pad1;
};

// -----------------------------------------------------------------------
// Lookup tables for DEFLATE length/distance extra bits
// -----------------------------------------------------------------------

__constant__ unsigned int LENGTH_BASE[29] = {
    3, 4, 5, 6, 7, 8, 9, 10,
    11, 13, 15, 17, 19, 23, 27, 31,
    35, 43, 51, 59, 67, 83, 99, 115,
    131, 163, 195, 227, 258
};

__constant__ unsigned int LENGTH_EXTRA[29] = {
    0, 0, 0, 0, 0, 0, 0, 0,
    1, 1, 1, 1, 2, 2, 2, 2,
    3, 3, 3, 3, 4, 4, 4, 4,
    5, 5, 5, 5, 0
};

__constant__ unsigned int DIST_BASE[30] = {
    1, 2, 3, 4, 5, 7, 9, 13,
    17, 25, 33, 49, 65, 97, 129, 193,
    257, 385, 513, 769, 1025, 1537, 2049, 3073,
    4097, 6145, 8193, 12289, 16385, 24577
};

__constant__ unsigned int DIST_EXTRA[30] = {
    0, 0, 0, 0, 1, 1, 2, 2,
    3, 3, 4, 4, 5, 5, 6, 6,
    7, 7, 8, 8, 9, 9, 10, 10,
    11, 11, 12, 12, 13, 13
};

// -----------------------------------------------------------------------
// Sub-stream word addressing
// -----------------------------------------------------------------------

// Compute the u32 index in `compressed` for word `w` of stream `s`.
//   w==0: initial state, index = s        (bytes [s*4 .. s*4+3])
//   w>=1: interleaved,   index = 32 + (w-1)*32 + s
__device__ unsigned int stream_word_index(unsigned int s, unsigned int w) {
    if (w == 0u) {
        return s;
    }
    return 32u + (w - 1u) * 32u + s;
}

// -----------------------------------------------------------------------
// BitReader — per-thread state passed by pointer
// -----------------------------------------------------------------------

// Read one bit from a stream. Returns 0 or 1.
// If past end of data, returns 0 (decodes as EOB in fixed Huffman).
__device__ unsigned int read_bit(
    unsigned int* word_pos,
    unsigned int* bit_pos,
    unsigned int* cur_word,
    unsigned int stream_id,
    const unsigned int* compressed,
    unsigned int max_words
) {
    if (*bit_pos >= 32u) {
        *word_pos += 1u;
        unsigned int idx = stream_word_index(stream_id, *word_pos);
        if (idx < max_words) {
            *cur_word = compressed[idx];
        } else {
            *cur_word = 0u; // past end -> zero bits -> EOB
        }
        *bit_pos = 0u;
    }
    unsigned int bit = (*cur_word >> *bit_pos) & 1u;
    *bit_pos += 1u;
    return bit;
}

// Read n bits LSB-first (for extra bits in DEFLATE length/distance encoding).
__device__ unsigned int read_bits_lsb(
    unsigned int* word_pos,
    unsigned int* bit_pos,
    unsigned int* cur_word,
    unsigned int stream_id,
    unsigned int n,
    const unsigned int* compressed,
    unsigned int max_words
) {
    unsigned int result = 0u;
    for (unsigned int i = 0u; i < n; i++) {
        unsigned int b = read_bit(word_pos, bit_pos, cur_word,
                                  stream_id, compressed, max_words);
        result |= b << i;
    }
    return result;
}

// -----------------------------------------------------------------------
// Fixed Huffman decoders
// -----------------------------------------------------------------------

// Decode one literal/length symbol using fixed DEFLATE Huffman codes.
//
// Fixed code assignment (RFC 1951 §3.2.6):
//   7-bit codes  0..23    -> symbols 256..279  (EOB + length codes)
//   8-bit codes  48..191  -> symbols 0..143    (literal bytes)
//   8-bit codes  192..197 -> symbols 280..285  (length codes)
//   9-bit codes  396..507 -> symbols 144..255  (literal bytes)
//
// Returns symbol 0..285, or 0xFFFF on error.
__device__ unsigned int decode_litlen(
    unsigned int* word_pos,
    unsigned int* bit_pos,
    unsigned int* cur_word,
    unsigned int stream_id,
    const unsigned int* compressed,
    unsigned int max_words
) {
    unsigned int code = 0u;

    // Read 7 bits (MSB-first: first bit read = MSB of code)
    for (unsigned int i = 0u; i < 7u; i++) {
        unsigned int b = read_bit(word_pos, bit_pos, cur_word,
                                  stream_id, compressed, max_words);
        code = (code << 1u) | b;
    }
    if (code <= 23u) {
        return 256u + code;
    }

    // Read 8th bit
    unsigned int b8 = read_bit(word_pos, bit_pos, cur_word,
                               stream_id, compressed, max_words);
    code = (code << 1u) | b8;
    if (code >= 48u && code <= 191u) {
        return code - 48u;
    }
    if (code >= 192u && code <= 197u) {
        return 280u + code - 192u;
    }

    // Read 9th bit
    unsigned int b9 = read_bit(word_pos, bit_pos, cur_word,
                               stream_id, compressed, max_words);
    code = (code << 1u) | b9;
    if (code >= 396u && code <= 507u) {
        return 144u + code - 396u;
    }

    return 0xFFFFu; // Invalid code
}

// Decode one distance symbol using fixed DEFLATE Huffman codes.
// All 30 distance codes are 5-bit, values 0..29.
__device__ unsigned int decode_dist_code(
    unsigned int* word_pos,
    unsigned int* bit_pos,
    unsigned int* cur_word,
    unsigned int stream_id,
    const unsigned int* compressed,
    unsigned int max_words
) {
    unsigned int code = 0u;
    for (unsigned int i = 0u; i < 5u; i++) {
        unsigned int b = read_bit(word_pos, bit_pos, cur_word,
                                  stream_id, compressed, max_words);
        code = (code << 1u) | b;
    }
    return code;
}

// -----------------------------------------------------------------------
// Output buffer byte access
// -----------------------------------------------------------------------

__device__ void write_output_byte(unsigned int* output_buf,
                                  unsigned int pos, unsigned int val) {
    unsigned int word_idx = pos / 4u;
    unsigned int shift = (pos % 4u) * 8u;
    unsigned int mask = 0xFFu << shift;
    output_buf[word_idx] = (output_buf[word_idx] & ~mask) |
                           ((val & 0xFFu) << shift);
}

__device__ unsigned int read_output_byte(const unsigned int* output_buf,
                                         unsigned int pos) {
    unsigned int word_idx = pos / 4u;
    unsigned int shift = (pos % 4u) * 8u;
    return (output_buf[word_idx] >> shift) & 0xFFu;
}

// -----------------------------------------------------------------------
// Shared memory for inter-thread symbol communication
// -----------------------------------------------------------------------

__shared__ unsigned int g_sym[32];
__shared__ unsigned int g_len[32];
__shared__ unsigned int g_dist[32];
__shared__ unsigned int g_out_pos;
__shared__ unsigned int g_done;

// -----------------------------------------------------------------------
// Entry point
// -----------------------------------------------------------------------

extern "C" __global__ void gdeflate_decompress_tile(
    const GDeflateMeta* __restrict__ tile_meta,
    const unsigned int* __restrict__ compressed,
    unsigned int* __restrict__ output_buf
) {
    unsigned int tid = threadIdx.x;
    unsigned int uncompressed_size = tile_meta->uncompressed_size;
    unsigned int max_words = tile_meta->payload_size / 4u;

    // Initialize this thread's BitReader state with its sub-stream's first word.
    unsigned int wp = 0u;
    unsigned int bp = 0u;
    unsigned int cw = compressed[stream_word_index(tid, 0u)];

    // Thread 0 initializes shared state and reads the block header.
    if (tid == 0u) {
        g_out_pos = 0u;
        g_done = 0u;

        // Read BFINAL (1 bit, LSB-first) — consume but don't use.
        read_bit(&wp, &bp, &cw, 0u, compressed, max_words);
        // Read BTYPE (2 bits, LSB-first).
        unsigned int btype = read_bits_lsb(&wp, &bp, &cw, 0u, 2u,
                                           compressed, max_words);
        if (btype != 1u) {
            // Only fixed Huffman is supported.
            g_done = 1u;
        }
    }

    __syncthreads();

    if (g_done != 0u) {
        return;
    }

    // Main decode loop: each iteration decodes 32 symbols (one per thread).
    while (true) {
        // --- Phase 1: Parallel Huffman decode ---
        unsigned int sym = decode_litlen(&wp, &bp, &cw, tid,
                                         compressed, max_words);

        unsigned int match_len  = 0u;
        unsigned int match_dist = 0u;

        // If this is a length code (257..285), also decode extra + distance.
        if (sym >= 257u && sym <= 285u) {
            unsigned int len_idx   = sym - 257u;
            unsigned int base_len  = LENGTH_BASE[len_idx];
            unsigned int extra_cnt = LENGTH_EXTRA[len_idx];
            unsigned int extra_val = 0u;
            if (extra_cnt > 0u) {
                extra_val = read_bits_lsb(&wp, &bp, &cw, tid, extra_cnt,
                                          compressed, max_words);
            }
            match_len = base_len + extra_val;

            // Distance code (5-bit fixed Huffman) + extra bits
            unsigned int dc = decode_dist_code(&wp, &bp, &cw, tid,
                                               compressed, max_words);
            if (dc < 30u) {
                unsigned int base_d    = DIST_BASE[dc];
                unsigned int d_extra_n = DIST_EXTRA[dc];
                unsigned int d_extra_v = 0u;
                if (d_extra_n > 0u) {
                    d_extra_v = read_bits_lsb(&wp, &bp, &cw, tid, d_extra_n,
                                              compressed, max_words);
                }
                match_dist = base_d + d_extra_v;
            }
        }

        // Store results to shared memory.
        g_sym[tid]  = sym;
        g_len[tid]  = match_len;
        g_dist[tid] = match_dist;

        __syncthreads();

        // --- Phase 2: Sequential output (thread 0 only) ---
        if (tid == 0u) {
            for (unsigned int i = 0u; i < 32u; i++) {
                if (g_done != 0u) {
                    break;
                }

                unsigned int s = g_sym[i];

                if (s < 256u) {
                    // Literal byte
                    if (g_out_pos < uncompressed_size) {
                        write_output_byte(output_buf, g_out_pos, s);
                        g_out_pos += 1u;
                    }
                } else if (s == 256u) {
                    // End of block
                    g_done = 1u;
                } else if (s <= 285u) {
                    // LZ77 match: copy `length` bytes from `distance` back
                    unsigned int length   = g_len[i];
                    unsigned int distance = g_dist[i];
                    if (distance > 0u && distance <= g_out_pos) {
                        unsigned int src = g_out_pos - distance;
                        for (unsigned int j = 0u; j < length; j++) {
                            if (g_out_pos < uncompressed_size) {
                                unsigned int bv = read_output_byte(
                                    output_buf, src + j);
                                write_output_byte(output_buf, g_out_pos, bv);
                                g_out_pos += 1u;
                            }
                        }
                    }
                }
                // else: invalid symbol — skip

                if (g_out_pos >= uncompressed_size) {
                    g_done = 1u;
                }
            }
        }

        __syncthreads();

        if (g_done != 0u) {
            break;
        }
    }
}
