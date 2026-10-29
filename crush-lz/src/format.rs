//! `fast-lz` stream layout: frame header, block headers, end marker, bounds.
//!
//! ```text
//! payload      := frame_header block* end_marker
//! frame_header := "LZ" version(1) flags(0) block_size_log2 reserved[3]
//! block        := u32 (compressed_len | stored << 31)  u32 uncompressed_len  payload
//! end_marker   := u32 0
//! ```
//!
//! All integers are little-endian. See
//! `specs/023-lz4-fast-plugin/contracts/fast-lz-format.md`.

use crush_core::error::{CrushError, Result, ValidationError};

/// Size of the frame header in bytes.
pub const FRAME_HEADER_SIZE: usize = 8;
/// Size of a block header in bytes.
pub const BLOCK_HEADER_SIZE: usize = 8;
/// Marks the end of the block list: a `compressed_len` word of zero.
pub const END_MARKER: [u8; 4] = [0; 4];
/// Smallest allowed block size, as a power of two (64 kilobytes).
pub const MIN_BLOCK_SIZE_LOG2: u8 = 16;
/// Largest allowed block size, as a power of two (1 megabyte).
pub const MAX_BLOCK_SIZE_LOG2: u8 = 20;
/// Default block size, as a power of two (1 megabyte).
pub const DEFAULT_BLOCK_SIZE_LOG2: u8 = 20;

const MAGIC: [u8; 2] = *b"LZ";
const VERSION: u8 = 1;
const STORED_BIT: u32 = 1 << 31;

/// A `CorruptedData` validation error with `msg`.
pub(crate) fn corrupt(msg: impl Into<String>) -> CrushError {
    ValidationError::CorruptedData(format!("fast-lz: {}", msg.into())).into()
}

/// Worst-case compressed size of one block of `uncompressed_len` bytes
/// (literal-only encoding: one length byte per 255 literals, plus slack).
#[must_use]
pub fn block_bound(uncompressed_len: usize) -> usize {
    uncompressed_len + uncompressed_len / 255 + 16
}

/// Worst-case size of a whole `fast-lz` payload for `input_len` bytes split
/// into blocks of `block_size`.
#[must_use]
pub fn compress_bound(input_len: usize, block_size: usize) -> usize {
    let full = input_len / block_size;
    let tail = input_len % block_size;
    let mut bound = FRAME_HEADER_SIZE + END_MARKER.len();
    bound += full * (BLOCK_HEADER_SIZE + block_bound(block_size));
    if tail > 0 {
        bound += BLOCK_HEADER_SIZE + block_bound(tail);
    }
    bound
}

/// Frame header: format identity and block size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameHeader {
    /// Block size as a power of two, `MIN_BLOCK_SIZE_LOG2..=MAX_BLOCK_SIZE_LOG2`.
    pub block_size_log2: u8,
}

impl Default for FrameHeader {
    fn default() -> Self {
        Self {
            block_size_log2: DEFAULT_BLOCK_SIZE_LOG2,
        }
    }
}

impl FrameHeader {
    /// Header for blocks of `2^block_size_log2` bytes.
    ///
    /// # Errors
    ///
    /// `CorruptedData` if `block_size_log2` is outside 16..=20.
    pub fn new(block_size_log2: u8) -> Result<Self> {
        if !(MIN_BLOCK_SIZE_LOG2..=MAX_BLOCK_SIZE_LOG2).contains(&block_size_log2) {
            return Err(corrupt(format!(
                "block_size_log2 {block_size_log2} outside {MIN_BLOCK_SIZE_LOG2}..={MAX_BLOCK_SIZE_LOG2}"
            )));
        }
        Ok(Self { block_size_log2 })
    }

    /// Block size in bytes.
    #[must_use]
    pub fn block_size(&self) -> usize {
        1 << self.block_size_log2
    }

    /// Encoded header.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; FRAME_HEADER_SIZE] {
        [
            MAGIC[0],
            MAGIC[1],
            VERSION,
            0,
            self.block_size_log2,
            0,
            0,
            0,
        ]
    }

    /// Parse and validate a frame header from the start of `bytes`.
    ///
    /// # Errors
    ///
    /// `CorruptedData` if the header is truncated or any field is invalid.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let Some(h) = bytes.get(..FRAME_HEADER_SIZE) else {
            return Err(corrupt("truncated frame header"));
        };
        if h[..2] != MAGIC {
            return Err(corrupt("bad frame magic"));
        }
        if h[2] != VERSION {
            return Err(corrupt(format!("unsupported version {}", h[2])));
        }
        if h[3] != 0 {
            return Err(corrupt(format!("unknown flags {:#04x}", h[3])));
        }
        if h[5..8] != [0, 0, 0] {
            return Err(corrupt("non-zero reserved bytes"));
        }
        Self::new(h[4])
    }
}

/// Block header: sizes and the stored flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockHeader {
    /// Payload length in bytes (low 31 bits on the wire).
    pub compressed_len: u32,
    /// Payload is the raw input bytes.
    pub stored: bool,
    /// Decoded length of the block.
    pub uncompressed_len: u32,
}

