//! The sequences section: code tables, table choice, the bit stream, and
//! running sequences against the output.
//!
//! Length codes are those of RFC 8878. Offset value `v` has code `log2(v)`
//! with that many extra bits; values 1 to 3 select repeat offsets and the rest
//! are `offset + 3`.

use crate::bits::{BitReader, BitWriter};
use crate::format::{corrupt, read_varint, write_varint};
use crate::histogram::entropy_bits;
use crate::matcher::Sequence;
use crate::tans::{
    self, DecodeTable, EncodeState, EncodeTable, MAX_LOG_LITERAL_LENGTH, MAX_LOG_MATCH_LENGTH,
    MAX_LOG_OFFSET, MAX_SYMBOLS, MIN_LOG, normalize_counts,
};
use crush_core::error::Result;

/// Where repeat offsets start in every block.
pub const INITIAL_REPEATS: [u32; 3] = [1, 4, 8];

const LL_CODES: usize = 36;
const ML_CODES: usize = 53;
/// Offsets reach at most 4 `MiB`, so the longest code is 22. The table allows
/// a few more for a future block size.
const OF_CODES: usize = 27;

const MODE_PREDEFINED: u8 = 0;
const MODE_RLE: u8 = 1;
const MODE_EXPLICIT: u8 = 2;
const MODE_REPEAT: u8 = 3;

const LL_DEFAULT: [i16; LL_CODES] = [
    4, 3, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 2, 2, 3, 2, 1, 1, 1, 1, 1,
    -1, -1, -1, -1,
];
const ML_DEFAULT: [i16; ML_CODES] = [
    1, 4, 3, 2, 2, 2, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, -1, -1, -1, -1, -1, -1, -1,
];
const OF_DEFAULT: [i16; 29] = [
    1, 1, 1, 1, 1, 1, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, -1, -1, -1, -1, -1,
];
const LL_DEFAULT_LOG: u8 = 6;
const ML_DEFAULT_LOG: u8 = 6;
const OF_DEFAULT_LOG: u8 = 5;

const fn ll_bits() -> [u8; MAX_SYMBOLS] {
    let tail = [
        1, 1, 1, 1, 2, 2, 3, 3, 4, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,
    ];
    let mut out = [0u8; MAX_SYMBOLS];
    let mut i = 0;
    while i < tail.len() {
        out[16 + i] = tail[i];
        i += 1;
    }
    out
}

const fn ml_bits() -> [u8; MAX_SYMBOLS] {
    let tail = [
        1, 1, 1, 1, 2, 2, 3, 3, 4, 4, 5, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,
    ];
    let mut out = [0u8; MAX_SYMBOLS];
    let mut i = 0;
    while i < tail.len() {
        out[32 + i] = tail[i];
        i += 1;
    }
    out
}

const fn bases(bits: &[u8; MAX_SYMBOLS], first: u32) -> [u32; MAX_SYMBOLS] {
    let mut out = [0u32; MAX_SYMBOLS];
    let mut value = first;
    let mut i = 0;
    while i < MAX_SYMBOLS {
        out[i] = value;
        // Past the last real code the shift stays small: bits are zero there.
        value += 1 << bits[i];
        i += 1;
    }
    out
}

const LL_BITS: [u8; MAX_SYMBOLS] = ll_bits();
const ML_BITS: [u8; MAX_SYMBOLS] = ml_bits();
const LL_BASE: [u32; MAX_SYMBOLS] = bases(&LL_BITS, 0);
const ML_BASE: [u32; MAX_SYMBOLS] = bases(&ML_BITS, 3);

/// Code for each value below 128, searched once at compile time.
const fn small_codes(base: &[u32; MAX_SYMBOLS], first: u32) -> [u8; 128] {
    let mut out = [0u8; 128];
    let mut value = 0;
    let mut code = 0;
    while value < 128 {
        while base[code + 1] <= value as u32 + first {
            code += 1;
        }
        out[value] = code as u8;
        value += 1;
    }
    out
}

const LL_SMALL: [u8; 128] = small_codes(&LL_BASE, 0);
const ML_SMALL: [u8; 128] = small_codes(&ML_BASE, 3);

#[inline]
fn ll_code(length: u32) -> u8 {
    if length < 64 {
        LL_SMALL[length as usize & 127]
    } else {
        (length.ilog2() + 19) as u8
    }
}

