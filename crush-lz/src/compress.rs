//! `fast-lz` block compressor: greedy LZ77 with a single-probe hash table.
//!
//! Design (research R4): a 2^14-entry table of positions, reused across
//! blocks without clearing (entries older than the current block fail the
//! range check); 4-byte matches found by a multiplicative hash; matches
//! extended 8 bytes at a time; the probe step grows while no match is found,
//! so incompressible regions are skipped quickly.

use crate::format::{BLOCK_HEADER_SIZE, BlockHeader, END_MARKER, FrameHeader, compress_bound};
use crush_core::error::{PluginError, Result};
use std::sync::atomic::{AtomicBool, Ordering};

const HASH_LOG: u32 = 14;
const HASH_SIZE: usize = 1 << HASH_LOG;
const MIN_MATCH: usize = 4;
const MAX_OFFSET: usize = 65_535;
/// Probes without a match before the step grows by one byte.
const SKIP_TRIGGER: u32 = 6;
/// Matches only start where 8 bytes can be read.
const READ_AHEAD: usize = 8;

/// Reusable compressor state (about 64 kilobytes).
pub(crate) struct Compressor {
    table: Box<[u32; HASH_SIZE]>,
    /// Table value of the current block's first byte. Positions are stored as
    /// `base + i`; anything below `base` belongs to an earlier block.
    base: u32,
}

impl Compressor {
    // 64 KB built on the stack then moved into the box: small enough for
    // every thread's stack, and it keeps the table a fixed-size array so
    // indexing with `& (HASH_SIZE - 1)` needs no bounds check.
    #[allow(clippy::large_stack_arrays)]
    pub(crate) fn new() -> Self {
        Self {
            table: Box::new([0; HASH_SIZE]),
            base: 1,
        }
    }

    /// Prepare for a block of `len` bytes: advance `base` past the previous
    /// block, clearing the table only when the position counter would overflow.
    fn start_block(&mut self, len: usize) {
        let len = u32::try_from(len).unwrap_or(u32::MAX);
        if self.base > u32::MAX - len.saturating_add(2) {
            self.table.fill(0);
            self.base = 1;
        }
    }

    /// Append the sequences for `src` (one block) to `dst`.
    pub(crate) fn compress_block(&mut self, src: &[u8], dst: &mut Vec<u8>) {
        self.start_block(src.len());
        let base = self.base;
        let len = src.len();
        let mut anchor = 0usize;

        if len > READ_AHEAD + MIN_MATCH {
            let limit = len - READ_AHEAD;
            let mut i = 0usize;
            let mut misses = 0u32;
            while i < limit {
                let word = read_u32(src, i);
                let h = hash(word);
                let cand = self.table[h];
                let here = base + pos32(i);
                self.table[h] = here;

                let c = cand.wrapping_sub(base) as usize;
                if cand >= base && i - c <= MAX_OFFSET && read_u32(src, c) == word {
                    // Extend backwards over equal bytes not yet emitted.
                    let (mut s, mut c) = (i, c);
                    while s > anchor && c > 0 && src[s - 1] == src[c - 1] {
                        s -= 1;
                        c -= 1;
                    }
                    let mlen = MIN_MATCH + count_equal(src, s + MIN_MATCH, c + MIN_MATCH);
                    emit_sequence(dst, &src[anchor..s], s - c, mlen);
                    i = s + mlen;
                    anchor = i;
                    misses = 0;
                    // Index a position inside the match for the next search.
                    if i < limit && i >= 2 {
                        self.table[hash(read_u32(src, i - 2))] = base + pos32(i - 2);
                    }
                } else {
                    misses += 1;
                    i += 1 + (misses >> SKIP_TRIGGER) as usize;
                }
            }
        }

        emit_last_literals(dst, &src[anchor..]);
        self.base = base + pos32(len) + 1;
    }
}

