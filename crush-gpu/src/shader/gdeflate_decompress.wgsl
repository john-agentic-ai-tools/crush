// GDeflate GPU Decompression Shader
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
    payload_size: u32,
    uncompressed_size: u32,
    _pad0: u32,
    _pad1: u32,
};

@group(0) @binding(0) var<storage, read>       tile_meta:  GDeflateMeta;
@group(0) @binding(1) var<storage, read>       compressed: array<u32>;
@group(0) @binding(2) var<storage, read_write> output:     array<u32>;

// Shared memory for inter-thread symbol communication
var<workgroup> g_sym:     array<u32, 32>;  // decoded symbols (0..285 or 0xFFFF)
var<workgroup> g_len:     array<u32, 32>;  // match lengths (0 for literals/EOB)
var<workgroup> g_dist:    array<u32, 32>;  // match distances (0 for literals/EOB)
var<workgroup> g_out_pos: u32;             // current output byte position
var<workgroup> g_done:    u32;             // 1 when EOB reached or output full

// ---------------------------------------------------------------------------
// Lookup tables for DEFLATE length/distance extra bits
// ---------------------------------------------------------------------------

const LENGTH_BASE = array<u32, 29>(
    3u, 4u, 5u, 6u, 7u, 8u, 9u, 10u,
    11u, 13u, 15u, 17u, 19u, 23u, 27u, 31u,
    35u, 43u, 51u, 59u, 67u, 83u, 99u, 115u,
    131u, 163u, 195u, 227u, 258u
);

const LENGTH_EXTRA = array<u32, 29>(
    0u, 0u, 0u, 0u, 0u, 0u, 0u, 0u,
    1u, 1u, 1u, 1u, 2u, 2u, 2u, 2u,
    3u, 3u, 3u, 3u, 4u, 4u, 4u, 4u,
    5u, 5u, 5u, 5u, 0u
);

const DIST_BASE = array<u32, 30>(
    1u, 2u, 3u, 4u, 5u, 7u, 9u, 13u,
    17u, 25u, 33u, 49u, 65u, 97u, 129u, 193u,
    257u, 385u, 513u, 769u, 1025u, 1537u, 2049u, 3073u,
    4097u, 6145u, 8193u, 12289u, 16385u, 24577u
);

const DIST_EXTRA = array<u32, 30>(
    0u, 0u, 0u, 0u, 1u, 1u, 2u, 2u,
    3u, 3u, 4u, 4u, 5u, 5u, 6u, 6u,
    7u, 7u, 8u, 8u, 9u, 9u, 10u, 10u,
    11u, 11u, 12u, 12u, 13u, 13u
);

// ---------------------------------------------------------------------------
// Sub-stream word addressing
// ---------------------------------------------------------------------------

// Compute the u32 index in `compressed` for word `w` of stream `s`.
//   w==0: initial state, index = s        (bytes [s*4 .. s*4+3])
//   w>=1: interleaved,   index = 32 + (w-1)*32 + s
fn stream_word_index(s: u32, w: u32) -> u32 {
    if (w == 0u) {
        return s;
    }
    return 32u + (w - 1u) * 32u + s;
}

// ---------------------------------------------------------------------------
// BitReader — per-thread, function-scope state
// ---------------------------------------------------------------------------

// Read one bit from a stream. Returns 0 or 1.
// If past end of data, returns 0 (which decodes as EOB in fixed Huffman).
fn read_bit(
    word_pos: ptr<function, u32>,
    bit_pos:  ptr<function, u32>,
    cur_word: ptr<function, u32>,
    stream_id: u32
) -> u32 {
    if (*bit_pos >= 32u) {
        *word_pos += 1u;
        let idx = stream_word_index(stream_id, *word_pos);
        let max_words = tile_meta.payload_size / 4u;
        if (idx < max_words) {
            *cur_word = compressed[idx];
        } else {
            *cur_word = 0u; // past end → zero bits → EOB
        }
        *bit_pos = 0u;
    }
    let bit = (*cur_word >> *bit_pos) & 1u;
    *bit_pos += 1u;
    return bit;
}

// Read n bits LSB-first (for extra bits in DEFLATE length/distance encoding).
fn read_bits_lsb(
    word_pos: ptr<function, u32>,
    bit_pos:  ptr<function, u32>,
    cur_word: ptr<function, u32>,
    stream_id: u32,
    n: u32
) -> u32 {
    var result = 0u;
    for (var i = 0u; i < n; i++) {
        let b = read_bit(word_pos, bit_pos, cur_word, stream_id);
        result |= b << i;
    }
    return result;
}

// ---------------------------------------------------------------------------
// Fixed Huffman decoders
// ---------------------------------------------------------------------------

// Decode one literal/length symbol using fixed DEFLATE Huffman codes.
//
// Fixed code assignment (RFC 1951 §3.2.6):
//   7-bit codes  0..23    → symbols 256..279  (EOB + length codes)
//   8-bit codes  48..191  → symbols 0..143    (literal bytes)
//   8-bit codes  192..197 → symbols 280..285  (length codes)
//   9-bit codes  396..507 → symbols 144..255  (literal bytes)
//
// Returns symbol 0..285, or 0xFFFF on error.
fn decode_litlen(
    word_pos: ptr<function, u32>,
    bit_pos:  ptr<function, u32>,
    cur_word: ptr<function, u32>,
    stream_id: u32
) -> u32 {
    var code = 0u;

    // Read 7 bits (MSB-first: first bit read = MSB of code)
    for (var i = 0u; i < 7u; i++) {
        let b = read_bit(word_pos, bit_pos, cur_word, stream_id);
        code = (code << 1u) | b;
    }
    if (code <= 23u) {
        return 256u + code;
    }

    // Read 8th bit
    let b8 = read_bit(word_pos, bit_pos, cur_word, stream_id);
    code = (code << 1u) | b8;
    if (code >= 48u && code <= 191u) {
        return code - 48u;
    }
    if (code >= 192u && code <= 197u) {
        return 280u + code - 192u;
    }

    // Read 9th bit
    let b9 = read_bit(word_pos, bit_pos, cur_word, stream_id);
    code = (code << 1u) | b9;
    if (code >= 396u && code <= 507u) {
        return 144u + code - 396u;
    }

    return 0xFFFFu; // Invalid code
}