#[inline]
fn ml_code(length: u32) -> u8 {
    let v = length - 3;
    if v < 128 {
        ML_SMALL[v as usize & 127]
    } else {
        (v.ilog2() + 36) as u8
    }
}

/// One segment's sequences as field codes and extra bits.
#[derive(Debug, Default)]
struct Codes {
    ll: Vec<u8>,
    of: Vec<u8>,
    ml: Vec<u8>,
    /// Literal length, match length and offset extra bits.
    extra: Vec<[u32; 3]>,
}

impl Codes {
    fn fill(&mut self, sequences: &[Sequence], repeats: &mut [u32; 3]) {
        let n = sequences.len();
        self.ll.resize(n, 0);
        self.of.resize(n, 0);
        self.ml.resize(n, 0);
        self.extra.resize(n, [0; 3]);
        let slots = self.ll.iter_mut().zip(&mut self.of).zip(&mut self.ml);
        for (((ll, of), ml), (extra, s)) in slots.zip(self.extra.iter_mut().zip(sequences)) {
            let ll_value = ll_code(s.literal_length);
            let ml_value = ml_code(s.match_length);
            let value = offset_value(s, repeats);
            let code = value.ilog2();
            *ll = ll_value;
            *ml = ml_value;
            *of = code as u8;
            *extra = [
                s.literal_length - LL_BASE[usize::from(ll_value) & 63],
                s.match_length - ML_BASE[usize::from(ml_value) & 63],
                value - (1 << code),
            ];
        }
    }
}

/// The coded value for a sequence's offset, updating `repeats`. With no
/// literals the match cannot be at the first repeat offset, or the matcher
/// would have extended the one before, so values 1 to 3 shift up one place and
/// 3 means the first repeat offset less one.
#[inline]
fn offset_value(s: &Sequence, repeats: &mut [u32; 3]) -> u32 {
    let [first, second, third] = *repeats;
    let shift = u32::from(s.literal_length == 0);
    let index = if s.offset == first {
        0
    } else if s.offset == second {
        1
    } else if s.offset == third {
        2
    } else if shift == 1 && s.offset + 1 == first {
        3
    } else {
        *repeats = [s.offset, first, second];
        return s.offset + 3;
    };
    if shift == 1 && index == 0 {
        // Rare: a repeat at the first offset after no literals. Spell it out.
        *repeats = [s.offset, first, second];
        return s.offset + 3;
    }
    match index {
        1 => *repeats = [second, first, third],
        2 | 3 => *repeats = [s.offset, first, second],
        _ => {}
    }
    index + 1 - shift
}

fn histogram(codes: &[u8]) -> ([u32; MAX_SYMBOLS], usize) {
    let mut counts = [0u32; MAX_SYMBOLS];
    for &c in codes {
        counts[usize::from(c) & 63] += 1;
    }
    let used = counts.iter().rposition(|&c| c != 0).map_or(0, |p| p + 1);
    (counts, used)
}

/// Bits to code `counts` with a table, or `None` if a symbol has no slot.
fn table_cost(counts: &[u32; MAX_SYMBOLS], normalized: &[i16], log: u8) -> Option<f64> {
    let mut bits = 0.0;
    for (i, &c) in counts.iter().enumerate() {
        if c != 0 {
            let n = normalized.get(i).copied().unwrap_or(0);
            if n == 0 {
                return None;
            }
            let slots = f64::from(n.max(1));
            bits += f64::from(c) * (f64::from(log) - slots.log2());
        }
    }
    Some(bits)
}

/// The largest table worth its description for `count` symbols.
fn table_log_for(count: usize, used: usize, max_log: u8) -> u8 {
    let wanted = count.ilog2().saturating_sub(2).min(u32::from(max_log)) as u8;
    let floor = (used.ilog2() as u8 + 2).max(MIN_LOG);
    wanted.max(floor).min(max_log)
}

/// One field's encoder tables.
#[derive(Debug)]
struct FieldEncoder {
    /// In use, and the source of repeat mode.
    current: EncodeTable,
    current_counts: [i16; MAX_SYMBOLS],
    current_log: u8,
    /// False until an explicit table is set.
    repeatable: bool,
    /// What this segment will use, until committed.
    next: EncodeTable,
    next_counts: [i16; MAX_SYMBOLS],
    next_log: u8,
    predefined: EncodeTable,
    default_counts: &'static [i16],
    default_log: u8,
    max_log: u8,
    mode: u8,
}

