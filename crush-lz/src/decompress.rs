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
/// Memory is reserved from `limit` only up to what `payload` can expand to,
/// so a forged total cannot force a huge allocation.
pub(crate) fn decode_frame(
    payload: &[u8],
    limit: Option<usize>,
    cancel: Option<&AtomicBool>,
) -> Result<Vec<u8>> {
    let frame = FrameHeader::parse(payload)?;
    let block_size = u32::try_from(frame.block_size()).map_err(|_| corrupt("block size"))?;
    let mut pos = crate::format::FRAME_HEADER_SIZE;
    // Reserve the expected size up front (no regrowth copies), but never
    // more than the payload could expand to: a sequence of at most 3 bytes
    // per 255 output bytes, so 256 bytes out per byte in is a safe ceiling.
    let mut out =
        Vec::with_capacity(limit.map_or(0, |n| n.min(payload.len().saturating_mul(256)) + WILD));

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

/// Bytes a fast copy may write past the end of a literal run or match. Each
/// block is decoded into `ulen + WILD` bytes so fixed-size copies never need
/// a length check; the slack is truncated afterwards and anything written
/// past a sequence's end is overwritten by the next one.
const WILD: usize = 16;

/// Append exactly `ulen` decoded bytes of one compressed block to `out`.
fn decode_block(src: &[u8], ulen: usize, out: &mut Vec<u8>) -> Result<()> {
    let start = out.len();
    out.resize(start + ulen + WILD, 0);
    let result = decode_into(src, ulen, &mut out[start..]);
    out.truncate(start + ulen);
    result
}

/// Decode one block into `dst` (`ulen + WILD` bytes). Matches never reach
/// before the start of `dst`: offsets are checked against `op`.
fn decode_into(src: &[u8], ulen: usize, dst: &mut [u8]) -> Result<()> {
    let mut i = 0usize;
    let mut op = 0usize;

    loop {
        // Common sequences first, in a tight loop of their own.
        (i, op) = shortcuts(src, dst, i, op);

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
        if lit_end > src.len() {
            return Err(corrupt("literals run past the block"));
        }
        if lit > ulen - op {
            return Err(corrupt("literals exceed the block's declared size"));
        }
        if lit <= WILD && i + WILD <= src.len() {
            // op + lit <= ulen, so op + WILD <= dst.len()
            dst[op..op + WILD].copy_from_slice(&src[i..i + WILD]);
        } else {
            dst[op..op + lit].copy_from_slice(&src[i..lit_end]);
        }
        op += lit;
        i = lit_end;

        if i == src.len() {
            break; // final, literals-only sequence
        }

        let off = src
            .get(i..i + 2)
            .map(|b| usize::from(u16::from_le_bytes([b[0], b[1]])))
            .ok_or_else(|| corrupt("truncated match offset"))?;
        i += 2;
        if off == 0 || off > op {
            return Err(corrupt(format!("match offset {off} out of range")));
        }

        let mut mlen = usize::from(token & 15) + 4;
        if token & 15 == 15 {
            mlen += read_length_ext(src, &mut i)?;
        }
        if mlen > ulen - op {
            return Err(corrupt("match exceeds the block's declared size"));
        }
        copy_match(dst, op, off, mlen);
        op += mlen;
    }

    if op != ulen {
        return Err(corrupt(format!(
            "block decoded to {op} bytes, declared {ulen}"
        )));
    }
    Ok(())
}

/// Decode the common sequence shape (about 87% of sequences on real files)
/// from `src[i..]` into `dst[op..]`, and keep going while the next one has
/// it too: under 15 literals, a match of at most 18 bytes at an offset of
/// at least `WILD`, and at least 32 bytes of the block left to fill, so
/// every copy is a fixed 16 bytes and needs no further length check.
/// Returns the positions of the first sequence it did not decode.
///
/// A sequence that starts 17 or more bytes before the end of the block is
/// never the final, literals-only one (that would need 15+ literals), so
/// its offset is always present. Kept out of line so that its state stays
/// in registers.
#[inline(never)]
fn shortcuts(src: &[u8], dst: &mut [u8], mut i: usize, mut op: usize) -> (usize, usize) {
    // dst.len() == ulen + WILD; op <= last <=> op + 32 <= ulen
    let Some(last) = dst.len().checked_sub(WILD + 32) else {
        return (i, op);
    };
    while op <= last {
        let Some(s) = src.get(i..).and_then(<[u8]>::first_chunk::<17>) else {
            break;
        };
        let lit = usize::from(s[0] >> 4);
        let ml = usize::from(s[0] & 15);
        if lit >= 15 || ml >= 15 {
            break;
        }
        let mop = op + lit;
        let off = usize::from(u16::from_le_bytes([s[1 + lit], s[2 + lit]]));
        if off < WILD || off > mop {
            break;
        }
        let Some(lits) = s[1..].first_chunk::<WILD>() else {
            break;
        };
        // op + 32 <= ulen and lit <= 14: all three copies end within
        // op + 46 <= ulen + WILD.
        dst[op..op + WILD].copy_from_slice(lits);
        let from = mop - off;
        let (done, rest) = dst.split_at_mut(mop);
        rest[..WILD].copy_from_slice(&done[from..from + WILD]);
        if ml > WILD - 4 {
            let (done, rest) = dst.split_at_mut(mop + WILD);
            rest[..WILD].copy_from_slice(&done[from + WILD..from + 2 * WILD]);
        }
        op = mop + ml + 4;
        i += 3 + lit;
    }
    (i, op)
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

/// Write `len` bytes at `dst[op..]` copied from `off` bytes back, in
/// fixed-size steps (up to `WILD - 1` bytes of overshoot, which `dst`'s
/// slack absorbs). The caller has checked `0 < off <= op` and
/// `op + len + WILD <= dst.len()`.
#[inline]
fn copy_match(dst: &mut [u8], op: usize, off: usize, len: usize) {
    if off >= WILD {
        // Each step reads bytes that are already final (they end at or
        // before the step's destination).
        let mut k = 0;
        while k < len {
            let (done, rest) = dst.split_at_mut(op + k);
            let from = op + k - off;
            rest[..WILD].copy_from_slice(&done[from..from + WILD]);
            k += WILD;
        }
        return;
    }
    if off >= 8 {
        // Steps of 8 read only final bytes when the offset is at least 8.
        let mut k = 0;
        while k < len {
            let (done, rest) = dst.split_at_mut(op + k);
            let from = op + k - off;
            rest[..8].copy_from_slice(&done[from..from + 8]);
            k += 8;
        }
        return;
    }
    // Period under 8: the output repeats with period `off`, hence also with
    // any multiple `d` of it. Write the first `d >= 8` bytes one at a time,
    // then copy 8-byte steps from `d` back.
    let d = off * 8usize.div_ceil(off);
    let head = len.min(d);
    for k in op..op + head {
        dst[k] = dst[k - off];
    }
    let mut k = head;
    while k < len {
        let (done, rest) = dst.split_at_mut(op + k);
        let from = op + k - d;
        rest[..8].copy_from_slice(&done[from..from + 8]);
        k += 8;
    }
}
