//! Two-table match finder, in the manner of zstd's level 3.
//!
//! One table hashes 8 bytes and one hashes 5. Each position probes both, and
//! the repeat offset one byte ahead, and the step grows while nothing matches.
//! The tables are boxed arrays, and a constant mask sits on every hash, so
//! their lookups need no bounds check.

use crush_core::error::{CrushError, Result};

/// The largest 8-byte table.
pub const MAX_LONG_LOG: u32 = 18;
/// The largest 5-byte table.
pub const MAX_SHORT_LOG: u32 = 17;

const LONG_ENTRIES: usize = 1 << MAX_LONG_LOG;
const SHORT_ENTRIES: usize = 1 << MAX_SHORT_LOG;
const PRIME_8: u64 = 0xCF1B_BCDC_B7A5_6463;
const PRIME_5: u64 = 0x00CF_1BBC_DCBB;

/// One match and the literals before it. `offset` is a plain distance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sequence {
    /// Literals before the match.
    pub literal_length: u32,
    /// Distance back to the match.
    pub offset: u32,
    /// Bytes matched.
    pub match_length: u32,
}

#[inline(always)]
fn read_64(data: &[u8], at: usize) -> u64 {
    data.get(at..at + 8)
        .and_then(|w| <[u8; 8]>::try_from(w).ok())
        .map_or(0, u64::from_le_bytes)
}

#[inline(always)]
fn read_32(data: &[u8], at: usize) -> u32 {
    data.get(at..at + 4)
        .and_then(|w| <[u8; 4]>::try_from(w).ok())
        .map_or(0, u32::from_le_bytes)
}

/// Bytes equal between `data[a..]` and `data[b..end]`, with `a < b`.
#[inline]
fn common_length(data: &[u8], a: usize, b: usize, end: usize) -> usize {
    let (Some(left), Some(right)) = (data.get(a..end), data.get(b..end)) else {
        return 0;
    };
    let words = left.as_chunks::<8>().0.iter().zip(right.as_chunks::<8>().0);
    let mut matched = 0;
    for (w, v) in words {
        let (w, v) = (u64::from_le_bytes(*w), u64::from_le_bytes(*v));
        if w != v {
            return matched + ((w ^ v).trailing_zeros() / 8) as usize;
        }
        matched += 8;
    }
    let tail = left.iter().skip(matched).zip(right.iter().skip(matched));
    matched + tail.take_while(|(x, y)| x == y).count()
}

/// Bytes equal going back from `at` and `from`, at most `max`.
#[inline]
fn common_length_back(data: &[u8], at: usize, from: usize, max: usize) -> usize {
    let (Some(left), Some(right)) = (data.get(..at), data.get(..from)) else {
        return 0;
    };
    let pairs = left.iter().rev().zip(right.iter().rev());
    pairs.take(max).take_while(|(x, y)| x == y).count()
}

/// Update repeat offsets for a match at `offset`.
#[inline]
fn update_repeats(repeats: &mut [u32; 3], offset: u32) {
    if offset == repeats[1] {
        repeats.swap(0, 1);
    } else if offset != repeats[0] {
        *repeats = [offset, repeats[0], repeats[1]];
    }
}

/// Finds matches. Holds about 1.5 MB of tables, reused across blocks.
#[derive(Debug)]
pub struct Matcher {
    long: Box<[u32; LONG_ENTRIES]>,
    short: Box<[u32; SHORT_ENTRIES]>,
    long_shift: u32,
    short_shift: u32,
}

impl Matcher {
    /// A matcher with `2^long_log` 8-byte and `2^short_log` 5-byte entries.
    ///
    /// # Errors
    ///
    /// `InvalidConfig` if either log is zero or above its maximum.
    pub fn new(long_log: u32, short_log: u32) -> Result<Self> {
        if !(1..=MAX_LONG_LOG).contains(&long_log) || !(1..=MAX_SHORT_LOG).contains(&short_log) {
            return Err(CrushError::InvalidConfig(format!(
                "match table logs {long_log} and {short_log} out of range"
            )));
        }
        let long = vec![0u32; LONG_ENTRIES].into_boxed_slice().try_into();
        let short = vec![0u32; SHORT_ENTRIES].into_boxed_slice().try_into();
        match (long, short) {
            (Ok(long), Ok(short)) => Ok(Self {
                long,
                short,
                long_shift: 64 - long_log,
                short_shift: 64 - short_log,
            }),
            _ => Err(CrushError::InvalidConfig("match tables".into())),
        }
    }

    /// Forget all positions, for the start of a block.
    pub fn reset(&mut self) {
        self.long.fill(0);
        self.short.fill(0);
    }

    #[inline(always)]
    fn long_slot(&self, word: u64) -> usize {
        (word.wrapping_mul(PRIME_8) >> self.long_shift) as usize & (LONG_ENTRIES - 1)
    }

    #[inline(always)]
    fn short_slot(&self, word: u64) -> usize {
        ((word << 24).wrapping_mul(PRIME_5) >> self.short_shift) as usize & (SHORT_ENTRIES - 1)
    }