impl FieldEncoder {
    fn new(default_counts: &'static [i16], default_log: u8, max_log: u8) -> Result<Self> {
        Ok(Self {
            current: EncodeTable::empty(),
            current_counts: [0; MAX_SYMBOLS],
            current_log: 0,
            repeatable: false,
            next: EncodeTable::empty(),
            next_counts: [0; MAX_SYMBOLS],
            next_log: 0,
            predefined: EncodeTable::new(default_counts, default_log, default_log)?,
            default_counts,
            default_log,
            max_log,
            mode: MODE_PREDEFINED,
        })
    }

    fn reset(&mut self) {
        self.repeatable = false;
    }

    /// Choose a mode for `codes` and append any table it needs.
    fn prepare(&mut self, codes: &[u8], out: &mut Vec<u8>) -> Result<()> {
        let (counts, used) = histogram(codes);
        if counts.iter().filter(|&&c| c != 0).count() == 1 {
            let symbol = codes.first().copied().unwrap_or(0);
            self.next.set_rle(symbol);
            self.mode = MODE_RLE;
            out.push(symbol);
            return Ok(());
        }

        let log = table_log_for(codes.len(), used, self.max_log);
        let normalized = normalize_counts(&counts[..used], log)?;
        let start = out.len();
        tans::write_counts(&normalized, log, out)?;
        let header_bits = ((out.len() - start) * 8) as f64;
        let explicit = table_cost(&counts, &normalized, log).map(|c| c + header_bits);
        let defaults = table_cost(&counts, self.default_counts, self.default_log);
        let repeat = if self.repeatable {
            table_cost(&counts, &self.current_counts, self.current_log)
        } else {
            None
        };

        let mut best = explicit.unwrap_or(f64::MAX);
        self.mode = MODE_EXPLICIT;
        if defaults.is_some_and(|c| c <= best) {
            best = defaults.unwrap_or(best);
            self.mode = MODE_PREDEFINED;
        }
        if repeat.is_some_and(|c| c <= best) {
            self.mode = MODE_REPEAT;
        }
        if self.mode == MODE_EXPLICIT {
            self.next.build(&normalized, log, self.max_log)?;
            self.next_counts = [0; MAX_SYMBOLS];
            self.next_counts[..used].copy_from_slice(&normalized);
            self.next_log = log;
        } else {
            out.truncate(start);
        }
        Ok(())
    }

    fn table(&self) -> &EncodeTable {
        match self.mode {
            MODE_PREDEFINED => &self.predefined,
            MODE_REPEAT => &self.current,
            _ => &self.next,
        }
    }

    /// The segment stays compressed: its table is now the current one.
    fn commit(&mut self) {
        if self.mode == MODE_EXPLICIT {
            std::mem::swap(&mut self.current, &mut self.next);
            self.current_counts = self.next_counts;
            self.current_log = self.next_log;
            self.repeatable = true;
        } else if self.mode != MODE_REPEAT {
            // The decoder's current table moved to one we do not reuse.
            self.repeatable = false;
        }
    }
}

/// Encoder for the sequences sections of one worker.
#[derive(Debug)]
pub struct SequenceEncoder {
    ll: FieldEncoder,
    of: FieldEncoder,
    ml: FieldEncoder,
    codes: Codes,
    repeats: [u32; 3],
    next_repeats: [u32; 3],
}

impl SequenceEncoder {
    /// A new encoder.
    ///
    /// # Errors
    ///
    /// Only if a predefined table is invalid, which a test rules out.
    pub fn new() -> Result<Self> {
        Ok(Self {
            ll: FieldEncoder::new(&LL_DEFAULT, LL_DEFAULT_LOG, MAX_LOG_LITERAL_LENGTH)?,
            of: FieldEncoder::new(&OF_DEFAULT, OF_DEFAULT_LOG, MAX_LOG_OFFSET)?,
            ml: FieldEncoder::new(&ML_DEFAULT, ML_DEFAULT_LOG, MAX_LOG_MATCH_LENGTH)?,
            codes: Codes::default(),
            repeats: INITIAL_REPEATS,
            next_repeats: INITIAL_REPEATS,
        })
    }

