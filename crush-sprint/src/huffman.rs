//! Canonical Huffman coding of literals, in four streams.
//!
//! The decoder table has a fixed size and is indexed by exactly 11 bits, so
//! its lookups need no bounds check and no code length can reach outside it.

use crate::bits::{BitReader, BitWriter};
use crush_core::error::{Result, ValidationError};

/// The longest code.
pub const MAX_CODE_LENGTH: u8 = 11;

const TABLE_SIZE: usize = 1 << MAX_CODE_LENGTH;
const SYMBOLS: usize = 256;
/// The largest literal run in one segment.
const MAX_REGENERATED: usize = 131_072;
/// Stream size table: three `u16`, the fourth stream is implied.
const JUMP_TABLE: usize = 6;
/// Symbols decoded between refills: 5 x 11 + 7 bits fit in the container.
const SYMBOLS_PER_REFILL: usize = 5;

fn corrupt(what: &str) -> crush_core::error::CrushError {
    ValidationError::CorruptedData(what.into()).into()
}

/// Code lengths for `histogram`, complete and at most [`MAX_CODE_LENGTH`] bits.
///
/// # Errors
///
/// `CorruptedData` with fewer than two symbols present: one symbol has no
/// code, and that case belongs to the RLE mode.
pub fn build_code_lengths(histogram: &[u32; SYMBOLS]) -> Result<[u8; SYMBOLS]> {
    let mut order: Vec<usize> = (0..SYMBOLS).filter(|&s| histogram[s] > 0).collect();
    let n = order.len();
    if n < 2 {
        return Err(corrupt("Huffman needs at least two symbols"));
    }
    order.sort_unstable_by_key(|&s| (histogram[s], s));

    // Two-queue tree over sorted leaves: 0..n are leaves, n.. internal nodes.
    let nodes = 2 * n - 1;
    let mut weight = vec![0u64; nodes];
    let mut parent = vec![0usize; nodes];
    for (slot, &s) in weight.iter_mut().zip(&order) {
        *slot = u64::from(histogram[s]);
    }
    let (mut leaf, mut inner) = (0, n);
    for next in n..nodes {
        let mut pair = [0usize; 2];
        for chosen in &mut pair {
            if leaf < n && (inner >= next || weight[leaf] <= weight[inner]) {
                *chosen = leaf;
                leaf += 1;
            } else {
                *chosen = inner;
                inner += 1;
            }
        }
        weight[next] = weight[pair[0]] + weight[pair[1]];
        parent[pair[0]] = next;
        parent[pair[1]] = next;
    }
    let mut depth = vec![0u32; nodes];
    for i in (0..nodes - 1).rev() {
        depth[i] = depth[parent[i]] + 1;
    }

    let mut lengths: Vec<u8> = depth[..n]
        .iter()
        .map(|&d| d.min(u32::from(MAX_CODE_LENGTH)) as u8)
        .collect();
    limit_lengths(&mut lengths);

    let mut out = [0u8; SYMBOLS];
    for (&s, &l) in order.iter().zip(&lengths) {
        out[s] = l;
    }
    Ok(out)
}

/// Repair the Kraft sum after clamping. `lengths` runs from the rarest symbol
/// to the commonest.
fn limit_lengths(lengths: &mut [u8]) {
    let unit = |l: u8| 1i64 << (MAX_CODE_LENGTH - l);
    let mut excess: i64 = lengths.iter().map(|&l| unit(l)).sum::<i64>() - TABLE_SIZE as i64;
    // Lengthen the longest codes first, the cheapest bits to give up.
    while excess > 0 {
        for target in (1..MAX_CODE_LENGTH).rev() {
            for l in lengths.iter_mut() {
                if excess > 0 && *l == target {
                    *l += 1;
                    excess -= unit(target + 1);
                }
            }
        }
    }
    // Spend what the overshoot freed on the commonest symbols.
    let mut spare = -excess;
    while spare > 0 {
        for l in lengths.iter_mut().rev() {
            if *l > 1 && unit(*l) <= spare {
                spare -= unit(*l);
                *l -= 1;
            }
        }
    }
}

/// Check that `lengths` form a complete code of at most [`MAX_CODE_LENGTH`] bits.
///
/// # Errors
///
/// `CorruptedData` for too many symbols, a length over the limit, or a code
/// that is empty, incomplete or oversubscribed.
pub fn validate_code_lengths(lengths: &[u8]) -> Result<()> {
    if lengths.len() > SYMBOLS {
        return Err(corrupt("too many Huffman symbols"));
    }
    let mut kraft = 0u32;
    for &l in lengths {
        if l > MAX_CODE_LENGTH {
            return Err(corrupt("Huffman code too long"));
        }
        if l != 0 {
            kraft += 1 << (MAX_CODE_LENGTH - l);
        }
    }
    if kraft == TABLE_SIZE as u32 {
        Ok(())
    } else {
        Err(corrupt("Huffman code is not complete"))
    }
}