#[inline]
fn pos32(i: usize) -> u32 {
    // Blocks are at most 1 megabyte, so positions within a block fit.
    u32::try_from(i).unwrap_or(u32::MAX)
}

#[inline]
fn hash(word: u32) -> usize {
    (word.wrapping_mul(2_654_435_761) >> (32 - HASH_LOG)) as usize & (HASH_SIZE - 1)
}

#[inline]
fn read_u32(s: &[u8], i: usize) -> u32 {
    match s.get(i..).and_then(<[u8]>::first_chunk::<4>) {
        Some(b) => u32::from_le_bytes(*b),
        None => 0,
    }
}

#[inline]
fn read_u64(s: &[u8], i: usize) -> u64 {
    match s.get(i..).and_then(<[u8]>::first_chunk::<8>) {
        Some(b) => u64::from_le_bytes(*b),
        None => 0,
    }
}

/// Number of equal bytes at `src[a..]` and `src[b..]` (`b < a`), up to the end of `src`.
#[inline]
fn count_equal(src: &[u8], mut a: usize, mut b: usize) -> usize {
    let start = a;
    while a + 8 <= src.len() {
        let x = read_u64(src, a) ^ read_u64(src, b);
        if x != 0 {
            return a - start + (x.trailing_zeros() / 8) as usize;
        }
        a += 8;
        b += 8;
    }
    while a < src.len() && src[a] == src[b] {
        a += 1;
        b += 1;
    }
    a - start
}

fn push_length_ext(dst: &mut Vec<u8>, mut n: usize) {
    while n >= 255 {
        dst.push(255);
        n -= 255;
    }
    // n < 255 here
    dst.push(u8::try_from(n).unwrap_or(254));
}

#[inline]
fn nibble(n: usize) -> u8 {
    u8::try_from(n.min(15)).unwrap_or(15)
}

fn emit_sequence(dst: &mut Vec<u8>, literals: &[u8], offset: usize, mlen: usize) {
    let ml = mlen - MIN_MATCH;
    dst.push((nibble(literals.len()) << 4) | nibble(ml));
    if literals.len() >= 15 {
        push_length_ext(dst, literals.len() - 15);
    }
    dst.extend_from_slice(literals);
    dst.extend_from_slice(&u16::try_from(offset).unwrap_or(0).to_le_bytes());
    if ml >= 15 {
        push_length_ext(dst, ml - 15);
    }
}

fn emit_last_literals(dst: &mut Vec<u8>, literals: &[u8]) {
    dst.push(nibble(literals.len()) << 4);
    if literals.len() >= 15 {
        push_length_ext(dst, literals.len() - 15);
    }
    dst.extend_from_slice(literals);
}

/// Compress `input` into a full `fast-lz` payload with blocks of `2^block_size_log2` bytes.
pub(crate) fn compress_frame(
    input: &[u8],
    frame: FrameHeader,
    cancel: Option<&AtomicBool>,
) -> Result<Vec<u8>> {
    let block_size = frame.block_size();
    let mut out = Vec::with_capacity(compress_bound(input.len(), block_size));
    out.extend_from_slice(&frame.to_bytes());
    let mut compressor = Compressor::new();

    for chunk in input.chunks(block_size) {
        if cancel.is_some_and(|c| c.load(Ordering::Acquire)) {
            return Err(PluginError::Cancelled.into());
        }
        let header_at = out.len();
        out.extend_from_slice(&[0; BLOCK_HEADER_SIZE]);
        let payload_at = out.len();
        compressor.compress_block(chunk, &mut out);
        let stored = out.len() - payload_at >= chunk.len();
        if stored {
            out.truncate(payload_at);
            out.extend_from_slice(chunk);
        }
        let header = BlockHeader {
            compressed_len: pos32(out.len() - payload_at),
            stored,
            uncompressed_len: pos32(chunk.len()),
        };
        out[header_at..payload_at].copy_from_slice(&header.to_bytes());
    }

    out.extend_from_slice(&END_MARKER);
    Ok(out)
}