    /// Forget all state, for the start of a block.
    pub fn reset(&mut self) {
        self.ll.reset();
        self.of.reset();
        self.ml.reset();
        self.repeats = INITIAL_REPEATS;
        self.next_repeats = INITIAL_REPEATS;
    }

    /// Repeat offsets the decoder holds after the last committed segment.
    #[must_use]
    pub fn repeats(&self) -> [u32; 3] {
        self.repeats
    }

    /// Append a sequences section. Takes effect on [`Self::commit`].
    ///
    /// # Errors
    ///
    /// Only on an internal table failure.
    pub fn encode(&mut self, sequences: &[Sequence], out: &mut Vec<u8>) -> Result<()> {
        write_varint(sequences.len(), out);
        self.next_repeats = self.repeats;
        if sequences.is_empty() {
            return Ok(());
        }
        self.codes.fill(sequences, &mut self.next_repeats);
        let modes_at = out.len();
        out.push(0);
        self.ll.prepare(&self.codes.ll, out)?;
        self.of.prepare(&self.codes.of, out)?;
        self.ml.prepare(&self.codes.ml, out)?;
        out[modes_at] = self.ll.mode | (self.of.mode << 2) | (self.ml.mode << 4);
        self.write_stream(out);
        Ok(())
    }

    /// The segment stays compressed.
    pub fn commit(&mut self, had_sequences: bool) {
        self.repeats = self.next_repeats;
        if had_sequences {
            self.ll.commit();
            self.of.commit();
            self.ml.commit();
        }
    }

    fn write_stream(&self, out: &mut Vec<u8>) {
        let c = &self.codes;
        let (ll, of, ml) = (self.ll.table(), self.of.table(), self.ml.table());
        let n = c.ll.len();
        let mut writer = BitWriter::with_capacity(n * 10 + 16);
        let last = n - 1;
        let mut ll_state = ll.init(c.ll[last]);
        let mut of_state = of.init(c.of[last]);
        let mut ml_state = ml.init(c.ml[last]);
        write_extras(c, last, &mut writer);
        let states: (&mut EncodeState, &mut EncodeState, &mut EncodeState) =
            (&mut ll_state, &mut of_state, &mut ml_state);
        for i in (0..last).rev() {
            of.step(states.1, c.of[i], &mut writer);
            ml.step(states.2, c.ml[i], &mut writer);
            ll.step(states.0, c.ll[i], &mut writer);
            writer.flush();
            write_extras(c, i, &mut writer);
        }
        ml.finish(ml_state, &mut writer);
        of.finish(of_state, &mut writer);
        ll.finish(ll_state, &mut writer);
        out.extend_from_slice(&writer.finish());
    }
}

#[inline(always)]
fn write_extras(c: &Codes, i: usize, writer: &mut BitWriter) {
    let [ll, ml, of] = c.extra[i];
    writer.add(u64::from(ll), u32::from(LL_BITS[usize::from(c.ll[i]) & 63]));
    writer.add(u64::from(ml), u32::from(ML_BITS[usize::from(c.ml[i]) & 63]));
    // At most 16 + 16 + 22 bits, on top of the 7 a flush can leave.
    writer.add(u64::from(of), u32::from(c.of[i]));
    writer.flush();
}

/// A tANS entry with its field's code already resolved, so a sequence needs
/// no second table lookup.
#[derive(Debug, Clone, Copy, Default)]
struct SequenceEntry {
    /// The field's value before extra bits.
    value: u32,
    /// Added to the state bits read to give the next state.
    next: u16,
    /// State bits to read.
    bits: u8,
    /// Extra bits to read.
    extra: u8,
}

const TABLE_SIZE: usize = 512;
const TABLE_MASK: usize = TABLE_SIZE - 1;

type SequenceTable = Box<[SequenceEntry; TABLE_SIZE]>;

fn empty_table() -> SequenceTable {
    Box::new([SequenceEntry::default(); TABLE_SIZE])
}

/// Copy `entries` from `source`, resolving codes for `field`.
fn resolve(source: &DecodeTable, field: usize, table: &mut SequenceTable, log: u32) {
    for (state, slot) in table.iter_mut().enumerate().take(1 << log) {
        let entry = source.entry(state);
        let code = usize::from(entry.symbol) & 63;
        let (value, extra) = match field {
            0 => (LL_BASE[code], LL_BITS[code]),
            1 => (1 << code.min(31), code as u8),
            _ => (ML_BASE[code], ML_BITS[code]),
        };
        *slot = SequenceEntry {
            value,
            next: entry.base,
            bits: entry.bits,
            extra,
        };
    }
}

