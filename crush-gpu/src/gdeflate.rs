//! `GDeflate` compression and decompression codec
//!
//! `GDeflate` reformats DEFLATE bitstreams for 32-way GPU-parallel decompression.
//! Each 64KB tile is independently compressed and its variable-length codes are
//! distributed round-robin across 32 sub-streams.
//!
//! # Compression (CPU)
//!
//! 1. Find LZ77 matches (literals + length/distance pairs)
//! 2. Distribute symbols round-robin across 32 sub-streams
//! 3. Encode each sub-stream with fixed DEFLATE Huffman codes
//! 4. Serialize sub-streams into the `GDeflate` interleaved bitstream format
//!
//! # Decompression (CPU fallback)
//!
//! 1. Read 32 sub-stream initial states (128 bytes)
//! 2. Decode symbols from sub-streams in round-robin order
//! 3. Reconstruct the LZ77 output (literals + back-references)
//!
//! # Format Reference
//!
//! IETF draft: `draft-uralsky-gdeflate-00`
//! Reference implementation: `<https://github.com/microsoft/DirectStorage/tree/main/GDeflate>`

use crush_core::error::{CrushError, PluginError, Result};

/// Number of sub-streams in `GDeflate` (matches GPU warp width).
const NUM_STREAMS: usize = 32;

/// Maximum Huffman code length in DEFLATE.
const MAX_CODE_LEN: usize = 15;

/// Number of literal/length codes (0..285).
const NUM_LITLEN_CODES: usize = 286;

/// Number of distance codes (0..29).
const NUM_DIST_CODES: usize = 30;

/// Maximum match distance for LZ77 (DEFLATE limit).
const MAX_MATCH_DISTANCE: usize = 32768;

/// Minimum match length for LZ77 (DEFLATE spec: 3).
const MIN_MATCH_LEN: usize = 3;

/// Maximum match length for LZ77 (DEFLATE spec: 258).
const MAX_MATCH_LEN: usize = 258;

/// Base lengths for length codes 257..285.
const LENGTH_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];

/// Extra bits for length codes 257..285.
const LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];

/// Base distances for distance codes 0..29.
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];

/// Extra bits for distance codes 0..29.
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

// ---------------------------------------------------------------------------
// Bit-level reader for decoding GDeflate sub-streams
// ---------------------------------------------------------------------------

/// Reads bits from a byte slice in LSB-first order (DEFLATE convention).
struct BitReader<'a> {
    data: &'a [u8],
    byte_pos: usize,
    bit_pos: u8,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            byte_pos: 0,
            bit_pos: 0,
        }
    }

    /// Read `n` bits (up to 16) from the stream, LSB first.
    fn read_bits(&mut self, n: u8) -> Result<u16> {
        let mut result: u16 = 0;
        for i in 0..n {
            if self.byte_pos >= self.data.len() {
                return Err(CrushError::InvalidFormat(
                    "GDeflate: unexpected end of bitstream".to_owned(),
                ));
            }
            let bit = (self.data[self.byte_pos] >> self.bit_pos) & 1;
            result |= u16::from(bit) << i;
            self.bit_pos += 1;
            if self.bit_pos >= 8 {
                self.bit_pos = 0;
                self.byte_pos += 1;
            }
        }
        Ok(result)
    }
}

// ---------------------------------------------------------------------------
// Bit-level writer for building GDeflate sub-streams
// ---------------------------------------------------------------------------

/// Writes bits to a byte vector in LSB-first order.
struct BitWriter {
    data: Vec<u8>,
    current_byte: u8,
    bit_pos: u8,
}

impl BitWriter {
    fn new() -> Self {
        Self {
            data: Vec::new(),
            current_byte: 0,
            bit_pos: 0,
        }
    }

    /// Write `n` bits (up to 16) in LSB-first order.
    fn write_bits(&mut self, value: u16, n: u8) {
        for i in 0..n {
            if (value >> i) & 1 != 0 {
                self.current_byte |= 1 << self.bit_pos;
            }
            self.bit_pos += 1;
            if self.bit_pos >= 8 {
                self.data.push(self.current_byte);
                self.current_byte = 0;
                self.bit_pos = 0;
            }
        }
    }