/// Canonical codes, in symbol order within each length.
fn canonical_codes(lengths: &[u8]) -> [u16; SYMBOLS] {
    let mut count = [0u16; MAX_CODE_LENGTH as usize + 2];
    for &l in lengths {
        count[usize::from(l)] += 1;
    }
    count[0] = 0;
    let mut next = [0u16; MAX_CODE_LENGTH as usize + 2];
    let mut code = 0u16;
    for l in 1..=usize::from(MAX_CODE_LENGTH) {
        code = (code + count[l - 1]) << 1;
        next[l] = code;
    }
    let mut codes = [0u16; SYMBOLS];
    for (slot, &l) in codes.iter_mut().zip(lengths) {
        if l != 0 {
            *slot = next[usize::from(l)];
            next[usize::from(l)] += 1;
        }
    }
    codes
}

/// How `len` symbols divide across four streams: the first three take
/// `ceil(len / 4)` while any remain, and the fourth takes the rest.
fn stream_lengths(len: usize) -> [usize; 4] {
    let each = len.div_ceil(4);
    let a = each.min(len);
    let b = each.min(len - a);
    let c = each.min(len - a - b);
    [a, b, c, len - a - b - c]
}

/// Encodes literals with one code table.
#[derive(Debug, Clone)]
pub struct HuffmanEncoder {
    /// `code | length << 16`, zero for a symbol with no code.
    table: [u32; SYMBOLS],
}

impl HuffmanEncoder {
    /// Build from code lengths.
    ///
    /// # Errors
    ///
    /// `CorruptedData` if the lengths are not a valid code.
    pub fn from_lengths(lengths: &[u8]) -> Result<Self> {
        validate_code_lengths(lengths)?;
        let codes = canonical_codes(lengths);
        let mut table = [0u32; SYMBOLS];
        for ((slot, &code), &l) in table.iter_mut().zip(&codes).zip(lengths) {
            *slot = u32::from(code) | (u32::from(l) << 16);
        }
        Ok(Self { table })
    }

    /// Append four streams and their size table to `out`.
    ///
    /// # Errors
    ///
    /// `CorruptedData` if `literals` is over the segment limit or holds a
    /// symbol with no code.
    pub fn encode_4_streams(&self, literals: &[u8], out: &mut Vec<u8>) -> Result<()> {
        if literals.len() > MAX_REGENERATED {
            return Err(corrupt("too many literals"));
        }
        let start = out.len();
        out.resize(start + JUMP_TABLE, 0);
        let mut rest = literals;
        for (i, len) in stream_lengths(literals.len()).into_iter().enumerate() {
            let (stream, tail) = rest.split_at(len);
            rest = tail;
            let bytes = self.encode_stream(stream)?;
            if i < 3 {
                let size = u16::try_from(bytes.len()).map_err(|_| corrupt("stream too large"))?;
                out[start + 2 * i..start + 2 * i + 2].copy_from_slice(&size.to_le_bytes());
            }
            out.extend_from_slice(&bytes);
        }
        Ok(())
    }

    fn encode_stream(&self, symbols: &[u8]) -> Result<Vec<u8>> {
        // Written last symbol first, because the reader walks backward.
        let mut writer =
            BitWriter::with_capacity(symbols.len() * usize::from(MAX_CODE_LENGTH) / 8 + 8);
        let mut missing = false;
        for group in symbols.rchunks(4) {
            for &s in group.iter().rev() {
                let entry = self.table[usize::from(s)];
                missing |= entry == 0;
                writer.add(u64::from(entry & 0xFFFF), entry >> 16);
            }
            writer.flush();
        }
        if missing {
            return Err(corrupt("literal has no Huffman code"));
        }
        Ok(writer.finish())
    }
}

/// Decodes literals with one code table.
#[derive(Debug, Clone)]
pub struct HuffmanDecoder {
    /// `symbol | length << 8`, indexed by the next 11 bits.
    table: Box<[u16; TABLE_SIZE]>,
}