/// Take `n` bits, at most 63, off the top of `window`.
#[inline(always)]
fn pull(window: &mut u64, n: u8) -> u32 {
    let value = (*window >> 1) >> (63 - n);
    *window <<= n;
    value as u32
}

/// Decoder tables for one worker.
#[derive(Debug)]
pub struct SequenceDecoder {
    tables: [SequenceTable; 3],
    logs: [u32; 3],
    defaults: [SequenceTable; 3],
    default_logs: [u32; 3],
    scratch: DecodeTable,
    valid: [bool; 3],
    repeats: [u32; 3],
}

const FIELD_LOGS: [u8; 3] = [MAX_LOG_LITERAL_LENGTH, MAX_LOG_OFFSET, MAX_LOG_MATCH_LENGTH];
const FIELD_SYMBOLS: [usize; 3] = [LL_CODES, OF_CODES, ML_CODES];

impl SequenceDecoder {
    /// A new decoder.
    ///
    /// # Errors
    ///
    /// Only if a predefined table is invalid, which a test rules out.
    pub fn new() -> Result<Self> {
        let mut defaults = [empty_table(), empty_table(), empty_table()];
        let sources: [(&[i16], u8); 3] = [
            (&LL_DEFAULT, LL_DEFAULT_LOG),
            (&OF_DEFAULT, OF_DEFAULT_LOG),
            (&ML_DEFAULT, ML_DEFAULT_LOG),
        ];
        for (field, (counts, log)) in sources.into_iter().enumerate() {
            let table = DecodeTable::new(counts, log, log)?;
            resolve(&table, field, &mut defaults[field], u32::from(log));
        }
        Ok(Self {
            tables: [empty_table(), empty_table(), empty_table()],
            logs: [0; 3],
            defaults,
            default_logs: [
                u32::from(LL_DEFAULT_LOG),
                u32::from(OF_DEFAULT_LOG),
                u32::from(ML_DEFAULT_LOG),
            ],
            scratch: DecodeTable::empty(),
            valid: [false; 3],
            repeats: INITIAL_REPEATS,
        })
    }

    /// Forget all state, for the start of a block.
    pub fn reset(&mut self) {
        self.valid = [false; 3];
        self.repeats = INITIAL_REPEATS;
    }