    /// Find matches in `block[start..end]`, which may reach back to the start
    /// of `block`, and append them to `sequences`. Returns where the trailing
    /// literals start. Position 0 is never a match source.
    #[allow(clippy::cast_possible_truncation)] // block positions fit in u32
    #[allow(clippy::too_many_lines)] // one hot loop
    pub fn find(
        &mut self,
        block: &[u8],
        start: usize,
        end: usize,
        repeats: &mut [u32; 3],
        sequences: &mut Vec<Sequence>,
    ) -> usize {
        let limit = end.min(block.len()).saturating_sub(8);
        let mut ip = start;
        let mut anchor = start;

        while ip < limit {
            let word = read_64(block, ip);
            let long_slot = self.long_slot(word);
            let short_slot = self.short_slot(word);
            let long_candidate = self.long[long_slot] as usize;
            let short_candidate = self.short[short_slot] as usize;
            self.long[long_slot] = ip as u32;
            self.short[short_slot] = ip as u32;

            let repeat = repeats[0] as usize;
            let (match_at, source, length) =
                if ip + 1 >= repeat && read_32(block, ip + 1 - repeat) == read_32(block, ip + 1) {
                    let at = ip + 1;
                    (at, at - repeat, common_length(block, at - repeat, at, end))
                } else if long_candidate > 0 && read_64(block, long_candidate) == word {
                    let n = 8 + common_length(block, long_candidate + 8, ip + 8, end);
                    (ip, long_candidate, n)
                } else if short_candidate > 0 && read_32(block, short_candidate) == word as u32 {
                    // A long match one byte on beats this short one.
                    let next = read_64(block, ip + 1);
                    let slot = self.long_slot(next);
                    let candidate = self.long[slot] as usize;
                    self.long[slot] = (ip + 1) as u32;
                    if candidate > 0 && read_64(block, candidate) == next {
                        let n = 8 + common_length(block, candidate + 8, ip + 9, end);
                        (ip + 1, candidate, n)
                    } else {
                        let n = 4 + common_length(block, short_candidate + 4, ip + 4, end);
                        (ip, short_candidate, n)
                    }
                } else {
                    ip += ((ip - anchor) >> 8) + 1;
                    continue;
                };

            // Extend backward over literals not yet emitted.
            let back = common_length_back(block, match_at, source, match_at - anchor);
            let (at, from, length) = (match_at - back, source - back, length + back);

            let offset = (at - from) as u32;
            update_repeats(repeats, offset);
            sequences.push(Sequence {
                literal_length: (at - anchor) as u32,
                offset,
                match_length: length as u32,
            });
            let match_start = at;
            ip = at + length;
            anchor = ip;

            if ip < limit {
                self.insert_after_match(block, match_start, ip);
                ip = self.repeat_run(block, ip, end, limit, repeats, sequences);
                anchor = ip;
            }
        }
        anchor
    }

    #[inline(always)]
    #[allow(clippy::cast_possible_truncation)]
    fn insert_after_match(&mut self, block: &[u8], match_start: usize, ip: usize) {
        let early = match_start + 2;
        let word = read_64(block, early);
        let slot = self.long_slot(word);
        self.long[slot] = early as u32;
        let slot = self.short_slot(word);
        self.short[slot] = early as u32;
        let late = ip - 2;
        let word = read_64(block, late);
        let slot = self.long_slot(word);
        self.long[slot] = late as u32;
        let slot = self.short_slot(read_64(block, ip - 1));
        self.short[slot] = (ip - 1) as u32;
    }

    /// Take matches at the second repeat offset, with no literals between.
    #[inline(always)]
    #[allow(clippy::cast_possible_truncation)]
    fn repeat_run(
        &mut self,
        block: &[u8],
        mut ip: usize,
        end: usize,
        limit: usize,
        repeats: &mut [u32; 3],
        sequences: &mut Vec<Sequence>,
    ) -> usize {
        while ip < limit {
            let second = repeats[1] as usize;
            if second > ip || read_32(block, ip - second) != read_32(block, ip) {
                break;
            }
            let length = 4 + common_length(block, ip - second + 4, ip + 4, end);
            repeats.swap(0, 1);
            sequences.push(Sequence {
                literal_length: 0,
                offset: second as u32,
                match_length: length as u32,
            });
            let word = read_64(block, ip);
            let slot = self.long_slot(word);
            self.long[slot] = ip as u32;
            let slot = self.short_slot(word);
            self.short[slot] = ip as u32;
            ip += length;
        }
        ip
    }
}

#[cfg(test)]
// Plain `assert!(a == b)`: `assert_eq!` would print megabytes on failure.
#[allow(clippy::expect_used, clippy::manual_assert_eq)]
mod tests {
    use super::*;

    const REPEATS: [u32; 3] = [1, 4, 8];

    fn rebuild(block: &[u8], sequences: &[Sequence], tail: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(block.len());
        let mut at = 0;
        for s in sequences {
            let lits = s.literal_length as usize;
            out.extend_from_slice(&block[at..at + lits]);
            at += lits;
            let from = out.len() - s.offset as usize;
            assert!(s.offset > 0, "zero offset");
            for i in 0..s.match_length as usize {
                out.push(out[from + i]);
            }
            at += s.match_length as usize;
        }
        assert_eq!(at, tail, "sequences must end where the tail starts");
        out.extend_from_slice(&block[tail..]);
        out
    }

