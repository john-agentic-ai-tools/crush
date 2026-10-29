//! Validating `fast-lz` decoder.
//!
//! Every length and offset is checked against the payload, the bytes already
//! produced in the block, and the block's declared size before anything is
//! copied, so malformed input is an error, never a panic or an oversized
//! allocation (contracts/fast-lz-format.md, FR-005).

use crate::format::{BlockHeader, END_MARKER, FrameHeader, corrupt};
use crush_core::error::{PluginError, Result};
use std::sync::atomic::{AtomicBool, Ordering};

/// Decode a whole `fast-lz` payload.
///
/// `limit`: when `Some(n)`, the total decoded length must be exactly `n` and
/// decoding stops as soon as a block would exceed it. When `None`, the caller
/// checks the total (crush-core compares it with the outer header).
/// Memory grows block by block, never by a declared total up front.
pub(crate) fn decode_frame(
    payload: &[u8],
    limit: Option<usize>,
    cancel: Option<&AtomicBool>,
) -> Result<Vec<u8>> {
    let frame = FrameHeader::parse(payload)?;
    let block_size = u32::try_from(frame.block_size()).map_err(|_| corrupt("block size"))?;
    let mut pos = crate::format::FRAME_HEADER_SIZE;
    let mut out = Vec::new();

    loop {
        let rest = payload.get(pos..).unwrap_or_default();
        let Some(h) = BlockHeader::parse(rest, block_size)? else {
            pos += END_MARKER.len();
            break;
        };
        pos += crate::format::BLOCK_HEADER_SIZE;
        let clen = h.compressed_len as usize;
        let ulen = h.uncompressed_len as usize;
        let block = payload
            .get(pos..pos + clen)
            .ok_or_else(|| corrupt("block payload truncated"))?;
        pos += clen;
        if let Some(n) = limit
            && out.len() + ulen > n
        {
            return Err(corrupt("decoded size exceeds the expected length"));
        }
        if h.stored {
            out.extend_from_slice(block);
        } else {
            decode_block(block, ulen, &mut out)?;
        }
        if cancel.is_some_and(|c| c.load(Ordering::Acquire)) {
            return Err(PluginError::Cancelled.into());
        }
    }

    if pos != payload.len() {
        return Err(corrupt("trailing bytes after the end marker"));
    }
    if let Some(n) = limit
        && out.len() != n
    {
        return Err(corrupt(format!(
            "decoded {} bytes, expected {n}",
            out.len()
        )));
    }
    Ok(out)
}

/// Append exactly `ulen` decoded bytes of one compressed block to `out`.
fn decode_block(src: &[u8], ulen: usize, out: &mut Vec<u8>) -> Result<()> {
    let start = out.len();
    let end = start + ulen;
    out.reserve(ulen);
    let mut i = 0usize;

    loop {
        let token = *src
            .get(i)
            .ok_or_else(|| corrupt("missing sequence token"))?;
        i += 1;

        let mut lit = usize::from(token >> 4);
        if lit == 15 {
            lit += read_length_ext(src, &mut i)?;
        }
        let lit_end = i
            .checked_add(lit)
            .ok_or_else(|| corrupt("literal length overflow"))?;
        let literals = src
            .get(i..lit_end)
            .ok_or_else(|| corrupt("literals run past the block"))?;
        if out.len() + lit > end {
            return Err(corrupt("literals exceed the block's declared size"));
        }
        out.extend_from_slice(literals);
        i = lit_end;

        if i == src.len() {
            break; // final, literals-only sequence
        }

        let off = src
            .get(i..i + 2)
            .map(|b| usize::from(u16::from_le_bytes([b[0], b[1]])))
            .ok_or_else(|| corrupt("truncated match offset"))?;
        i += 2;
        if off == 0 || off > out.len() - start {
            return Err(corrupt(format!("match offset {off} out of range")));
        }

        let mut mlen = usize::from(token & 15) + 4;
        if token & 15 == 15 {
            mlen += read_length_ext(src, &mut i)?;
        }
        if out.len() + mlen > end {
            return Err(corrupt("match exceeds the block's declared size"));
        }
        copy_match(out, off, mlen);
    }

    if out.len() != end {
        return Err(corrupt(format!(
            "block decoded to {} bytes, declared {ulen}",
            out.len() - start
        )));
    }
    Ok(())
}

/// Read a 255-continued length extension at `src[*i..]`.
fn read_length_ext(src: &[u8], i: &mut usize) -> Result<usize> {
    let mut n = 0usize;
    loop {
        let b = *src
            .get(*i)
            .ok_or_else(|| corrupt("length extension runs past the block"))?;
        *i += 1;
        n += usize::from(b);
        if b != 255 {
            return Ok(n);
        }
    }
}

/// Append `len` bytes copied from `off` bytes back. Overlapping copies
/// (`off < len`) repeat the pattern, doubling the copied span each step.
/// The caller has checked `0 < off <= out.len()`.
fn copy_match(out: &mut Vec<u8>, off: usize, len: usize) {
    let from = out.len() - off;
    if off >= len {
        out.extend_from_within(from..from + len);
        return;
    }
    let mut remaining = len;
    while remaining > 0 {
        let n = remaining.min(out.len() - from);
        out.extend_from_within(from..from + n);
        remaining -= n;
    }
}
