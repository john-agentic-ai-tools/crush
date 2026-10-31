//! Validating `fast-lz` decoder.
//!
//! Every length and offset is checked against the payload, the bytes already
//! produced in the block, and the block's declared size before anything is
//! copied, so malformed input is an error, never a panic or an oversized
//! allocation (contracts/fast-lz-format.md, FR-005).

use crate::format::{BlockHeader, END_MARKER, FrameHeader, corrupt};
use crush_core::error::{PluginError, Result};
use std::io::Write;
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
    // One zeroed allocation up front: a large one comes from fresh pages, so
    // nothing is filled, where a `resize` per block would zero every byte.
    let size = limit.map_or_else(|| declared_len(payload), |n| initial_len(n, payload.len()));
    let mut out = output_buffer(size);
    let mut at = 0;
    let total = for_each_block(payload, limit, cancel, |block, stored, ulen| {
        decode_at(block, stored, ulen, &mut out, at)?;
        at += ulen;
        Ok(())
    })?;
    out.truncate(total);
    Ok(out)
}

/// A zeroed buffer of `size` bytes, or an empty one if memory is refused:
/// `size` can come from forged lengths, and a failed `vec!` aborts the process.
/// `decode_at` grows an empty buffer block by block, after each header passes
/// validation, so forged input ends in a decode error instead.
fn output_buffer(size: usize) -> Vec<u8> {
    // Asking first keeps the zeroed allocation below, which `reserve` plus
    // `resize` would fill byte by byte.
    if Vec::<u8>::new().try_reserve_exact(size).is_err() {
        return Vec::new();
    }
    vec![0; size]
}

/// Output buffer size for `expected` bytes from `payload_len` bytes: never
/// more than the payload could expand to, so a forged length can't force a
/// huge allocation. A sequence takes at most 3 bytes per 255 output bytes,
/// so 256 bytes out per byte in is a safe ceiling.
fn initial_len(expected: usize, payload_len: usize) -> usize {
    expected
        .min(payload_len.saturating_mul(256))
        .saturating_add(WILD)
}

/// Largest buffer sized from block headers alone, per payload byte. Headers
/// are forgeable, so the limit is tighter than [`initial_len`]'s ceiling;
/// output beyond it grows as blocks arrive.
const MAX_DECLARED_EXPANSION: usize = 16;

/// Output buffer size for a payload with no expected length: what its block
/// headers declare, up to [`MAX_DECLARED_EXPANSION`]. Malformed headers end
/// the sum, and decoding reports them.
fn declared_len(payload: &[u8]) -> usize {
    let mut declared = 0usize;
    let walked = for_each_block(payload, None, None, |_, _, ulen| {
        declared += ulen;
        Ok(())
    });
    drop(walked);
    declared
        .min(payload.len().saturating_mul(MAX_DECLARED_EXPANSION))
        .saturating_add(WILD)
}

/// Decode one block to `out[at..at + ulen]`, growing `out` if it's short.
///
/// The destination is cut to exactly `ulen + WILD` bytes: the decoder takes
/// its bounds from that length, so a longer slice would let a corrupt block
/// run past its declared size.
fn decode_at(block: &[u8], stored: bool, ulen: usize, out: &mut Vec<u8>, at: usize) -> Result<()> {
    if stored {
        // Appending when short avoids zeroing bytes about to be overwritten.
        if let Some(dst) = out.get_mut(at..at + ulen) {
            dst.copy_from_slice(block);
        } else {
            out.truncate(at);
            out.extend_from_slice(block);
        }
        return Ok(());
    }
    let end = at + ulen + WILD;
    if out.len() < end {
        out.resize(end, 0);
    }
    decode_into(block, ulen, &mut out[at..end])
}

/// Decode a whole `fast-lz` payload into `w`, one block at a time (at most
/// one block of memory). Returns the number of bytes written; the caller
/// checks it against the expected total. On error, `w` may hold a prefix.
pub(crate) fn decode_frame_to(
    payload: &[u8],
    cancel: Option<&AtomicBool>,
    w: &mut dyn Write,
) -> Result<u64> {
    // Kept at its longest length between blocks, so that it's zeroed only as
    // it grows. The decoder never reads a byte it didn't write for the
    // current block, and only `ulen` bytes of a block that decoded are output.
    let mut scratch = Vec::new();
    let total = for_each_block(payload, None, cancel, |block, stored, ulen| {
        if stored {
            w.write_all(block)?;
        } else {
            decode_at(block, false, ulen, &mut scratch, 0)?;
            w.write_all(&scratch[..ulen])?;
        }
        Ok(())
    })?;
    Ok(total as u64)
}