    fn matcher() -> Matcher {
        Matcher::new(18, 16).expect("matcher")
    }

    fn check_segments(block: &[u8], segment: usize) -> usize {
        let mut m = matcher();
        let (mut repeats, mut all) = (REPEATS, Vec::new());
        let mut out = Vec::new();
        let mut start = 0;
        while start < block.len() {
            let end = (start + segment).min(block.len());
            all.clear();
            let tail = m.find(block, start, end, &mut repeats, &mut all);
            assert!(tail <= end);
            let mut at = start;
            for s in &all {
                out.extend_from_slice(&block[at..at + s.literal_length as usize]);
                at += (s.literal_length + s.match_length) as usize;
                let from = out.len() - s.offset as usize;
                for i in 0..s.match_length as usize {
                    out.push(out[from + i]);
                }
            }
            assert_eq!(at, tail);
            out.extend_from_slice(&block[tail..end]);
            start = end;
        }
        assert!(out == block);
        all.len()
    }

    fn pseudo_random(len: usize) -> Vec<u8> {
        let mut x = 0x9E37_79B9_7F4A_7C15_u64;
        (0..len)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                x as u8
            })
            .collect()
    }

    #[test]
    fn short_inputs_are_all_literals() {
        for len in 0..=8 {
            let block = vec![7u8; len];
            let mut sequences = Vec::new();
            let tail = matcher().find(&block, 0, len, &mut REPEATS.clone(), &mut sequences);
            assert_eq!(tail, 0);
            assert_eq!(sequences.len(), 0);
        }
    }

    /// Segments cap the step at about 512, so a far copy cannot be stepped over.
    #[test]
    fn a_far_copy_is_found_across_the_block() {
        let head = pseudo_random(4096);
        let mut block = head.clone();
        block.extend(pseudo_random(3 << 20).iter().map(|b| b ^ 0x5A));
        block.extend_from_slice(&head);
        let mut m = matcher();
        let mut repeats = REPEATS;
        let mut longest = 0;
        for start in (0..block.len()).step_by(131_072) {
            let end = (start + 131_072).min(block.len());
            let mut sequences = Vec::new();
            m.find(&block, start, end, &mut repeats, &mut sequences);
            let top = sequences.iter().map(|s| s.match_length).max();
            longest = longest.max(top.unwrap_or(0));
        }
        assert!(longest > 3000, "longest match {longest}");
        check_segments(&block, 131_072);
    }

    #[test]
    fn runs_use_the_repeat_offset() {
        let block = vec![3u8; 100_000];
        let mut sequences = Vec::new();
        let tail = matcher().find(&block, 0, block.len(), &mut REPEATS.clone(), &mut sequences);
        assert_eq!(sequences[0].offset, 1);
        assert!(sequences.len() < 3);
        assert!(rebuild(&block, &sequences, tail) == block);
    }

    #[test]
    fn matches_stop_at_the_segment_end_and_may_reach_earlier_segments() {
        let line: Vec<u8> = b"crush sprint segments end where they end\n".repeat(20_000);
        assert!(check_segments(&line, 131_072) < 16);
        let mut mixed = pseudo_random(300_000);
        mixed.extend_from_within(1000..200_000);
        mixed.extend(line.iter().take(200_000));
        check_segments(&mixed, 131_072);
    }

    /// Inputs that differ only in their fifth byte must not share a slot
    /// by construction, and bytes past the fifth must not matter.
    #[test]
    fn the_short_hash_covers_exactly_five_bytes() {
        let m = matcher();
        let base = 0x0807_0605_0403_0201_u64;
        assert_ne!(m.short_slot(base), m.short_slot(base ^ (0xFF << 32)));
        assert_ne!(m.short_slot(base), m.short_slot(base ^ 0xFF));
        assert_eq!(m.short_slot(base), m.short_slot(base ^ (0xFF << 40)));
    }

    #[test]
    fn repeat_offsets_rotate() {
        let mut r = [1, 4, 8];
        update_repeats(&mut r, 1);
        assert_eq!(r, [1, 4, 8]);
        update_repeats(&mut r, 4);
        assert_eq!(r, [4, 1, 8]);
        update_repeats(&mut r, 100);
        assert_eq!(r, [100, 4, 1]);
    }

    #[test]
    fn tables_reset_between_blocks() {
        let block = pseudo_random(1 << 16);
        let mut m = matcher();
        let (mut a, mut b) = (Vec::new(), Vec::new());
        m.find(&block, 0, block.len(), &mut REPEATS.clone(), &mut a);
        m.reset();
        m.find(&block, 0, block.len(), &mut REPEATS.clone(), &mut b);
        assert_eq!(a, b);
    }

    #[test]
    fn rejects_table_logs_out_of_range() {
        assert!(Matcher::new(19, 16).is_err());
        assert!(Matcher::new(18, 18).is_err());
        assert!(Matcher::new(0, 16).is_err());
    }
}