// Decode one distance symbol using fixed DEFLATE Huffman codes.
// All 30 distance codes are 5-bit, values 0..29.
fn decode_dist_code(
    word_pos: ptr<function, u32>,
    bit_pos:  ptr<function, u32>,
    cur_word: ptr<function, u32>,
    stream_id: u32
) -> u32 {
    var code = 0u;
    for (var i = 0u; i < 5u; i++) {
        let b = read_bit(word_pos, bit_pos, cur_word, stream_id);
        code = (code << 1u) | b;
    }
    return code;
}

// ---------------------------------------------------------------------------
// Output buffer byte access
// ---------------------------------------------------------------------------

fn write_output_byte(pos: u32, val: u32) {
    let word_idx = pos / 4u;
    let shift = (pos % 4u) * 8u;
    let mask = 0xFFu << shift;
    output[word_idx] = (output[word_idx] & ~mask) | ((val & 0xFFu) << shift);
}

fn read_output_byte(pos: u32) -> u32 {
    let word_idx = pos / 4u;
    let shift = (pos % 4u) * 8u;
    return (output[word_idx] >> shift) & 0xFFu;
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

@compute @workgroup_size(32, 1, 1)
fn main(@builtin(local_invocation_id) local_id: vec3<u32>) {
    let tid = local_id.x;
    let uncompressed_size = tile_meta.uncompressed_size;

    // Initialize this thread's BitReader state with its sub-stream's first word.
    var wp = 0u;
    var bp = 0u;
    var cw = compressed[stream_word_index(tid, 0u)];

    // Thread 0 initializes shared state and reads the block header.
    if (tid == 0u) {
        g_out_pos = 0u;
        g_done = 0u;

        // Read BFINAL (1 bit, LSB-first) — we don't use it, just consume.
        _ = read_bit(&wp, &bp, &cw, 0u);
        // Read BTYPE (2 bits, LSB-first).
        let btype = read_bits_lsb(&wp, &bp, &cw, 0u, 2u);
        if (btype != 1u) {
            // Only fixed Huffman is supported.
            g_done = 1u;
        }
    }

    workgroupBarrier();

    if (g_done != 0u) {
        return;
    }

    // Main decode loop: each iteration decodes 32 symbols (one per thread).
    loop {
        // --- Phase 1: Parallel Huffman decode ---
        // Each thread decodes one literal/length symbol from its sub-stream.
        let sym = decode_litlen(&wp, &bp, &cw, tid);

        var match_len  = 0u;
        var match_dist = 0u;

        // If this is a length code (257..285), also decode extra + distance.
        if (sym >= 257u && sym <= 285u) {
            let len_idx   = sym - 257u;
            let base_len  = LENGTH_BASE[len_idx];
            let extra_cnt = LENGTH_EXTRA[len_idx];
            var extra_val = 0u;
            if (extra_cnt > 0u) {
                extra_val = read_bits_lsb(&wp, &bp, &cw, tid, extra_cnt);
            }
            match_len = base_len + extra_val;

            // Distance code (5-bit fixed Huffman) + extra bits
            let dc = decode_dist_code(&wp, &bp, &cw, tid);
            if (dc < 30u) {
                let base_d    = DIST_BASE[dc];
                let d_extra_n = DIST_EXTRA[dc];
                var d_extra_v = 0u;
                if (d_extra_n > 0u) {
                    d_extra_v = read_bits_lsb(&wp, &bp, &cw, tid, d_extra_n);
                }
                match_dist = base_d + d_extra_v;
            }
        }

        // Store results to shared memory.
        g_sym[tid]  = sym;
        g_len[tid]  = match_len;
        g_dist[tid] = match_dist;

        workgroupBarrier();

        // --- Phase 2: Sequential output (thread 0 only) ---
        if (tid == 0u) {
            for (var i = 0u; i < 32u; i++) {
                if (g_done != 0u) {
                    break;
                }

                let s = g_sym[i];

                if (s < 256u) {
                    // Literal byte
                    if (g_out_pos < uncompressed_size) {
                        write_output_byte(g_out_pos, s);
                        g_out_pos += 1u;
                    }
                } else if (s == 256u) {
                    // End of block
                    g_done = 1u;
                } else if (s <= 285u) {
                    // LZ77 match: copy `length` bytes from `distance` back
                    let length   = g_len[i];
                    let distance = g_dist[i];
                    if (distance > 0u && distance <= g_out_pos) {
                        let src = g_out_pos - distance;
                        for (var j = 0u; j < length; j++) {
                            if (g_out_pos < uncompressed_size) {
                                let bv = read_output_byte(src + j);
                                write_output_byte(g_out_pos, bv);
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

        workgroupBarrier();

        if (g_done != 0u) {
            break;
        }
    }
}