    /// Write `n` bits in MSB-first order (for Huffman codes).
    ///
    /// DEFLATE Huffman codes are stored MSB-first in the bitstream.
    /// The first bit written is the most-significant bit of the code.
    fn write_bits_msb(&mut self, value: u16, n: u8) {
        for i in (0..n).rev() {
            let bit = (value >> i) & 1;
            if bit != 0 {
                self.current_byte |= 1 << self.bit_pos;
            }
            self.bit_pos += 1;
            if self.bit_pos >= 8 {
                self.data.push(self.current_byte);
                self.current_byte = 0;
                self.bit_pos = 0;
            }
        }
    }

    /// Flush remaining bits (pad with zeros).
    fn flush(&mut self) {
        if self.bit_pos > 0 {
            self.data.push(self.current_byte);
            self.current_byte = 0;
            self.bit_pos = 0;
        }
    }

    /// Pad to 4-byte (u32) alignment.
    fn pad_to_u32(&mut self) {
        self.flush();
        while !self.data.len().is_multiple_of(4) {
            self.data.push(0);
        }
    }
}

// ---------------------------------------------------------------------------
// Huffman table for DEFLATE decoding
// ---------------------------------------------------------------------------

/// A Huffman decoder built from code lengths (canonical Huffman).
struct HuffmanTable {
    min_code: [u16; MAX_CODE_LEN + 1],
    max_code: [i32; MAX_CODE_LEN + 1],
    offsets: [usize; MAX_CODE_LEN + 1],
    symbols: Vec<u16>,
}

impl HuffmanTable {
    /// Build a Huffman table from an array of code lengths.
    fn from_code_lengths(code_lengths: &[u8]) -> Result<Self> {
        let mut bl_count = [0u16; MAX_CODE_LEN + 1];
        for &cl in code_lengths {
            if usize::from(cl) > MAX_CODE_LEN {
                return Err(CrushError::InvalidFormat(format!(
                    "GDeflate: Huffman code length {cl} exceeds max {MAX_CODE_LEN}"
                )));
            }
            bl_count[usize::from(cl)] += 1;
        }
        bl_count[0] = 0;

        let mut next_code = [0u16; MAX_CODE_LEN + 1];
        let mut code: u16 = 0;
        for bits in 1..=MAX_CODE_LEN {
            code = (code + bl_count[bits - 1]) << 1;
            next_code[bits] = code;
        }

        let total_symbols: usize = bl_count.iter().map(|&c| usize::from(c)).sum();
        let mut symbols = vec![0u16; total_symbols];

        let mut min_code = [0u16; MAX_CODE_LEN + 1];
        let mut max_code = [-1i32; MAX_CODE_LEN + 1];
        let mut offsets = [0usize; MAX_CODE_LEN + 1];

        let mut offset = 0;
        for bits in 1..=MAX_CODE_LEN {
            offsets[bits] = offset;
            min_code[bits] = next_code[bits];
            if bl_count[bits] > 0 {
                max_code[bits] = i32::from(next_code[bits] + bl_count[bits] - 1);
            }
            offset += usize::from(bl_count[bits]);
        }

        let mut code_idx = [0usize; MAX_CODE_LEN + 1];
        for (sym, &cl) in code_lengths.iter().enumerate() {
            let len = usize::from(cl);
            if len > 0 {
                let idx = offsets[len] + code_idx[len];
                if idx < symbols.len() {
                    #[allow(clippy::cast_possible_truncation)]
                    {
                        symbols[idx] = sym as u16; // sym < 286, fits in u16
                    }
                }
                code_idx[len] += 1;
            }
        }

        Ok(Self {
            min_code,
            max_code,
            offsets,
            symbols,
        })
    }

    /// Decode one symbol from the bitstream (reads MSB-first Huffman code).
    fn decode(&self, reader: &mut BitReader<'_>) -> Result<u16> {
        let mut code: u16 = 0;
        for bits in 1..=MAX_CODE_LEN {
            let bit = reader.read_bits(1)?;
            code = (code << 1) | bit;
            if i32::from(code) <= self.max_code[bits] {
                let idx = self.offsets[bits] + usize::from(code - self.min_code[bits]);
                if idx < self.symbols.len() {
                    return Ok(self.symbols[idx]);
                }
            }
        }
        Err(CrushError::InvalidFormat(
            "GDeflate: invalid Huffman code".to_owned(),
        ))
    }