impl BlockHeader {
    /// Encoded header.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; BLOCK_HEADER_SIZE] {
        let word = (self.compressed_len & !STORED_BIT) | if self.stored { STORED_BIT } else { 0 };
        let mut b = [0u8; BLOCK_HEADER_SIZE];
        b[..4].copy_from_slice(&word.to_le_bytes());
        b[4..].copy_from_slice(&self.uncompressed_len.to_le_bytes());
        b
    }

    /// Parse the next block header from the start of `bytes`, or `None` at the
    /// end marker. `block_size` is the frame's block size.
    ///
    /// # Errors
    ///
    /// `CorruptedData` if the header is truncated or its sizes are invalid.
    pub fn parse(bytes: &[u8], block_size: u32) -> Result<Option<Self>> {
        let Some(w) = bytes.get(..4) else {
            return Err(corrupt("truncated block header"));
        };
        let word = u32::from_le_bytes([w[0], w[1], w[2], w[3]]);
        if word == 0 {
            return Ok(None);
        }
        let Some(u) = bytes.get(4..BLOCK_HEADER_SIZE) else {
            return Err(corrupt("truncated block header"));
        };
        let h = Self {
            compressed_len: word & !STORED_BIT,
            stored: word & STORED_BIT != 0,
            uncompressed_len: u32::from_le_bytes([u[0], u[1], u[2], u[3]]),
        };
        if h.uncompressed_len == 0 || h.uncompressed_len > block_size {
            return Err(corrupt(format!(
                "block uncompressed_len {} outside 1..={block_size}",
                h.uncompressed_len
            )));
        }
        if h.compressed_len == 0 {
            return Err(corrupt("empty block payload"));
        }
        if h.stored && h.compressed_len != h.uncompressed_len {
            return Err(corrupt("stored block length mismatch"));
        }
        if h.compressed_len as usize > block_bound(h.uncompressed_len as usize) {
            return Err(corrupt("block compressed_len exceeds bound"));
        }
        Ok(Some(h))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::needless_pass_by_value)]
    fn is_corrupt<T: std::fmt::Debug>(r: Result<T>) -> bool {
        matches!(
            r,
            Err(CrushError::Validation(ValidationError::CorruptedData(_)))
        )
    }

    #[test]
    fn frame_header_round_trips() {
        for log2 in MIN_BLOCK_SIZE_LOG2..=MAX_BLOCK_SIZE_LOG2 {
            let h = FrameHeader::new(log2).expect("valid");
            let bytes = h.to_bytes();
            assert_eq!(&bytes[..2], b"LZ");
            assert_eq!(FrameHeader::parse(&bytes).expect("parse"), h);
            assert_eq!(h.block_size(), 1usize << log2);
        }
        assert_eq!(
            FrameHeader::default().block_size_log2,
            DEFAULT_BLOCK_SIZE_LOG2
        );
    }

    #[test]
    fn frame_header_rejects_invalid_fields() {
        let good = FrameHeader::default().to_bytes();
        let mut cases = Vec::new();
        for (i, v) in [
            (0, b'X'),
            (1, b'Y'),
            (2, 2),
            (3, 1),
            (4, 15),
            (4, 21),
            (5, 1),
            (7, 1),
        ] {
            let mut b = good;
            b[i] = v;
            cases.push(b);
        }
        for b in cases {
            assert!(is_corrupt(FrameHeader::parse(&b)), "{b:?}");
        }
        assert!(is_corrupt(FrameHeader::parse(&good[..7])));
        assert!(FrameHeader::new(15).is_err());
        assert!(FrameHeader::new(21).is_err());
    }

    #[test]
    fn block_header_round_trips_with_stored_bit() {
        for (c, stored, u) in [(100u32, false, 4096u32), (4096, true, 4096), (1, false, 1)] {
            let h = BlockHeader {
                compressed_len: c,
                stored,
                uncompressed_len: u,
            };
            let b = h.to_bytes();
            assert_eq!(
                u32::from_le_bytes([b[0], b[1], b[2], b[3]]) >> 31,
                u32::from(stored)
            );
            assert_eq!(BlockHeader::parse(&b, 1 << 16).expect("parse"), Some(h));
        }
    }

    #[test]
    fn end_marker_parses_as_none() {
        assert_eq!(
            BlockHeader::parse(&END_MARKER, 1 << 16).expect("parse"),
            None
        );
    }

    #[test]
    fn block_header_rejects_invalid_sizes() {
        let bs = 1 << 16;
        let bad = [
            // uncompressed_len 0
            BlockHeader {
                compressed_len: 10,
                stored: false,
                uncompressed_len: 0,
            },
            // uncompressed_len > block size
            BlockHeader {
                compressed_len: 10,
                stored: false,
                uncompressed_len: bs + 1,
            },
            // compressed_len above the bound
            BlockHeader {
                compressed_len: u32::try_from(block_bound(1000)).expect("fits") + 1,
                stored: false,
                uncompressed_len: 1000,
            },
            // stored with mismatching lengths
            BlockHeader {
                compressed_len: 999,
                stored: true,
                uncompressed_len: 1000,
            },
            // stored with zero length
            BlockHeader {
                compressed_len: 0,
                stored: true,
                uncompressed_len: 0,
            },
        ];
        for h in bad {
            assert!(is_corrupt(BlockHeader::parse(&h.to_bytes(), bs)), "{h:?}");
        }
        // Truncated header (not an end marker).
        assert!(is_corrupt(BlockHeader::parse(&[1, 0, 0, 0, 5], bs)));
        assert!(is_corrupt(BlockHeader::parse(&[0, 0], bs)));
    }

    #[test]
    fn compress_bound_matches_contract() {
        let bs = 1usize << 16;
        assert_eq!(compress_bound(0, bs), FRAME_HEADER_SIZE + END_MARKER.len());
        let n = 3 * bs + 5;
        let expected = FRAME_HEADER_SIZE
            + 3 * (BLOCK_HEADER_SIZE + bs + bs / 255 + 16)
            + (BLOCK_HEADER_SIZE + 5 + 16)
            + END_MARKER.len();
        assert_eq!(compress_bound(n, bs), expected);
    }
}