    /// Set up one field's table. Returns the input after any description.
    fn read_table<'a>(&mut self, field: usize, mode: u8, input: &'a [u8]) -> Result<&'a [u8]> {
        match mode {
            MODE_PREDEFINED => {
                self.tables[field].clone_from(&self.defaults[field]);
                self.logs[field] = self.default_logs[field];
                self.valid[field] = true;
                Ok(input)
            }
            MODE_RLE => {
                let (&symbol, rest) = input
                    .split_first()
                    .ok_or_else(|| corrupt("truncated RLE table"))?;
                if usize::from(symbol) >= FIELD_SYMBOLS[field] {
                    return Err(corrupt("RLE code out of range"));
                }
                self.scratch.set_rle(symbol);
                resolve(&self.scratch, field, &mut self.tables[field], 0);
                self.logs[field] = 0;
                self.valid[field] = true;
                Ok(rest)
            }
            MODE_EXPLICIT => {
                let mut counts = [0i16; MAX_SYMBOLS];
                let (log, used, rest) =
                    tans::read_counts(input, FIELD_SYMBOLS[field], &mut counts)?;
                self.scratch
                    .build(&counts[..used], log, FIELD_LOGS[field])?;
                resolve(
                    &self.scratch,
                    field,
                    &mut self.tables[field],
                    u32::from(log),
                );
                self.logs[field] = u32::from(log);
                self.valid[field] = true;
                Ok(rest)
            }
            _ => {
                if self.valid[field] {
                    Ok(input)
                } else {
                    Err(corrupt("repeat mode with no earlier table"))
                }
            }
        }
    }

    /// Decode a sequences section, which fills `output[start..end]` from
    /// `literals`. `output[..start]` is what the block has produced so far.
    ///
    /// # Errors
    ///
    /// `CorruptedData` for any rule in the format's validation list.
    pub fn execute(
        &mut self,
        section: &[u8],
        literals: &[u8],
        output: &mut [u8],
        start: usize,
        end: usize,
    ) -> Result<()> {
        let (count, rest) =
            read_varint(section).ok_or_else(|| corrupt("truncated sequence count"))?;
        if count == 0 {
            if !rest.is_empty() || literals.len() != end - start {
                return Err(corrupt("literals do not fill the segment"));
            }
            output[start..end].copy_from_slice(literals);
            return Ok(());
        }
        // Each sequence makes at least a 3-byte match.
        if count > (end - start) / 3 {
            return Err(corrupt("too many sequences"));
        }
        let (&modes, rest) = rest
            .split_first()
            .ok_or_else(|| corrupt("truncated sequence modes"))?;
        if modes >> 6 != 0 {
            return Err(corrupt("reserved mode bits"));
        }
        let rest = self.read_table(0, modes & 3, rest)?;
        let rest = self.read_table(1, (modes >> 2) & 3, rest)?;
        let stream = self.read_table(2, (modes >> 4) & 3, rest)?;
        self.run(stream, count, literals, output, start, end)
    }

    fn run(
        &mut self,
        stream: &[u8],
        count: usize,
        literals: &[u8],
        output: &mut [u8],
        start: usize,
        end: usize,
    ) -> Result<()> {
        let [ll, of, ml] = &self.tables;
        let [ll_log, of_log, ml_log] = self.logs;
        let mut repeats = self.repeats;
        let mut reader = BitReader::new(stream)?;
        reader.refill();
        let mut ll_state = reader.take(ll_log) as usize;
        let mut of_state = reader.take(of_log) as usize;
        let mut ml_state = reader.take(ml_log) as usize;
        let (mut op, mut lp) = (start, 0usize);

        for i in 0..count {
            let ll_entry = ll[ll_state & TABLE_MASK];
            let of_entry = of[of_state & TABLE_MASK];
            let ml_entry = ml[ml_state & TABLE_MASK];

            reader.refill();
            let mut window = reader.window();
            let value = of_entry.value + pull(&mut window, of_entry.extra);
            let match_length = (ml_entry.value + pull(&mut window, ml_entry.extra)) as usize;
            let literal_length = (ll_entry.value + pull(&mut window, ll_entry.extra)) as usize;
            reader.consume(
                u32::from(of_entry.extra) + u32::from(ml_entry.extra) + u32::from(ll_entry.extra),
            );

            // Without branches: which kind of offset comes next is hard to predict.
            // Codes 1 to 3 pick from `choices`, one place later with no literals.
            let kind = (value as usize - 1 + usize::from(literal_length == 0)).min(4);
            let kind = if value > 3 { 4 } else { kind };
            let [first, second, third] = repeats;
            let choices = [
                first,
                second,
                third,
                first.wrapping_sub(1),
                value.wrapping_sub(3),
            ];
            let offset = choices[kind];
            repeats = [
                offset,
                if kind == 0 { second } else { first },
                if kind >= 2 { second } else { third },
            ];
            let offset = offset as usize;

            let after_literals = op + literal_length;
            let after_match = after_literals + match_length;
            if lp + literal_length > literals.len()
                || after_match > end
                || offset == 0
                || offset > after_literals
            {
                return Err(corrupt("sequence outside its bounds"));
            }
            copy_literals(output, op, literals, lp, literal_length);
            copy_match(output, after_literals, offset, match_length);
            op = after_match;
            lp += literal_length;

            if i + 1 < count {
                reader.refill();
                let mut window = reader.window();
                ll_state = usize::from(ll_entry.next) + pull(&mut window, ll_entry.bits) as usize;
                ml_state = usize::from(ml_entry.next) + pull(&mut window, ml_entry.bits) as usize;
                of_state = usize::from(of_entry.next) + pull(&mut window, of_entry.bits) as usize;
                reader.consume(
                    u32::from(ll_entry.bits) + u32::from(ml_entry.bits) + u32::from(of_entry.bits),
                );
            }
        }

        if !reader.is_exhausted() {
            return Err(corrupt("sequence stream did not end at its start"));
        }
        let tail = literals.get(lp..).unwrap_or(&[]);
        if op + tail.len() != end {
            return Err(corrupt("sequences do not fill the segment"));
        }
        output[op..end].copy_from_slice(tail);
        self.repeats = repeats;
        Ok(())
    }
}