/// Walk the frame's blocks, validating headers, lengths, `limit` and the
/// end of the payload, and hand each block to `f` as `(bytes, stored,
/// uncompressed_len)`. Returns the total uncompressed length.
fn for_each_block(
    payload: &[u8],
    limit: Option<usize>,
    cancel: Option<&AtomicBool>,
    mut f: impl FnMut(&[u8], bool, usize) -> Result<()>,
) -> Result<usize> {
    let frame = FrameHeader::parse(payload)?;
    let block_size = u32::try_from(frame.block_size()).map_err(|_| corrupt("block size"))?;
    let mut pos = crate::format::FRAME_HEADER_SIZE;
    let mut total = 0usize;

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
            && total + ulen > n
        {
            return Err(corrupt("decoded size exceeds the expected length"));
        }
        f(block, h.stored, ulen)?;
        total += ulen;
        if cancel.is_some_and(|c| c.load(Ordering::Acquire)) {
            return Err(PluginError::Cancelled.into());
        }
    }

    if pos != payload.len() {
        return Err(corrupt("trailing bytes after the end marker"));
    }
    if let Some(n) = limit
        && total != n
    {
        return Err(corrupt(format!("decoded {total} bytes, expected {n}")));
    }
    Ok(total)
}

/// Bytes a fast copy may write past the end of a literal run or match. Each
/// block is decoded into `ulen + WILD` bytes so fixed-size copies never need
/// a length check; anything written past a sequence's end is overwritten by
/// the next one, and the slack is never output.
const WILD: usize = 16;

/// Decode one block into `dst` (`ulen + WILD` bytes). Matches never reach
/// before the start of `dst`: offsets are checked against `op`.
fn decode_into(src: &[u8], ulen: usize, dst: &mut [u8]) -> Result<()> {
    debug_assert_eq!(dst.len(), ulen + WILD);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compress::Compressor;
    use proptest::collection::vec;
    use proptest::prelude::*;

    #[test]
    fn initial_len_is_capped_by_the_payload() {
        assert_eq!(initial_len(0, 100), WILD);
        assert_eq!(initial_len(1000, 100), 1000 + WILD);
        assert_eq!(initial_len(1 << 40, 100), 25_600 + WILD);
        assert_eq!(initial_len(usize::MAX, 100), 25_600 + WILD);
        assert_eq!(initial_len(usize::MAX, usize::MAX), usize::MAX);
    }

    #[test]
    fn output_buffer_is_empty_when_memory_is_refused() {
        // More than any allocator grants: with `vec!` this would abort.
        assert_eq!(output_buffer(1 << 62), Vec::<u8>::new());
        assert_eq!(output_buffer(1000), vec![0; 1000]);
    }

    #[test]
    fn declared_len_sums_headers_up_to_the_limit() {
        let frame = crate::format::FrameHeader::new(20).expect("header");
        let block = |ulen: u32| {
            crate::format::BlockHeader {
                compressed_len: 1,
                stored: false,
                uncompressed_len: ulen,
            }
            .to_bytes()
        };
        let mut payload = frame.to_bytes().to_vec();
        payload.extend_from_slice(&block(10));
        payload.push(0);
        assert_eq!(declared_len(&payload), 10 + WILD, "truncated frame");

        // Blocks of a few bytes that each declare 1 MiB.
        for _ in 0..1000 {
            payload.extend_from_slice(&block(1 << 20));
            payload.push(0);
        }
        assert_eq!(declared_len(&payload), 16 * payload.len() + WILD);
        assert_eq!(declared_len(b"not a frame"), WILD);
    }

    fn decode_over(src: &[u8], ulen: usize, fill: u8) -> Option<Vec<u8>> {
        let mut dst = vec![fill; ulen + WILD];
        decode_into(src, ulen, &mut dst).ok()?;
        dst.truncate(ulen);
        Some(dst)
    }

    proptest! {
        /// Reusing a buffer between blocks is safe only if the decoder never
        /// lets its earlier contents affect the result.
        #[test]
        fn decode_into_ignores_buffer_contents(
            data in vec(0u8..4, 1..2000),
            flips in vec((any::<prop::sample::Index>(), any::<u8>()), 0..4),
            extra in 0usize..40,
        ) {
            let mut src = vec![0; data.len() + WILD];
            let n = Compressor::new().compress_block(&data, &mut src);
            prop_assume!(n.is_some());
            src.truncate(n.unwrap_or(0));
            for (at, byte) in flips {
                let i = at.index(src.len());
                src[i] = byte;
            }
            let ulen = data.len() + extra;
            for ulen in [data.len(), ulen] {
                let zeroed = decode_over(&src, ulen, 0);
                prop_assert_eq!(&zeroed, &decode_over(&src, ulen, 0xFF));
                prop_assert_eq!(&zeroed, &decode_over(&src, ulen, 0xA5));
            }
        }
    }
}