    /// Build the fixed literal/length Huffman table (BTYPE=01).
    fn fixed_litlen() -> Result<Self> {
        let mut code_lengths = [0u8; NUM_LITLEN_CODES];
        for cl in &mut code_lengths[0..=143] {
            *cl = 8;
        }
        for cl in &mut code_lengths[144..=255] {
            *cl = 9;
        }
        for cl in &mut code_lengths[256..=279] {
            *cl = 7;
        }
        for cl in &mut code_lengths[280..=285] {
            *cl = 8;
        }
        Self::from_code_lengths(&code_lengths)
    }

    /// Build the fixed distance Huffman table (BTYPE=01).
    fn fixed_dist() -> Result<Self> {
        let code_lengths = [5u8; NUM_DIST_CODES];
        Self::from_code_lengths(&code_lengths)
    }
}

// ---------------------------------------------------------------------------
// LZ77 symbol representation
// ---------------------------------------------------------------------------

/// A symbol in the LZ77 output stream.
#[derive(Debug, Clone)]
enum LzSymbol {
    Literal(u8),
    Match { length: u16, distance: u16 },
    EndOfBlock,
}

// ---------------------------------------------------------------------------
// LZ77 match finder
// ---------------------------------------------------------------------------

/// Hash 3 bytes at the given position for the LZ77 hash table.
fn hash3(data: &[u8], pos: usize) -> usize {
    let b0 = u32::from(data[pos]);
    let b1 = u32::from(data[pos + 1]);
    let b2 = u32::from(data[pos + 2]);
    ((b0 << 10) ^ (b1 << 5) ^ b2) as usize & 0xFFFF
}

/// Find LZ77 matches in the data and return a symbol stream.
///
/// Uses a simple hash-chain approach: 3-byte hash → most recent position.
/// Finds one candidate match per position (greedy, no lazy evaluation).
fn lz77_find_matches(data: &[u8]) -> Vec<LzSymbol> {
    let mut symbols = Vec::with_capacity(data.len());
    let mut pos = 0;

    // Hash table: 16-bit hash → most recent position.
    // We use u32::MAX as "no entry" sentinel.
    let mut hash_table = vec![u32::MAX; 65536];

    while pos < data.len() {
        let mut best_len = 0usize;
        let mut best_dist = 0usize;

        // Try to find a match (need at least 3 bytes remaining for hash)
        if pos + 2 < data.len() {
            let h = hash3(data, pos);
            let prev = hash_table[h] as usize;
            #[allow(clippy::cast_possible_truncation)]
            {
                hash_table[h] = pos as u32; // pos <= 65536, fits in u32
            }

            // Check if the hash entry points to a valid, in-range position
            if prev != u32::MAX as usize {
                let dist = pos - prev;
                if dist > 0 && dist <= MAX_MATCH_DISTANCE {
                    let max_len = std::cmp::min(MAX_MATCH_LEN, data.len() - pos);
                    let mut len = 0;
                    while len < max_len && data[prev + len] == data[pos + len] {
                        len += 1;
                    }
                    if len >= MIN_MATCH_LEN {
                        best_len = len;
                        best_dist = dist;
                    }
                }
            }
        }

        if best_len >= MIN_MATCH_LEN {
            #[allow(clippy::cast_possible_truncation)]
            symbols.push(LzSymbol::Match {
                length: best_len as u16,    // <= 258
                distance: best_dist as u16, // <= 32768
            });
            // Update hash table for positions we skip over
            for j in 1..best_len {
                if pos + j + 2 < data.len() {
                    let h = hash3(data, pos + j);
                    #[allow(clippy::cast_possible_truncation)]
                    {
                        hash_table[h] = (pos + j) as u32; // pos < 65536
                    }
                }
            }
            pos += best_len;
        } else {
            symbols.push(LzSymbol::Literal(data[pos]));
            pos += 1;
        }
    }

    symbols.push(LzSymbol::EndOfBlock);
    symbols
}

// ---------------------------------------------------------------------------
// Fixed Huffman code generation
// ---------------------------------------------------------------------------