/// Copy literals, bounds already checked. Short runs use one fixed-size copy,
/// which may write past the run into space the block will overwrite.
#[inline(always)]
fn copy_literals(output: &mut [u8], op: usize, literals: &[u8], lp: usize, length: usize) {
    if length <= 16 && lp + 16 <= literals.len() && op + 16 <= output.len() {
        output[op..op + 16].copy_from_slice(&literals[lp..lp + 16]);
    } else {
        output[op..op + length].copy_from_slice(&literals[lp..lp + length]);
    }
}

/// Copy a match, bounds already checked. Source and destination may overlap,
/// in which case the copied bytes repeat. The fixed-size paths may write past
/// the match into space the block will overwrite.
#[inline(always)]
fn copy_match(output: &mut [u8], op: usize, offset: usize, length: usize) {
    let mut source = op - offset;
    let end = op + length;
    if offset >= 16 && end + 16 <= output.len() {
        // Each chunk's source ends at or before its destination, so a split
        // gives both as plain slices.
        let mut to = op;
        while to < end {
            let (done, rest) = output.split_at_mut(to);
            rest[..16].copy_from_slice(&done[source..source + 16]);
            source += 16;
            to += 16;
        }
    } else if offset >= 8 && end + 8 <= output.len() {
        let mut to = op;
        while to < end {
            let (done, rest) = output.split_at_mut(to);
            rest[..8].copy_from_slice(&done[source..source + 8]);
            source += 8;
            to += 8;
        }
    } else if offset == 1 {
        let byte = output[source];
        output[op..end].fill(byte);
    } else if end + 8 <= output.len() {
        // A short period: lay it out in one word, then advance by whole periods.
        let mut word = [0u8; 8];
        let period = &output[source..op];
        for (i, slot) in word.iter_mut().enumerate() {
            *slot = period[i % offset];
        }
        let step = 8 / offset * offset;
        let mut to = op;
        while to < end {
            output[to..to + 8].copy_from_slice(&word);
            to += step;
        }
    } else {
        for to in op..end {
            output[to] = output[source];
            source += 1;
        }
    }
}

/// Bits per symbol the literals would need, for tests and estimates.
#[must_use]
pub fn order_0_bits(histogram: &[u32]) -> f64 {
    entropy_bits(histogram)
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn predefined_tables_are_valid() {
        SequenceEncoder::new().expect("encoder");
        SequenceDecoder::new().expect("decoder");
    }

    #[test]
    fn length_codes_invert() {
        for length in (0..4096).chain([65_535, 65_536, 131_069]) {
            let code = usize::from(ll_code(length));
            assert!(code < LL_CODES, "ll {length}");
            let extra = length - LL_BASE[code];
            assert!(extra < (1 << LL_BITS[code]), "ll {length} code {code}");
        }
        for length in (3..4096).chain([65_535, 65_539, 131_072]) {
            let code = usize::from(ml_code(length));
            assert!(code < ML_CODES, "ml {length}");
            let extra = length - ML_BASE[code];
            assert!(extra < (1 << ML_BITS[code]), "ml {length} code {code}");
        }
    }

    #[test]
    fn code_tables_match_rfc_8878() {
        assert_eq!(LL_BASE[16], 16);
        assert_eq!(LL_BASE[25], 64);
        assert_eq!(LL_BASE[35], 65_536);
        assert_eq!(ML_BASE[32], 35);
        assert_eq!(ML_BASE[42], 99);
        assert_eq!(ML_BASE[43], 131);
        assert_eq!(ML_BASE[52], 65_539);
    }

    #[test]
    fn table_description_round_trips() {
        let counts = normalize_counts(&[900, 300, 0, 20, 1, 1, 0, 7, 50], 9).expect("normalize");
        let mut out = Vec::new();
        tans::write_counts(&counts, 9, &mut out).expect("write");
        out.push(0xAB);
        let mut back = [0i16; MAX_SYMBOLS];
        let (log, used, rest) = tans::read_counts(&out, 36, &mut back).expect("read");
        assert_eq!((log, used), (9, counts.len()));
        assert_eq!(&back[..used], &counts[..]);
        assert_eq!(rest, [0xAB]);
    }
}
