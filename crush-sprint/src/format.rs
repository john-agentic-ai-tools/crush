//! Constants and small headers of the sprint block format.
//!
//! The contract is `specs/026-hybrid-speed-codec/contracts/sprint-format.md`.

use crush_core::error::{CrushError, Result, ValidationError};

/// Version byte at the start of every compressed block.
pub const SPRINT_VERSION: u8 = 1;
/// The most one segment can regenerate.
pub const MAX_SEGMENT_SIZE: usize = 131_072;
/// Independent block size (research R5).
pub const BLOCK_SIZE: usize = 4 << 20;
/// Bytes in a segment header.
pub const SEGMENT_HEADER_SIZE: usize = 3;

pub(crate) fn corrupt(what: &str) -> CrushError {
    ValidationError::CorruptedData(what.into()).into()
}

/// What a segment holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentKind {
    /// Bytes stored as they are.
    Raw,
    /// One byte repeated.
    Rle,
    /// Literals and sequences.
    Compressed,
}

/// A segment's type and size field. The size is the regenerated size for raw
/// and RLE segments, and the body size for compressed ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegmentHeader {
    /// Segment type.
    pub kind: SegmentKind,
    /// Size field, 22 bits.
    pub size: u32,
}

impl SegmentHeader {
    /// The header's 24 bits, type in the low two.
    #[must_use]
    pub fn to_bytes(self) -> [u8; SEGMENT_HEADER_SIZE] {
        let kind = match self.kind {
            SegmentKind::Raw => 0,
            SegmentKind::Rle => 1,
            SegmentKind::Compressed => 2,
        };
        write_u24(kind | (self.size << 2))
    }

    /// Parse the header at the start of `input`.
    ///
    /// # Errors
    ///
    /// `CorruptedData` if `input` is too short or the type is reserved.
    pub fn parse(input: &[u8]) -> Result<Self> {
        let value = read_u24(input).ok_or_else(|| corrupt("truncated segment header"))?;
        let kind = match value & 3 {
            0 => SegmentKind::Raw,
            1 => SegmentKind::Rle,
            2 => SegmentKind::Compressed,
            _ => return Err(corrupt("reserved segment type")),
        };
        Ok(Self {
            kind,
            size: value >> 2,
        })
    }
}

pub(crate) fn write_u24(value: u32) -> [u8; 3] {
    let [a, b, c, _] = value.to_le_bytes();
    [a, b, c]
}

pub(crate) fn read_u24(input: &[u8]) -> Option<u32> {
    match input {
        [a, b, c, ..] => Some(u32::from_le_bytes([*a, *b, *c, 0])),
        _ => None,
    }
}

/// Append `value` as LEB128.
pub(crate) fn write_varint(mut value: usize, out: &mut Vec<u8>) {
    while value >= 0x80 {
        out.push((value & 0x7F) as u8 | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

/// Read a LEB128 value of at most 3 bytes, enough for any sequence count.
pub(crate) fn read_varint(input: &[u8]) -> Option<(usize, &[u8])> {
    let mut value = 0;
    for (i, &byte) in input.iter().take(3).enumerate() {
        value |= usize::from(byte & 0x7F) << (7 * i);
        if byte & 0x80 == 0 {
            return Some((value, &input[i + 1..]));
        }
    }
    None
}
