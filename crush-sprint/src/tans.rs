//! tANS tables and coding, for the sequence fields.
//!
//! Tables have a fixed size and every index is masked, so a corrupt state can
//! decode the wrong symbol but cannot reach outside a table.

use crate::bits::{BitReader, BitWriter};
use crate::format::corrupt;
use crush_core::error::Result;

/// Maximum table log for literal lengths.
pub const MAX_LOG_LITERAL_LENGTH: u8 = 9;
/// Maximum table log for offsets.
pub const MAX_LOG_OFFSET: u8 = 8;
/// Maximum table log for match lengths.
pub const MAX_LOG_MATCH_LENGTH: u8 = 9;

/// The smallest table log. Below this the spread step can repeat itself.
pub const MIN_LOG: u8 = 5;
/// Symbols a table can hold.
pub const MAX_SYMBOLS: usize = 64;

const MAX_TABLE: usize = 1 << 9;
const STATE_MASK: usize = MAX_TABLE - 1;
const SYMBOL_MASK: usize = MAX_SYMBOLS - 1;

/// Scale `histogram` to sum to `2^log`. A rare symbol gets -1, which holds one
/// slot at the top of the table.
///
/// # Errors
///
/// `CorruptedData` if the histogram is empty, has too many symbols for the
/// table, or `log` is out of range.
#[allow(clippy::cast_possible_truncation)]
pub fn normalize_counts(histogram: &[u32], log: u8) -> Result<Vec<i16>> {
    let size = 1i64 << log;
    let total: i64 = histogram.iter().map(|&c| i64::from(c)).sum();
    let used = histogram.iter().filter(|&&c| c != 0).count() as i64;
    if !(MIN_LOG..=9).contains(&log) || total == 0 || used > size {
        return Err(corrupt("cannot normalize counts"));
    }
    let mut out = vec![0i16; histogram.len()];
    let mut sum = 0i64;
    for (slot, &c) in out.iter_mut().zip(histogram) {
        if c != 0 {
            let scaled = (i64::from(c) * size + total / 2) / total;
            *slot = if i64::from(c) * size < total {
                -1
            } else {
                scaled.max(1) as i16
            };
            sum += i64::from((*slot).max(1));
        }
    }
    // Rounding error goes to the largest counts, which feel it least.
    while sum != size {
        let mut best = None;
        for (i, &c) in out.iter().enumerate() {
            let can_move = c > 1 || (sum < size && c > 0);
            if can_move && best.is_none_or(|b: usize| c > out[b]) {
                best = Some(i);
            }
        }
        let Some(best) = best else {
            // Only -1 entries left: promote one and try again.
            let lowest = out.iter().position(|&c| c == -1);
            let Some(lowest) = lowest else {
                return Err(corrupt("cannot normalize counts"));
            };
            out[lowest] = 1;
            continue;
        };
        let step = ((sum - size).abs() / 2).clamp(1, i64::from(out[best]).max(2) - 1);
        if sum > size {
            out[best] -= step as i16;
            sum -= step;
        } else {
            out[best] += step as i16;
            sum += step;
        }
    }
    Ok(out)
}