impl HuffmanDecoder {
    /// A decoder with no table yet.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            table: Box::new([0u16; TABLE_SIZE]),
        }
    }

    /// Build from code lengths.
    ///
    /// # Errors
    ///
    /// `CorruptedData` if the lengths are not a valid code.
    pub fn from_lengths(lengths: &[u8]) -> Result<Self> {
        let mut decoder = Self::empty();
        decoder.rebuild(lengths)?;
        Ok(decoder)
    }

    /// Replace the table.
    ///
    /// # Errors
    ///
    /// `CorruptedData` if the lengths are not a valid code.
    pub fn rebuild(&mut self, lengths: &[u8]) -> Result<()> {
        validate_code_lengths(lengths)?;
        let codes = canonical_codes(lengths);
        for (symbol, (&code, &l)) in codes.iter().zip(lengths).enumerate() {
            if l != 0 {
                let shift = MAX_CODE_LENGTH - l;
                let first = usize::from(code) << shift;
                let entry = symbol as u16 | (u16::from(l) << 8);
                // A valid code keeps this in range; `get_mut` makes that local.
                if let Some(run) = self.table.get_mut(first..first + (1 << shift)) {
                    run.fill(entry);
                }
            }
        }
        Ok(())
    }

    /// Decode four streams into `output`, filling it exactly.
    ///
    /// # Errors
    ///
    /// `CorruptedData` if the stream sizes do not fit `input`, a stream has no
    /// sentinel, or any stream does not end exactly at its start.
    pub fn decode_4_streams(&self, input: &[u8], output: &mut [u8]) -> Result<()> {
        let (jump, streams) = input
            .split_at_checked(JUMP_TABLE)
            .ok_or_else(|| corrupt("missing stream sizes"))?;
        let size = |i: usize| usize::from(u16::from_le_bytes([jump[2 * i], jump[2 * i + 1]]));
        let (s1, streams) = streams
            .split_at_checked(size(0))
            .ok_or_else(|| corrupt("stream past the section"))?;
        let (s2, streams) = streams
            .split_at_checked(size(1))
            .ok_or_else(|| corrupt("stream past the section"))?;
        let (s3, s4) = streams
            .split_at_checked(size(2))
            .ok_or_else(|| corrupt("stream past the section"))?;

        let mut readers = [
            BitReader::new(s1)?,
            BitReader::new(s2)?,
            BitReader::new(s3)?,
            BitReader::new(s4)?,
        ];
        let [la, lb, lc, _] = stream_lengths(output.len());
        let (a, rest) = output.split_at_mut(la);
        let (b, rest) = rest.split_at_mut(lb);
        let (c, d) = rest.split_at_mut(lc);

        self.decode_interleaved(&mut readers, [a, b, c, d]);
        if readers.iter().all(BitReader::is_exhausted) {
            Ok(())
        } else {
            Err(corrupt("literal stream did not end at its start"))
        }
    }

    #[inline]
    fn symbol(&self, reader: &mut BitReader<'_>) -> u8 {
        let entry = self.table[reader.peek_11()];
        reader.consume(u32::from(entry >> 8));
        entry as u8
    }

    fn decode_interleaved(&self, readers: &mut [BitReader<'_>; 4], outputs: [&mut [u8]; 4]) {
        let [ra, rb, rc, rd] = readers;
        let [oa, ob, oc, od] = outputs;
        let rounds = od.len() / SYMBOLS_PER_REFILL;
        let (ha, ta) = oa.split_at_mut(rounds * SYMBOLS_PER_REFILL);
        let (hb, tb) = ob.split_at_mut(rounds * SYMBOLS_PER_REFILL);
        let (hc, tc) = oc.split_at_mut(rounds * SYMBOLS_PER_REFILL);
        let (hd, td) = od.split_at_mut(rounds * SYMBOLS_PER_REFILL);
        let groups = ha
            .as_chunks_mut::<SYMBOLS_PER_REFILL>()
            .0
            .iter_mut()
            .zip(hb.as_chunks_mut::<SYMBOLS_PER_REFILL>().0)
            .zip(hc.as_chunks_mut::<SYMBOLS_PER_REFILL>().0)
            .zip(hd.as_chunks_mut::<SYMBOLS_PER_REFILL>().0);
        for (((a, b), c), d) in groups {
            ra.refill();
            rb.refill();
            rc.refill();
            rd.refill();
            for (((x, y), z), w) in a.iter_mut().zip(b).zip(c).zip(d) {
                *x = self.symbol(ra);
                *y = self.symbol(rb);
                *z = self.symbol(rc);
                *w = self.symbol(rd);
            }
        }
        for (reader, tail) in [(ra, ta), (rb, tb), (rc, tc), (rd, td)] {
            for group in tail.chunks_mut(SYMBOLS_PER_REFILL) {
                reader.refill();
                for slot in group {
                    *slot = self.symbol(reader);
                }
            }
        }
    }
}
