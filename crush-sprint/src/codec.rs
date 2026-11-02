//! Block compression, and [`BlockCodec`] for sprint.

use crate::bits::BitWriter;
use crate::decompress::{Decoder, decompress_block};
use crate::format::{
    BLOCK_SIZE, MAX_SEGMENT_SIZE, SEGMENT_HEADER_SIZE, SPRINT_VERSION, SegmentHeader, SegmentKind,
    write_u24,
};
use crate::histogram::{byte_histogram, looks_incompressible};
use crate::huffman::{HuffmanEncoder, build_code_lengths};
use crate::matcher::{Matcher, Sequence};
use crate::sequences::SequenceEncoder;
use crate::tans::{EncodeTable, normalize_counts, write_counts};
use crush_core::error::Result;
use crush_parallel::codec::BlockCodec;

/// The value stored in the container header.
pub const CODEC_ID: u8 = 1;

/// 8-byte match table size. 2^18 reaches a better ratio, but 24 threads of it
/// overflow a shared L3 (`perf/log/2026-11-hybrid-speed-codec.md`).
const LONG_LOG: u32 = 17;
/// 5-byte match table size.
const SHORT_LOG: u32 = 16;
/// Below this, Huffman's table costs more than it saves.
const MIN_HUFFMAN_LITERALS: usize = 64;
/// One byte in this many feeds the segment boundary search.
const SPLIT_SAMPLE: usize = 8;
/// Bytes compared at a time by the boundary search.
const SPLIT_CHUNK: usize = 8192;
/// Total variation between a chunk and its segment that ends the segment, of
/// a possible 2. Tuned on `real`, where lower costs more in tables than it saves.
const SPLIT_DISTANCE: f64 = 0.7;
/// No boundary before this many bytes.
const MIN_SPLIT: usize = 16_384;
/// Bytes one short literal copy moves.
const WILD_COPY: usize = 16;

pub(crate) const LENGTHS_NIBBLES: u8 = 0;
pub(crate) const LENGTHS_CODED: u8 = 1;
/// Table log for coded Huffman lengths.
pub(crate) const LENGTHS_LOG: u8 = 6;

pub(crate) const LITERALS_RAW: u8 = 0;
pub(crate) const LITERALS_RLE: u8 = 1;
pub(crate) const LITERALS_HUFFMAN: u8 = 2;
pub(crate) const LITERALS_REPEAT: u8 = 3;

/// The sprint block codec.
#[derive(Debug, Clone, Copy, Default)]
pub struct SprintCodec;

/// One worker's compression state, reused across blocks.
#[derive(Debug)]
pub struct Encoder {
    matcher: Matcher,
    sequences: SequenceEncoder,
    found: Vec<Sequence>,
    /// Fixed size. The first `literal_count` bytes are this segment's.
    literals: Vec<u8>,
    literal_count: usize,
    body: Vec<u8>,
    /// The Huffman table the decoder holds, if any.
    table: Option<(HuffmanEncoder, [u8; 256])>,
    /// A table this segment set, until the segment is kept.
    pending: Option<(HuffmanEncoder, [u8; 256])>,
}

impl Encoder {
    fn new() -> Result<Self> {
        Ok(Self {
            matcher: Matcher::new(LONG_LOG, SHORT_LOG)?,
            sequences: SequenceEncoder::new()?,
            found: Vec::with_capacity(MAX_SEGMENT_SIZE / 8),
            literals: vec![0; MAX_SEGMENT_SIZE + WILD_COPY],
            literal_count: 0,
            body: Vec::with_capacity(MAX_SEGMENT_SIZE + 1024),
            table: None,
            pending: None,
        })
    }

    fn reset(&mut self) {
        self.matcher.reset();
        self.sequences.reset();
        self.table = None;
        self.pending = None;
    }