/// Fixed literal/length Huffman codes (BTYPE=01).
/// Returns `(codes, lengths)` arrays indexed by symbol.
fn build_fixed_litlen_codes() -> ([u16; NUM_LITLEN_CODES], [u8; NUM_LITLEN_CODES]) {
    let mut codes = [0u16; NUM_LITLEN_CODES];
    let mut lengths = [0u8; NUM_LITLEN_CODES];

    // Code lengths per RFC 1951 section 3.2.6.
    for item in &mut lengths[0..=143] {
        *item = 8;
    }
    for item in &mut lengths[144..=255] {
        *item = 9;
    }
    for item in &mut lengths[256..=279] {
        *item = 7;
    }
    for item in &mut lengths[280..=285] {
        *item = 8;
    }

    // Generate canonical codes.
    let mut bl_count = [0u16; MAX_CODE_LEN + 1];
    for &cl in &lengths {
        bl_count[usize::from(cl)] += 1;
    }

    let mut next_code = [0u16; MAX_CODE_LEN + 1];
    let mut code: u16 = 0;
    for bits in 1..=MAX_CODE_LEN {
        code = (code + bl_count[bits - 1]) << 1;
        next_code[bits] = code;
    }

    for (sym, &cl) in lengths.iter().enumerate() {
        if cl > 0 {
            codes[sym] = next_code[usize::from(cl)];
            next_code[usize::from(cl)] += 1;
        }
    }

    (codes, lengths)
}

/// Fixed distance Huffman codes (BTYPE=01).
fn build_fixed_dist_codes() -> ([u16; NUM_DIST_CODES], [u8; NUM_DIST_CODES]) {
    let mut codes = [0u16; NUM_DIST_CODES];
    let lengths = [5u8; NUM_DIST_CODES];

    // All distance codes are 5 bits, values 0..29.
    for (i, code) in codes.iter_mut().enumerate() {
        #[allow(clippy::cast_possible_truncation)]
        {
            *code = i as u16; // i < 30, fits in u16
        }
    }

    (codes, lengths)
}

/// Encode a match length (3..258) into `(code, extra_value, extra_bits)`.
fn encode_length(length: u16) -> Result<(u16, u16, u8)> {
    for (i, (&base, &extra)) in LENGTH_BASE.iter().zip(LENGTH_EXTRA.iter()).enumerate() {
        let max_len = base + (1 << extra) - 1;
        if length >= base && length <= max_len {
            #[allow(clippy::cast_possible_truncation)]
            let code = 257 + i as u16; // i < 29, fits in u16
            let extra_val = length - base;
            return Ok((code, extra_val, extra));
        }
    }
    Err(CrushError::InvalidFormat(format!(
        "GDeflate: cannot encode length {length}"
    )))
}

/// Encode a match distance (1..32768) into `(code, extra_value, extra_bits)`.
fn encode_distance(distance: u16) -> Result<(u16, u16, u8)> {
    for (i, (&base, &extra)) in DIST_BASE.iter().zip(DIST_EXTRA.iter()).enumerate() {
        let max_dist = base + (1 << extra) - 1;
        if distance >= base && distance <= max_dist {
            let extra_val = distance - base;
            #[allow(clippy::cast_possible_truncation)]
            {
                return Ok((i as u16, extra_val, extra)); // i < 30, fits in u16
            }
        }
    }
    Err(CrushError::InvalidFormat(format!(
        "GDeflate: cannot encode distance {distance}"
    )))
}

// ---------------------------------------------------------------------------
// GDeflate sub-stream serialization
// ---------------------------------------------------------------------------

