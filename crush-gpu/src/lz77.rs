//! Shared byte-aligned LZ77 codec
//!
//! Token format (byte-aligned, simple):
//!   Literal:  `0x00` `byte`
//!   Match:    `0x01` `length: u16 LE` `distance: u16 LE`
//!   End:      `0xFF`
//!
//! This is intentionally simple for the MVP.  A production implementation
//! would use Huffman coding, DEFLATE-compatible tokens, or a more
//! sophisticated encoding.

use crush_core::error::{CrushError, Result};

pub const TOKEN_LITERAL: u8 = 0x00;
pub const TOKEN_MATCH: u8 = 0x01;
pub const TOKEN_END: u8 = 0xFF;

/// Maximum match length (u16 range, capped to keep things reasonable).
pub const MAX_MATCH_LEN: usize = 258;

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Hash 3 bytes at `pos` for the LZ77 hash chain.
#[must_use]
pub fn hash3(data: &[u8], pos: usize) -> u32 {
    let b0 = u32::from(data[pos]);
    let b1 = u32::from(data[pos + 1]);
    let b2 = u32::from(data[pos + 2]);
    b0 | (b1 << 8) | (b2 << 16)
}

/// Count matching bytes between `data[a..]` and `data[b..]`, up to `max_len`.
#[must_use]
pub fn count_match(data: &[u8], a: usize, b: usize, max_len: usize) -> usize {
    let max = max_len.min(data.len() - b);
    let mut len = 0;
    while len < max && a + len < b && data[a + len] == data[b + len] {
        len += 1;
    }
    len
}

// ---------------------------------------------------------------------------
// Configurable LZ77 encoder
// ---------------------------------------------------------------------------

/// Configuration for the LZ77 encoder.
pub struct Lz77Config {
    /// Minimum match length (standard=6, enhanced=4).
    pub min_match_len: usize,
    /// Maximum match length.
    pub max_match_len: usize,
    /// Hash chain window size.
    pub window_size: usize,
    /// Maximum hash chain entries to probe.
    pub chain_depth: usize,
}

/// Standard configuration: conservative matching.
pub const STANDARD_CONFIG: Lz77Config = Lz77Config {
    min_match_len: 6,
    max_match_len: MAX_MATCH_LEN,
    window_size: 32768,
    chain_depth: 16,
};

/// Enhanced configuration: deeper search for text-heavy data.
pub const ENHANCED_CONFIG: Lz77Config = Lz77Config {
    min_match_len: 4,
    max_match_len: MAX_MATCH_LEN,
    window_size: 32768,
    chain_depth: 64,
};

/// LZ77 encode a byte slice using the given configuration.
#[must_use]
pub fn lz77_encode(input: &[u8], cfg: &Lz77Config) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len());
    let mut pos = 0;

    let mut hash_table: std::collections::HashMap<u32, Vec<usize>> =
        std::collections::HashMap::new();

    while pos < input.len() {
        let mut best_len = 0usize;
        let mut best_dist = 0usize;

        if pos + 3 <= input.len() {
            let h = hash3(input, pos);
            if let Some(positions) = hash_table.get(&h) {
                for &prev_pos in positions.iter().rev().take(cfg.chain_depth) {
                    if pos - prev_pos > cfg.window_size {
                        continue;
                    }
                    let match_len = count_match(input, prev_pos, pos, cfg.max_match_len);
                    if match_len >= cfg.min_match_len && match_len > best_len {
                        best_len = match_len;
                        best_dist = pos - prev_pos;
                        if best_len >= cfg.max_match_len {
                            break;
                        }
                    }
                }
            }
            hash_table.entry(h).or_default().push(pos);
        }

        // Emit match only if it saves space (match token = 5 bytes, literal = 2 bytes per byte).
        if best_len >= cfg.min_match_len && best_len * 2 > 5 {
            output.push(TOKEN_MATCH);
            // Safe: best_len ≤ MAX_MATCH_LEN (258), best_dist ≤ window_size (32768).
            #[allow(clippy::cast_possible_truncation)]
            let len_u16 = best_len as u16;
            #[allow(clippy::cast_possible_truncation)]
            let dist_u16 = best_dist as u16;
            output.extend_from_slice(&len_u16.to_le_bytes());
            output.extend_from_slice(&dist_u16.to_le_bytes());
            for p in (pos + 1)..pos + best_len {
                if p + 3 <= input.len() {
                    let h = hash3(input, p);
                    hash_table.entry(h).or_default().push(p);
                }
            }
            pos += best_len;
        } else {
            output.push(TOKEN_LITERAL);
            output.push(input[pos]);
            pos += 1;
        }
    }

    output.push(TOKEN_END);
    output
}

/// LZ77 decode an encoded byte slice.
///
/// # Errors
///
/// Returns [`CrushError::InvalidFormat`] if the encoded data contains
/// invalid tokens, truncated data, or out-of-bounds match distances.
pub fn lz77_decode(encoded: &[u8]) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut pos = 0;

    while pos < encoded.len() {
        match encoded[pos] {
            TOKEN_LITERAL => {
                if pos + 1 >= encoded.len() {
                    return Err(CrushError::InvalidFormat(
                        "LZ77: truncated literal token".to_owned(),
                    ));
                }
                output.push(encoded[pos + 1]);
                pos += 2;
            }
            TOKEN_MATCH => {
                if pos + 4 >= encoded.len() {
                    return Err(CrushError::InvalidFormat(
                        "LZ77: truncated match token".to_owned(),
                    ));
                }
                let length = u16::from_le_bytes([encoded[pos + 1], encoded[pos + 2]]) as usize;
                let distance = u16::from_le_bytes([encoded[pos + 3], encoded[pos + 4]]) as usize;
                pos += 5;

                if distance == 0 || distance > output.len() {
                    return Err(CrushError::InvalidFormat(format!(
                        "LZ77: invalid match distance {distance} (output len {})",
                        output.len()
                    )));
                }

                let start = output.len() - distance;
                for i in 0..length {
                    output.push(output[start + i]);
                }
            }
            TOKEN_END => {
                break;
            }
            other => {
                return Err(CrushError::InvalidFormat(format!(
                    "LZ77: unknown token 0x{other:02x}"
                )));
            }
        }
    }

    Ok(output)
}