    /// Append the literals section to `self.body`.
    fn write_literals(&mut self) -> Result<()> {
        let literals = &self.literals[..self.literal_count];
        let count = write_u24(literals.len() as u32);
        let first = literals.first().copied().unwrap_or(0);
        if !literals.is_empty() && literals.iter().all(|&b| b == first) {
            self.body.push(LITERALS_RLE);
            self.body.extend_from_slice(&count);
            self.body.push(first);
            return Ok(());
        }
        if literals.len() < MIN_HUFFMAN_LITERALS || looks_incompressible(literals) {
            self.write_raw_literals();
            return Ok(());
        }

        let histogram = byte_histogram(literals);
        let lengths = build_code_lengths(&histogram)?;
        let new_bits = code_bits(&histogram, &lengths);
        let description = description_size(&lengths);
        // Repeat costs nothing to describe and saves a table build in the decoder.
        let repeat = self.table.as_ref().filter(|(_, previous)| {
            all_coded(&histogram, previous)
                && code_bits(&histogram, previous) <= new_bits + description as u64 * 8
        });

        let at = self.body.len();
        if let Some((encoder, _)) = repeat {
            self.body.push(LITERALS_REPEAT);
            self.body.extend_from_slice(&count);
            self.body.extend_from_slice(&[0; 3]);
            encoder.encode_4_streams(literals, &mut self.body)?;
        } else {
            let encoder = HuffmanEncoder::from_lengths(&lengths)?;
            self.body.push(LITERALS_HUFFMAN);
            self.body.extend_from_slice(&count);
            self.body.extend_from_slice(&[0; 3]);
            write_description(&lengths, &mut self.body);
            encoder.encode_4_streams(literals, &mut self.body)?;
            self.pending = Some((encoder, lengths));
        }
        let total = self.body.len() - at - 7;
        if total + 7 >= literals.len() + 4 {
            self.body.truncate(at);
            self.pending = None;
            self.write_raw_literals();
        } else {
            self.body[at + 4..at + 7].copy_from_slice(&write_u24(total as u32));
        }
        Ok(())
    }

    fn write_raw_literals(&mut self) {
        self.body.push(LITERALS_RAW);
        self.body
            .extend_from_slice(&write_u24(self.literal_count as u32));
        self.body
            .extend_from_slice(&self.literals[..self.literal_count]);
    }

    /// Collect the bytes between matches. Most runs are short, so a fixed-size
    /// copy that may run past them beats a call per run.
    fn gather_literals(&mut self, block: &[u8], start: usize, tail: usize, end: usize) {
        let mut at = start;
        let mut to = 0;
        for s in &self.found {
            let run = s.literal_length as usize;
            if run <= WILD_COPY && at + WILD_COPY <= block.len() {
                let word: [u8; WILD_COPY] = block
                    .get(at..at + WILD_COPY)
                    .and_then(|w| w.try_into().ok())
                    .unwrap_or_default();
                self.literals[to..to + WILD_COPY].copy_from_slice(&word);
            } else {
                self.literals[to..to + run].copy_from_slice(&block[at..at + run]);
            }
            to += run;
            at += run + s.match_length as usize;
        }
        let rest = end - tail;
        self.literals[to..to + rest].copy_from_slice(&block[tail..end]);
        self.literal_count = to + rest;
    }

    /// Append `block[start..end]` to `out` as one segment.
    fn write_segment(
        &mut self,
        block: &[u8],
        start: usize,
        end: usize,
        repeats: &mut [u32; 3],
        out: &mut Vec<u8>,
    ) -> Result<()> {
        let segment = &block[start..end];
        let first = segment.first().copied().unwrap_or(0);
        if segment.iter().all(|&b| b == first) {
            let header = SegmentHeader {
                kind: SegmentKind::Rle,
                size: segment.len() as u32,
            };
            out.extend_from_slice(&header.to_bytes());
            out.push(first);
            return Ok(());
        }

        self.found.clear();
        let tail = self
            .matcher
            .find(block, start, end, repeats, &mut self.found);
        self.gather_literals(block, start, tail, end);

        self.body.clear();
        self.body
            .extend_from_slice(&write_u24(segment.len() as u32));
        self.write_literals()?;
        self.sequences.encode(&self.found, &mut self.body)?;

        if self.body.len() >= segment.len() {
            // The decoder will not see this segment, so nothing it set counts.
            self.pending = None;
            *repeats = self.sequences.repeats();
            let header = SegmentHeader {
                kind: SegmentKind::Raw,
                size: segment.len() as u32,
            };
            out.extend_from_slice(&header.to_bytes());
            out.extend_from_slice(segment);
        } else {
            self.sequences.commit(!self.found.is_empty());
            if let Some(pending) = self.pending.take() {
                self.table = Some(pending);
            }
            let header = SegmentHeader {
                kind: SegmentKind::Compressed,
                size: self.body.len() as u32,
            };
            out.reserve(SEGMENT_HEADER_SIZE + self.body.len());
            out.extend_from_slice(&header.to_bytes());
            out.extend_from_slice(&self.body);
        }
        Ok(())
    }
}