/// Serialize LZ77 symbols into `GDeflate`'s 32-way interleaved format.
///
/// Output layout:
/// ```text
/// [128 bytes: initial u32 state per stream, little-endian]
/// [variable: interleaved bitstream data, u32-aligned per stream]
/// ```
///
/// Each symbol's Huffman code + extra bits are written to the assigned sub-stream
/// in round-robin order. Length/distance pairs go to the same sub-stream.
fn serialize_gdeflate(symbols: &[LzSymbol]) -> Result<Vec<u8>> {
    let (litlen_codes, litlen_lengths) = build_fixed_litlen_codes();
    let (dist_codes, dist_lengths) = build_fixed_dist_codes();

    // Create 32 sub-stream bit writers.
    let mut streams: Vec<BitWriter> = (0..NUM_STREAMS).map(|_| BitWriter::new()).collect();

    // Write block header to stream 0: BFINAL=1, BTYPE=01 (fixed Huffman)
    streams[0].write_bits(1, 1); // BFINAL
    streams[0].write_bits(1, 2); // BTYPE = 01 (fixed)

    // Distribute symbols round-robin across sub-streams.
    let mut stream_idx = 0;
    for symbol in symbols {
        match symbol {
            LzSymbol::Literal(byte) => {
                let code_idx = usize::from(*byte);
                let code = litlen_codes[code_idx];
                let len = litlen_lengths[code_idx];
                streams[stream_idx].write_bits_msb(code, len);
                stream_idx = (stream_idx + 1) % NUM_STREAMS;
            }
            LzSymbol::Match { length, distance } => {
                // Encode length: find the length code
                let (len_code, len_extra, len_extra_bits) = encode_length(*length)?;
                let code = litlen_codes[usize::from(len_code)];
                let code_len = litlen_lengths[usize::from(len_code)];

                // Write length code + extra to current stream
                streams[stream_idx].write_bits_msb(code, code_len);
                if len_extra_bits > 0 {
                    streams[stream_idx].write_bits(len_extra, len_extra_bits);
                }

                // Distance code + extra also goes to the SAME stream
                let (dist_code, dist_extra, dist_extra_bits) = encode_distance(*distance)?;
                let dc = dist_codes[usize::from(dist_code)];
                let dl = dist_lengths[usize::from(dist_code)];

                streams[stream_idx].write_bits_msb(dc, dl);
                if dist_extra_bits > 0 {
                    streams[stream_idx].write_bits(dist_extra, dist_extra_bits);
                }

                stream_idx = (stream_idx + 1) % NUM_STREAMS;
            }
            LzSymbol::EndOfBlock => {
                // End-of-block symbol (256) goes to current stream
                let code = litlen_codes[256];
                let len = litlen_lengths[256];
                streams[stream_idx].write_bits_msb(code, len);
            }
        }
    }

    // Flush and pad all streams to u32 alignment.
    for s in &mut streams {
        s.pad_to_u32();
    }

    // Build output: 128 bytes of initial state + interleaved stream data.
    let mut output = Vec::new();

    // Initial state: first u32 from each stream (32 x 4 = 128 bytes).
    for s in &streams {
        if s.data.len() >= 4 {
            output.extend_from_slice(&s.data[..4]);
        } else {
            let mut buf = [0u8; 4];
            buf[..s.data.len()].copy_from_slice(&s.data);
            output.extend_from_slice(&buf);
        }
    }

    // Remaining u32 words interleaved round-robin.
    let max_words = streams.iter().map(|s| s.data.len() / 4).max().unwrap_or(0);
    for word_idx in 1..max_words {
        for s in &streams {
            let byte_off = word_idx * 4;
            if byte_off + 4 <= s.data.len() {
                output.extend_from_slice(&s.data[byte_off..byte_off + 4]);
            } else {
                output.extend_from_slice(&[0u8; 4]);
            }
        }
    }

    Ok(output)
}

