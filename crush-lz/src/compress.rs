//! `fast-lz` block compressor: greedy LZ77 with a single-probe hash table.
//!
//! Design (research R4): a 2^12-entry table of positions (16 KB, stays in
//! L1; 2^14 compressed 2% better but 16% slower), reused across
//! blocks without clearing (entries older than the current block fail the
//! range check); matches of 4 or more bytes found through a multiplicative
//! hash of the next 6 bytes; matches
//! extended 8 bytes at a time; the probe step grows while no match is found,
//! so incompressible regions are skipped quickly.

use crate::format::{BLOCK_HEADER_SIZE, BlockHeader, END_MARKER, FrameHeader, compress_bound};
use crush_core::error::{PluginError, Result};
use std::sync::atomic::{AtomicBool, Ordering};

const HASH_LOG: u32 = 12;
const HASH_SIZE: usize = 1 << HASH_LOG;
const MIN_MATCH: usize = 4;
const MAX_OFFSET: usize = 65_535;
/// Probes without a match before the step grows by one byte.
const SKIP_TRIGGER: u32 = 6;
/// Matches only start where 8 bytes can be read.
const READ_AHEAD: usize = 8;

/// Reusable compressor state (about 16 kilobytes).
pub(crate) struct Compressor {
    table: Box<[u32; HASH_SIZE]>,
    /// Table value of the current block's first byte. Positions are stored as
    /// `base + i`; anything below `base` belongs to an earlier block.
    base: u32,
}

impl Compressor {
    // 16 KB built on the stack then moved into the box: small enough for
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

    /// Write the sequences for `src` (one block) to `dst`, which must hold
    /// at least `src.len() + WILD` bytes. Returns the number of bytes
    /// written, or `None` as soon as the block cannot come out smaller than
    /// `src` (the caller then stores it).
    pub(crate) fn compress_block(&mut self, src: &[u8], dst: &mut [u8]) -> Option<usize> {
        self.start_block(src.len());
        let base = self.base;
        let len = src.len();
        let mut anchor = 0usize;
        let mut op = 0usize;
        let result = self.find_sequences(src, dst, base, &mut anchor, &mut op);
        self.base = base + pos32(len) + 1;
        result?;
        emit_last_literals(dst, &mut op, &src[anchor..], len)?;
        Some(op)
    }

    /// The match search over `src`; emits every sequence but the last.
    fn find_sequences(
        &mut self,
        src: &[u8],
        dst: &mut [u8],
        base: u32,
        anchor: &mut usize,
        op: &mut usize,
    ) -> Option<()> {
        let len = src.len();

        if len > READ_AHEAD + MIN_MATCH {
            let limit = len - READ_AHEAD;
            let mut i = 0usize;
            let mut misses = 0u32;
            while i < limit {
                let word = read_u32(src, i);
                let h = hash6(read_u64(src, i));
                let cand = self.table[h];
                let here = base + pos32(i);
                self.table[h] = here;

                let c = cand.wrapping_sub(base) as usize;
                if cand >= base && i - c <= MAX_OFFSET && read_u32(src, c) == word {
                    // Extend backwards over equal bytes not yet emitted.
                    let (mut s, mut c) = (i, c);
                    while s > *anchor && c > 0 && src[s - 1] == src[c - 1] {
                        s -= 1;
                        c -= 1;
                    }
                    let mlen = MIN_MATCH + count_equal(src, s + MIN_MATCH, c + MIN_MATCH);
                    emit_sequence(dst, op, src, *anchor, s, s - c, mlen)?;
                    i = s + mlen;
                    *anchor = i;
                    misses = 0;
                    // Index a position inside the match for the next search.
                    if i < limit && i >= 2 {
                        self.table[hash6(read_u64(src, i - 2))] = base + pos32(i - 2);
                    }
                } else {
                    misses += 1;
                    i += 1 + (misses >> SKIP_TRIGGER) as usize;
                }
            }
        }
        Some(())
    }
}

#[inline]
fn pos32(i: usize) -> u32 {
    // Blocks are at most 1 megabyte, so positions within a block fit.
    u32::try_from(i).unwrap_or(u32::MAX)
}