/// Lay symbols out across the table. Shared by both directions, so an encoder
/// and a decoder of the same counts agree.
fn spread(normalized: &[i16], log: u8, max_log: u8, table: &mut [u8; MAX_TABLE]) -> Result<()> {
    if !(MIN_LOG..=max_log.min(9)).contains(&log) || normalized.len() > MAX_SYMBOLS {
        return Err(corrupt("table log out of range"));
    }
    let size = 1usize << log;
    let mut sum = 0usize;
    for &c in normalized {
        if c < -1 {
            return Err(corrupt("normalized count below -1"));
        }
        sum += c.max(0) as usize + usize::from(c == -1);
    }
    if sum != size {
        return Err(corrupt("normalized counts do not sum to the table size"));
    }
    let mask = size - 1;
    let mut high = size - 1;
    for (symbol, &c) in normalized.iter().enumerate() {
        if c == -1 {
            table[high] = symbol as u8;
            high = high.saturating_sub(1);
        }
    }
    let step = (size >> 1) + (size >> 3) + 3;
    let mut position = 0;
    for (symbol, &c) in normalized.iter().enumerate() {
        for _ in 0..c.max(0) {
            table[position] = symbol as u8;
            position = (position + step) & mask;
            while position > high {
                position = (position + step) & mask;
            }
        }
    }
    if position == 0 {
        Ok(())
    } else {
        Err(corrupt("normalized counts do not fill the table"))
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct SymbolTransform {
    delta_bits: u32,
    delta_state: i32,
}

/// Encoder state: the next table position.
#[derive(Debug, Clone, Copy)]
pub struct EncodeState(u32);

/// Encodes one field.
#[derive(Debug, Clone)]
pub struct EncodeTable {
    states: Box<[u16; MAX_TABLE]>,
    symbols: [SymbolTransform; MAX_SYMBOLS],
    log: u8,
}

impl EncodeTable {
    /// A table holding nothing yet.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            states: Box::new([0; MAX_TABLE]),
            symbols: [SymbolTransform::default(); MAX_SYMBOLS],
            log: 0,
        }
    }

    /// Build from normalized counts.
    ///
    /// # Errors
    ///
    /// `CorruptedData` if the counts or `log` are not valid.
    pub fn new(normalized: &[i16], log: u8, max_log: u8) -> Result<Self> {
        let mut table = Self::empty();
        table.build(normalized, log, max_log)?;
        Ok(table)
    }

    /// Rebuild in place.
    ///
    /// # Errors
    ///
    /// `CorruptedData` if the counts or `log` are not valid.
    pub fn build(&mut self, normalized: &[i16], log: u8, max_log: u8) -> Result<()> {
        let mut layout = [0u8; MAX_TABLE];
        spread(normalized, log, max_log, &mut layout)?;
        let size = 1usize << log;
        let mut cumulative = [0usize; MAX_SYMBOLS + 1];
        for (i, &c) in normalized.iter().enumerate() {
            cumulative[i + 1] = cumulative[i] + c.max(1) as usize - usize::from(c == 0);
        }
        for (u, &symbol) in layout.iter().take(size).enumerate() {
            let slot = &mut cumulative[usize::from(symbol) & SYMBOL_MASK];
            self.states[*slot & STATE_MASK] = (size + u) as u16;
            *slot += 1;
        }
        let mut total = 0i32;
        for (transform, &c) in self.symbols.iter_mut().zip(normalized) {
            let c = i32::from(c.max(i16::from(c == -1)));
            if c == 0 {
                *transform = SymbolTransform::default();
            } else if c == 1 {
                *transform = SymbolTransform {
                    delta_bits: (u32::from(log) << 16) - (1 << log),
                    delta_state: total - 1,
                };
                total += 1;
            } else {
                let bits = u32::from(log) - (c - 1).ilog2();
                *transform = SymbolTransform {
                    delta_bits: (bits << 16) - ((c as u32) << bits),
                    delta_state: total - c,
                };
                total += c;
            }
        }
        self.log = log;
        Ok(())
    }

    /// A table with one symbol, which costs no bits.
    pub fn set_rle(&mut self, symbol: u8) {
        self.states[0] = 0;
        self.symbols[usize::from(symbol) & SYMBOL_MASK] = SymbolTransform::default();
        self.log = 0;
    }

    /// Start from the last symbol of a run.
    #[inline]
    #[must_use]
    pub fn init(&self, symbol: u8) -> EncodeState {
        let t = self.symbols[usize::from(symbol) & SYMBOL_MASK];
        let bits = t.delta_bits.wrapping_add(1 << 15) >> 16;
        let value = (bits << 16).wrapping_sub(t.delta_bits);
        let index = ((value >> bits) as i32).wrapping_add(t.delta_state);
        EncodeState(u32::from(self.states[index as usize & STATE_MASK]))
    }

    /// Encode `symbol`, at most 9 bits.
    #[inline(always)]
    pub fn step(&self, state: &mut EncodeState, symbol: u8, writer: &mut BitWriter) {
        let t = self.symbols[usize::from(symbol) & SYMBOL_MASK];
        let bits = state.0.wrapping_add(t.delta_bits) >> 16;
        writer.add(u64::from(state.0) & ((1u64 << bits) - 1), bits);
        let index = ((state.0 >> bits) as i32).wrapping_add(t.delta_state);
        state.0 = u32::from(self.states[index as usize & STATE_MASK]);
    }

    /// Write the final state.
    #[inline]
    pub fn finish(&self, state: EncodeState, writer: &mut BitWriter) {
        let log = u32::from(self.log);
        writer.write(u64::from(state.0) & ((1u64 << log) - 1), log);
    }

    /// Encode `symbols` so that [`DecodeTable::decode`] returns them in order.
    pub fn encode(&self, symbols: &[u8], writer: &mut BitWriter) {
        let Some((&last, rest)) = symbols.split_last() else {
            return;
        };
        let mut state = self.init(last);
        for &symbol in rest.iter().rev() {
            self.step(&mut state, symbol, writer);
            writer.flush();
        }
        self.finish(state, writer);
    }
}

/// One decoding step.
#[derive(Debug, Clone, Copy, Default)]
pub struct DecodeEntry {
    /// Added to the bits read to give the next state.
    pub base: u16,
    /// The symbol at this state.
    pub symbol: u8,
    /// Bits to read for the next state.
    pub bits: u8,
}

/// Decodes one field.
#[derive(Debug, Clone)]
pub struct DecodeTable {
    entries: Box<[DecodeEntry; MAX_TABLE]>,
    log: u8,
}