/// Deserialize `GDeflate` interleaved bitstream back to decompressed bytes.
fn deserialize_gdeflate(payload: &[u8], uncompressed_size: usize) -> Result<Vec<u8>> {
    if payload.len() < 128 {
        return Err(CrushError::InvalidFormat(
            "GDeflate: payload too small for initial state (need 128 bytes)".to_owned(),
        ));
    }

    // De-interleave the payload into 32 sub-streams.
    let mut stream_data: Vec<Vec<u8>> = (0..NUM_STREAMS).map(|_| Vec::new()).collect();

    // First 128 bytes: initial u32 per stream.
    for (i, sd) in stream_data.iter_mut().enumerate() {
        let off = i * 4;
        sd.extend_from_slice(&payload[off..off + 4]);
    }

    // Remaining words are interleaved round-robin across streams.
    let remaining = &payload[128..];
    let word_count = remaining.len() / 4;
    let mut word_idx = 0;
    while word_idx < word_count {
        for sd in &mut stream_data {
            if word_idx < word_count {
                let off = word_idx * 4;
                sd.extend_from_slice(&remaining[off..off + 4]);
                word_idx += 1;
            }
        }
    }

    // Build Huffman tables for fixed codes (BTYPE=01).
    let litlen_table = HuffmanTable::fixed_litlen()?;
    let dist_table = HuffmanTable::fixed_dist()?;

    // Create bit readers for each stream.
    let mut readers: Vec<BitReader<'_>> = stream_data.iter().map(|d| BitReader::new(d)).collect();

    // Read block header from stream 0.
    let _bfinal = readers[0].read_bits(1)?;
    let btype = readers[0].read_bits(2)?;

    if btype != 1 {
        return Err(CrushError::InvalidFormat(format!(
            "GDeflate: expected fixed Huffman (BTYPE=1), got BTYPE={btype}"
        )));
    }

    // Decode symbols round-robin from sub-streams.
    let mut output = Vec::with_capacity(uncompressed_size);
    let mut stream_idx = 0;

    loop {
        if output.len() >= uncompressed_size {
            break;
        }

        let sym = litlen_table.decode(&mut readers[stream_idx])?;

        match sym.cmp(&256) {
            std::cmp::Ordering::Less => {
                #[allow(clippy::cast_possible_truncation)]
                output.push(sym as u8); // sym < 256, truncation safe
                stream_idx = (stream_idx + 1) % NUM_STREAMS;
            }
            std::cmp::Ordering::Equal => {
                break;
            }
            std::cmp::Ordering::Greater => {
                // Length code 257..285
                let len_idx = usize::from(sym - 257);
                if len_idx >= LENGTH_BASE.len() {
                    return Err(CrushError::InvalidFormat(format!(
                        "GDeflate: invalid length code {sym}"
                    )));
                }
                let base_len = LENGTH_BASE[len_idx];
                let extra = LENGTH_EXTRA[len_idx];
                let extra_bits = if extra > 0 {
                    readers[stream_idx].read_bits(extra)?
                } else {
                    0
                };
                let length = usize::from(base_len + extra_bits);

                // Distance comes from the SAME stream (paired with length)
                let dist_sym = dist_table.decode(&mut readers[stream_idx])?;
                let dist_idx = usize::from(dist_sym);
                if dist_idx >= DIST_BASE.len() {
                    return Err(CrushError::InvalidFormat(format!(
                        "GDeflate: invalid distance code {dist_sym}"
                    )));
                }
                let base_dist = DIST_BASE[dist_idx];
                let dist_extra = DIST_EXTRA[dist_idx];
                let dist_extra_bits = if dist_extra > 0 {
                    readers[stream_idx].read_bits(dist_extra)?
                } else {
                    0
                };
                let distance = usize::from(base_dist + dist_extra_bits);

                // Advance stream after full length+distance pair
                stream_idx = (stream_idx + 1) % NUM_STREAMS;

                // Copy from output history
                if distance == 0 || distance > output.len() {
                    return Err(CrushError::InvalidFormat(format!(
                        "GDeflate: invalid match distance {distance} (output len {})",
                        output.len()
                    )));
                }
                let start = output.len() - distance;
                for j in 0..length {
                    let byte = output[start + j];
                    output.push(byte);
                }
            }
        }
    }

    output.truncate(uncompressed_size);
    Ok(output)
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Compress a tile (up to 64KB) into `GDeflate` format.
///
/// The output is a `GDeflate`-formatted bitstream payload that can be decompressed
/// by the GPU shader or the CPU fallback.
///
/// # Errors
///
/// Returns an error if the input exceeds 64KB.
pub fn gdeflate_compress_tile(tile_data: &[u8]) -> Result<Vec<u8>> {
    if tile_data.is_empty() {
        return Ok(Vec::new());
    }

    if tile_data.len() > 65536 {
        return Err(CrushError::from(PluginError::OperationFailed(
            "GDeflate: tile exceeds 64KB".to_owned(),
        )));
    }

    // Step 1: Find LZ77 matches.
    let symbols = lz77_find_matches(tile_data);

    // Step 2: Serialize symbols into GDeflate 32-way interleaved format.
    serialize_gdeflate(&symbols)
}

/// Decompress a `GDeflate` tile payload on the CPU (fallback path).
///
/// # Errors
///
/// Returns an error if the payload is malformed or contains invalid codes.
pub fn gdeflate_decompress_tile(payload: &[u8], uncompressed_size: usize) -> Result<Vec<u8>> {
    if payload.is_empty() {
        return Ok(Vec::new());
    }

    deserialize_gdeflate(payload, uncompressed_size)
}