/// Hash of the 6 bytes at the low end of `x`. Hashing more than the 4
/// bytes a match needs skips most 4- and 5-byte coincidences, which make
/// short, costly sequences: faster in both directions for a small ratio
/// cost (measured 4 to 8 bytes, perf-log experiments 4 and 5).
#[inline]
fn hash6(x: u64) -> usize {
    ((x << 16).wrapping_mul(227_718_039_650_203) >> (64 - HASH_LOG)) as usize & (HASH_SIZE - 1)
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

/// Bytes a literal copy may write past its end; `dst` has this much slack.
pub(crate) const WILD: usize = 16;

/// Bytes a length of `n` takes after its 4-bit field (`n >= 15`).
#[inline]
fn ext_len(n: usize) -> usize {
    if n >= 15 { (n - 15) / 255 + 1 } else { 0 }
}

/// Write the 255-continued extension of `n` (`n >= 15`) at `dst[*op..]`.
#[inline]
fn put_length_ext(dst: &mut [u8], op: &mut usize, n: usize) {
    let mut n = n - 15;
    while n >= 255 {
        dst[*op] = 255;
        *op += 1;
        n -= 255;
    }
    // n < 255 here
    dst[*op] = u8::try_from(n).unwrap_or(254);
    *op += 1;
}

#[inline]
fn nibble(n: usize) -> u8 {
    u8::try_from(n.min(15)).unwrap_or(15)
}

/// Write `src[from..to]` as literals at `dst[*op..]`: 16-byte steps that
/// may run up to `WILD - 1` bytes past the end (into later output or the
/// slack) when `src` has room to read them, an exact copy otherwise.
#[inline]
fn put_literals(dst: &mut [u8], op: &mut usize, src: &[u8], from: usize, to: usize) {
    let n = to - from;
    if n <= WILD && from + WILD <= src.len() {
        dst[*op..*op + WILD].copy_from_slice(&src[from..from + WILD]);
    } else {
        dst[*op..*op + n].copy_from_slice(&src[from..to]);
    }
    *op += n;
}

/// Emit literals `src[anchor..s]` and a match of `mlen` at `offset`.
/// `None` once the output would reach the block's own size: the block is
/// then stored, as it would have been after a full encode.
#[inline]
fn emit_sequence(
    dst: &mut [u8],
    op: &mut usize,
    src: &[u8],
    anchor: usize,
    s: usize,
    offset: usize,
    mlen: usize,
) -> Option<()> {
    let lit = s - anchor;
    let ml = mlen - MIN_MATCH;
    let need = 1 + ext_len(lit) + lit + 2 + ext_len(ml);
    if *op + need >= src.len() {
        return None;
    }
    // From here every write ends within src.len() + WILD <= dst.len().
    dst[*op] = (nibble(lit) << 4) | nibble(ml);
    *op += 1;
    if lit >= 15 {
        put_length_ext(dst, op, lit);
    }
    put_literals(dst, op, src, anchor, s);
    dst[*op..*op + 2].copy_from_slice(&u16::try_from(offset).unwrap_or(0).to_le_bytes());
    *op += 2;
    if ml >= 15 {
        put_length_ext(dst, op, ml);
    }
    Some(())
}

/// Emit the final, literals-only sequence; `None` if the block would not
/// come out smaller than `len` bytes.
fn emit_last_literals(dst: &mut [u8], op: &mut usize, literals: &[u8], len: usize) -> Option<()> {
    let lit = literals.len();
    if *op + 1 + ext_len(lit) + lit >= len {
        return None;
    }
    dst[*op] = nibble(lit) << 4;
    *op += 1;
    if lit >= 15 {
        put_length_ext(dst, op, lit);
    }
    dst[*op..*op + lit].copy_from_slice(literals);
    *op += lit;
    Some(())
}

/// Compress `input` into a full `fast-lz` payload with blocks of `2^block_size_log2` bytes.
pub(crate) fn compress_frame(
    input: &[u8],
    frame: FrameHeader,
    cancel: Option<&AtomicBool>,
) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    compress_frame_into(input, frame, cancel, &mut out)?;
    Ok(out)
}

/// [`compress_frame`], appending the payload to `out` after what it holds.
pub(crate) fn compress_frame_into(
    input: &[u8],
    frame: FrameHeader,
    cancel: Option<&AtomicBool>,
    out: &mut Vec<u8>,
) -> Result<()> {
    let block_size = frame.block_size();
    out.reserve(compress_bound(input.len(), block_size));
    out.extend_from_slice(&frame.to_bytes());
    let mut compressor = Compressor::new();

    for chunk in input.chunks(block_size) {
        if cancel.is_some_and(|c| c.load(Ordering::Acquire)) {
            return Err(PluginError::Cancelled.into());
        }
        let header_at = out.len();
        let payload_at = header_at + BLOCK_HEADER_SIZE;
        out.resize(payload_at + chunk.len() + WILD, 0);
        let written = compressor.compress_block(chunk, &mut out[payload_at..]);
        let stored = written.is_none();
        if let Some(n) = written {
            out.truncate(payload_at + n);
        } else {
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
    Ok(())
}