impl DecodeTable {
    /// A table holding nothing yet.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            entries: Box::new([DecodeEntry::default(); MAX_TABLE]),
            log: 0,
        }
    }

    /// Build from normalized counts.
    ///
    /// # Errors
    ///
    /// `CorruptedData` if the counts sum wrongly, a count is below -1, or
    /// `log` is outside [`MIN_LOG`]..=`max_log`.
    pub fn new(normalized: &[i16], log: u8, max_log: u8) -> Result<Self> {
        let mut table = Self::empty();
        table.build(normalized, log, max_log)?;
        Ok(table)
    }

    /// Rebuild in place.
    ///
    /// # Errors
    ///
    /// As for [`Self::new`].
    pub fn build(&mut self, normalized: &[i16], log: u8, max_log: u8) -> Result<()> {
        let mut layout = [0u8; MAX_TABLE];
        spread(normalized, log, max_log, &mut layout)?;
        let size = 1u32 << log;
        let mut next = [0u32; MAX_SYMBOLS];
        for (slot, &c) in next.iter_mut().zip(normalized) {
            *slot = i32::from(c).unsigned_abs();
        }
        for (entry, &symbol) in self.entries.iter_mut().zip(layout.iter()).take(1 << log) {
            let state = &mut next[usize::from(symbol) & SYMBOL_MASK];
            let bits = u32::from(log) - state.ilog2();
            *entry = DecodeEntry {
                base: ((*state << bits) - size) as u16,
                symbol,
                bits: bits as u8,
            };
            *state += 1;
        }
        self.log = log;
        Ok(())
    }

    /// A table with one symbol, which reads no bits.
    pub fn set_rle(&mut self, symbol: u8) {
        self.entries[0] = DecodeEntry {
            base: 0,
            symbol,
            bits: 0,
        };
        self.log = 0;
    }

    /// Bits in the opening state.
    #[inline]
    #[must_use]
    pub fn log(&self) -> u32 {
        u32::from(self.log)
    }

    /// The entry at `state`.
    #[inline(always)]
    #[must_use]
    pub fn entry(&self, state: usize) -> DecodeEntry {
        self.entries[state & STATE_MASK]
    }

    /// Decode `count` symbols.
    ///
    /// # Errors
    ///
    /// `CorruptedData` if the stream runs out.
    pub fn decode(&self, reader: &mut BitReader<'_>, count: usize) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        if count == 0 {
            return Ok(out);
        }
        let mut state = reader.read(self.log()) as usize;
        for i in 0..count {
            let entry = self.entry(state);
            out.push(entry.symbol);
            if i + 1 < count {
                state = usize::from(entry.base) + reader.read(u32::from(entry.bits)) as usize;
            }
            if reader.overflowed() {
                return Err(corrupt("tANS stream ran out"));
            }
        }
        Ok(out)
    }
}

/// Bits for the next count: it is at most `remaining`, and -1 and 0 need codes.
fn count_bits(remaining: u32) -> u32 {
    (remaining + 1).ilog2() + 1
}

/// Append a table description: table log, symbol count, stream size, then a
/// backward stream of `count + 1` fields, each as wide as the slots still free
/// require.
///
/// # Errors
///
/// `CorruptedData` if the counts are too many for one byte of stream size.
#[allow(clippy::cast_possible_truncation)]
pub fn write_counts(normalized: &[i16], log: u8, out: &mut Vec<u8>) -> Result<()> {
    let used = normalized
        .iter()
        .rposition(|&c| c != 0)
        .map_or(0, |p| p + 1);
    let mut fields = [(0u64, 0u32); MAX_SYMBOLS];
    let mut remaining = 1u32 << log;
    for (field, &c) in fields.iter_mut().zip(&normalized[..used]) {
        *field = ((i64::from(c) + 1) as u64, count_bits(remaining));
        remaining -= u32::from(c.unsigned_abs());
    }
    let mut writer = BitWriter::new();
    for &(value, bits) in fields[..used].iter().rev() {
        writer.write(value, bits);
    }
    let stream = writer.finish();
    let size = u8::try_from(stream.len()).map_err(|_| corrupt("table description too large"))?;
    out.extend_from_slice(&[log, used as u8, size]);
    out.extend_from_slice(&stream);
    Ok(())
}

/// Read a table description into `normalized`. Returns the table log, the
/// symbol count and the input that follows.
///
/// # Errors
///
/// `CorruptedData` if it is truncated or its counts overrun the table.
pub fn read_counts<'a>(
    input: &'a [u8],
    max_symbols: usize,
    normalized: &mut [i16; MAX_SYMBOLS],
) -> Result<(u8, usize, &'a [u8])> {
    let [log, used, size, rest @ ..] = input else {
        return Err(corrupt("truncated table description"));
    };
    let used = usize::from(*used);
    if !(MIN_LOG..=9).contains(log) || used > max_symbols.min(MAX_SYMBOLS) {
        return Err(corrupt("bad table description"));
    }
    let (stream, rest) = rest
        .split_at_checked(usize::from(*size))
        .ok_or_else(|| corrupt("truncated table description"))?;
    let mut reader = BitReader::new(stream)?;
    let mut remaining = 1i32 << *log;
    for slot in normalized.iter_mut().take(used) {
        let value = reader.read(count_bits(remaining.max(0) as u32));
        let count = value as i32 - 1;
        if count > remaining {
            return Err(corrupt("table counts overrun the table"));
        }
        *slot = count as i16;
        remaining -= count.abs();
    }
    if !reader.is_exhausted() || remaining != 0 {
        return Err(corrupt("table counts do not sum to the table size"));
    }
    Ok((*log, used, rest))
}
