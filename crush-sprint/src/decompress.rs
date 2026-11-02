//! Block decompression. Every failure is an error, never a panic.

use crate::codec::{LITERALS_HUFFMAN, LITERALS_RAW, LITERALS_REPEAT, LITERALS_RLE};
use crate::format::{
    MAX_SEGMENT_SIZE, SEGMENT_HEADER_SIZE, SPRINT_VERSION, SegmentHeader, SegmentKind, corrupt,
    read_u24,
};
use crate::huffman::HuffmanDecoder;
use crate::sequences::SequenceDecoder;
use crush_core::error::{CrushError, Result};

/// Where a segment's literals are.
enum Literals<'a> {
    /// Raw, in the payload.
    Borrowed(&'a [u8]),
    /// This many bytes at the start of the decoder's buffer.
    Decoded(usize),
}

/// One worker's decompression state, reused across blocks.
#[derive(Debug)]
pub struct Decoder {
    sequences: SequenceDecoder,
    huffman: HuffmanDecoder,
    has_table: bool,
    literals: Vec<u8>,
}

impl Decoder {
    pub(crate) fn new() -> Result<Self> {
        Ok(Self {
            sequences: SequenceDecoder::new()?,
            huffman: HuffmanDecoder::empty(),
            has_table: false,
            literals: vec![0; MAX_SEGMENT_SIZE + 32],
        })
    }

    /// Read the literals section. Returns where the literals are, and the
    /// input after the section.
    fn read_literals<'a>(
        &mut self,
        input: &'a [u8],
        regenerated: usize,
    ) -> Result<(Literals<'a>, &'a [u8])> {
        let (&mode, rest) = input
            .split_first()
            .ok_or_else(|| corrupt("missing literals section"))?;
        let size = read_u24(rest).ok_or_else(|| corrupt("truncated literals header"))? as usize;
        let rest = &rest[3..];
        if size > regenerated {
            return Err(corrupt("too many literals"));
        }
        match mode {
            LITERALS_RAW => {
                let (bytes, rest) = rest
                    .split_at_checked(size)
                    .ok_or_else(|| corrupt("literals past the segment"))?;
                Ok((Literals::Borrowed(bytes), rest))
            }
            LITERALS_RLE => {
                let (&byte, rest) = rest
                    .split_first()
                    .ok_or_else(|| corrupt("truncated literals"))?;
                self.literals[..size].fill(byte);
                Ok((Literals::Decoded(size), rest))
            }
            LITERALS_HUFFMAN | LITERALS_REPEAT => {
                let total = read_u24(rest).ok_or_else(|| corrupt("truncated literals"))? as usize;
                let (section, rest) = rest[3..]
                    .split_at_checked(total)
                    .ok_or_else(|| corrupt("literals past the segment"))?;
                let streams = if mode == LITERALS_HUFFMAN {
                    let streams = self.read_table(section)?;
                    self.has_table = true;
                    streams
                } else if self.has_table {
                    section
                } else {
                    return Err(corrupt("repeat mode with no earlier table"));
                };
                self.huffman
                    .decode_4_streams(streams, &mut self.literals[..size])?;
                Ok((Literals::Decoded(size), rest))
            }
            _ => Err(corrupt("reserved literals mode")),
        }
    }

    /// Read a table description. Returns the streams after it.
    fn read_table<'a>(&mut self, section: &'a [u8]) -> Result<&'a [u8]> {
        let (&last, rest) = section
            .split_first()
            .ok_or_else(|| corrupt("missing Huffman table"))?;
        let symbols = usize::from(last) + 1;
        let (packed, streams) = rest
            .split_at_checked(symbols.div_ceil(2))
            .ok_or_else(|| corrupt("truncated Huffman table"))?;
        let mut lengths = [0u8; 256];
        for (pair, &byte) in lengths.chunks_mut(2).zip(packed) {
            pair[0] = byte & 15;
            if let Some(high) = pair.get_mut(1) {
                *high = byte >> 4;
            }
        }
        // An odd count leaves a high nibble past the last symbol.
        if symbols % 2 == 1 && symbols < 256 && lengths[symbols] != 0 {
            return Err(corrupt("Huffman table past its last symbol"));
        }
        self.huffman.rebuild(&lengths)?;
        Ok(streams)
    }

    fn compressed_segment(
        &mut self,
        body: &[u8],
        output: &mut [u8],
        start: usize,
    ) -> Result<usize> {
        let regenerated = read_u24(body).ok_or_else(|| corrupt("truncated segment"))? as usize;
        if regenerated == 0 || regenerated > MAX_SEGMENT_SIZE {
            return Err(corrupt("regenerated size out of range"));
        }
        let end = start + regenerated;
        if end > output.len() {
            return Err(corrupt("segments exceed the block size"));
        }
        let (literals, section) = self.read_literals(&body[3..], regenerated)?;
        let literals = match literals {
            Literals::Borrowed(bytes) => bytes,
            Literals::Decoded(size) => &self.literals[..size],
        };
        self.sequences
            .execute(section, literals, output, start, end)?;
        Ok(end)
    }
}

/// Decode a block's segments into `output`, filling it exactly.
pub(crate) fn decompress_block(
    decoder: &mut Decoder,
    payload: &[u8],
    output: &mut [u8],
) -> Result<()> {
    let (&version, mut rest) = payload
        .split_first()
        .ok_or_else(|| corrupt("empty block"))?;
    if version != SPRINT_VERSION {
        return Err(CrushError::VersionMismatch {
            file_version: format!("sprint {version}"),
            current_version: format!("sprint {SPRINT_VERSION}"),
        });
    }
    decoder.sequences.reset();
    decoder.has_table = false;

    let mut op = 0;
    while !rest.is_empty() {
        let header = SegmentHeader::parse(rest)?;
        rest = &rest[SEGMENT_HEADER_SIZE..];
        let size = header.size as usize;
        match header.kind {
            SegmentKind::Raw => {
                let (bytes, tail) = rest
                    .split_at_checked(size)
                    .ok_or_else(|| corrupt("raw segment past the payload"))?;
                let end = op + size;
                if size == 0 || size > MAX_SEGMENT_SIZE || end > output.len() {
                    return Err(corrupt("raw segment size out of range"));
                }
                output[op..end].copy_from_slice(bytes);
                op = end;
                rest = tail;
            }
            SegmentKind::Rle => {
                let (&byte, tail) = rest
                    .split_first()
                    .ok_or_else(|| corrupt("truncated RLE segment"))?;
                let end = op + size;
                if size == 0 || size > MAX_SEGMENT_SIZE || end > output.len() {
                    return Err(corrupt("RLE segment size out of range"));
                }
                output[op..end].fill(byte);
                op = end;
                rest = tail;
            }
            SegmentKind::Compressed => {
                let (body, tail) = rest
                    .split_at_checked(size)
                    .ok_or_else(|| corrupt("segment past the payload"))?;
                op = decoder.compressed_segment(body, output, op)?;
                rest = tail;
            }
        }
    }
    if op == output.len() {
        Ok(())
    } else {
        Err(corrupt("segments do not fill the block"))
    }
}