/// Sample bytes in chunks, and end the segment where a chunk stops looking
/// like the segment so far, so each entropy table fits its data.
fn segment_end(block: &[u8], start: usize) -> usize {
    let limit = (start + MAX_SEGMENT_SIZE).min(block.len());
    let mut total = [0u32; 256];
    let mut count = 0u32;
    for (i, part) in block[start..limit].chunks(SPLIT_CHUNK).enumerate() {
        let mut current = [0u32; 256];
        for &byte in part.iter().step_by(SPLIT_SAMPLE) {
            current[usize::from(byte)] += 1;
        }
        let part_count: u32 = current.iter().sum();
        if count != 0 && (i + 1) * SPLIT_CHUNK > MIN_SPLIT {
            let (n, m) = (f64::from(count), f64::from(part_count));
            let distance: f64 = total
                .iter()
                .zip(&current)
                .map(|(&t, &c)| (f64::from(t) / n - f64::from(c) / m).abs())
                .sum();
            if distance > SPLIT_DISTANCE {
                return start + i * SPLIT_CHUNK;
            }
        }
        for (t, c) in total.iter_mut().zip(&current) {
            *t += c;
        }
        count += part_count;
    }
    limit
}

fn code_bits(histogram: &[u32; 256], lengths: &[u8; 256]) -> u64 {
    let pairs = histogram.iter().zip(lengths);
    pairs.map(|(&c, &l)| u64::from(c) * u64::from(l)).sum()
}

fn all_coded(histogram: &[u32; 256], lengths: &[u8; 256]) -> bool {
    let pairs = histogram.iter().zip(lengths);
    pairs.into_iter().all(|(&c, &l)| c == 0 || l != 0)
}

fn last_symbol(lengths: &[u8; 256]) -> usize {
    lengths.iter().rposition(|&l| l != 0).unwrap_or(0)
}

/// Bytes [`write_description`] would use without coding its lengths.
fn description_size(lengths: &[u8; 256]) -> usize {
    2 + (last_symbol(lengths) + 1).div_ceil(2)
}

/// Last symbol, a form byte, then a length for each symbol up to it: 4 bits
/// each for form 0, or tANS coded for form 1 when that is smaller.
fn write_description(lengths: &[u8; 256], out: &mut Vec<u8>) {
    let last = last_symbol(lengths);
    out.push(last as u8);
    let start = out.len();
    if !write_coded_lengths(&lengths[..=last], out)
        || out.len() - start >= description_size(lengths) - 1
    {
        out.truncate(start);
        out.push(LENGTHS_NIBBLES);
        for pair in lengths[..=last].chunks(2) {
            let high = pair.get(1).copied().unwrap_or(0);
            out.push(pair[0] | (high << 4));
        }
    }
}

/// Returns false if these lengths cannot be tANS coded.
fn write_coded_lengths(lengths: &[u8], out: &mut Vec<u8>) -> bool {
    let mut counts = [0u32; 16];
    for &l in lengths {
        counts[usize::from(l) & 15] += 1;
    }
    let Ok(normalized) = normalize_counts(&counts[..=12], LENGTHS_LOG) else {
        return false;
    };
    let Ok(table) = EncodeTable::new(&normalized, LENGTHS_LOG, LENGTHS_LOG) else {
        return false;
    };
    let mut writer = BitWriter::new();
    table.encode(lengths, &mut writer);
    let stream = writer.finish();
    let Ok(size) = u8::try_from(stream.len()) else {
        return false;
    };
    out.push(LENGTHS_CODED);
    if write_counts(&normalized, LENGTHS_LOG, out).is_err() {
        return false;
    }
    out.push(size);
    out.extend_from_slice(&stream);
    true
}

impl BlockCodec for SprintCodec {
    const CODEC_ID: u8 = CODEC_ID;
    type Encoder = Box<Encoder>;
    type Decoder = Box<Decoder>;

    fn encoder(&self) -> Result<Self::Encoder> {
        Encoder::new().map(Box::new)
    }

    fn decoder(&self) -> Result<Self::Decoder> {
        Decoder::new().map(Box::new)
    }

    fn compress_block(
        &self,
        encoder: &mut Self::Encoder,
        input: &[u8],
        out: &mut Vec<u8>,
    ) -> Result<bool> {
        debug_assert!(input.len() <= BLOCK_SIZE);
        out.clear();
        out.push(SPRINT_VERSION);
        encoder.reset();
        let mut repeats = encoder.sequences.repeats();
        let mut start = 0;
        while start < input.len() {
            let end = segment_end(input, start);
            encoder.write_segment(input, start, end, &mut repeats, out)?;
            start = end;
        }
        Ok(out.len() < input.len())
    }

    fn decompress_block(
        &self,
        decoder: &mut Self::Decoder,
        payload: &[u8],
        output: &mut [u8],
    ) -> Result<()> {
        decompress_block(decoder, payload, output)
    }
}
