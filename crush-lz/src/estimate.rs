//! Sampled ratio estimate for content-aware selection (research R6).
//!
//! Compresses evenly spaced samples totalling about 1/128 of the input
//! (between 4 samples of 4 kilobytes and 16 of 16 kilobytes) with the
//! block compressor, so the
//! cost stays near 1% of compressing the whole input (samples cost more per
//! byte than whole blocks; 1/64 measured 2.02% at 16 MB) and the samples see
//! all of it rather than its head. Inputs smaller than the minimum sample
//! total are compressed whole.

use crate::compress::{Compressor, WILD};
use std::cell::RefCell;
use std::ops::Range;

const MIN_SAMPLES: usize = 4;
const MAX_SAMPLES: usize = 16;
const MIN_SAMPLE: usize = 4 * 1024;
const MAX_SAMPLE: usize = 16 * 1024;
/// Fraction of the input sampled: `1 / SAMPLE_DIVISOR`.
const SAMPLE_DIVISOR: usize = 128;

/// Ranges of `input_len` bytes to compress for the estimate.
pub(crate) fn sample_ranges(input_len: usize) -> Vec<Range<usize>> {
    if input_len < MIN_SAMPLES * MIN_SAMPLE {
        return if input_len == 0 {
            Vec::new()
        } else {
            std::iter::once(0..input_len).collect()
        };
    }
    let budget = input_len / SAMPLE_DIVISOR;
    let count = (budget / MAX_SAMPLE).clamp(MIN_SAMPLES, MAX_SAMPLES);
    let size = (budget / count).clamp(MIN_SAMPLE, MAX_SAMPLE);
    // Evenly spaced: the first sample starts at 0, the last ends at input_len.
    let span = input_len - size;
    (0..count)
        .map(|k| {
            let start = span * k / (count - 1);
            start..start + size
        })
        .collect()
}

/// Reused per thread: the compressor's table and an output buffer.
struct State {
    compressor: Compressor,
    out: Vec<u8>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State {
        compressor: Compressor::new(),
        out: Vec::new(),
    });
}

/// Estimated `input.len() / compressed_len` for `input`; `None` when empty.
pub(crate) fn estimate_ratio(input: &[u8]) -> Option<f64> {
    let ranges = sample_ranges(input.len());
    if ranges.is_empty() {
        return None;
    }
    STATE.with(|state| {
        let mut state = state.try_borrow_mut().ok()?;
        let State { compressor, out } = &mut *state;
        let (mut raw, mut packed) = (0usize, 0usize);
        for r in ranges {
            let sample = input.get(r)?;
            if out.len() < sample.len() + WILD {
                out.resize(sample.len() + WILD, 0);
            }
            // A sample that would not shrink counts as stored.
            let n = compressor
                .compress_block(sample, out)
                .unwrap_or(sample.len());
            raw += sample.len();
            packed += n;
        }
        ratio(raw, packed)
    })
}

#[allow(clippy::cast_precision_loss)] // sample totals are far below 2^52
fn ratio(raw: usize, packed: usize) -> Option<f64> {
    (packed > 0).then(|| raw as f64 / packed as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn total(r: &[Range<usize>]) -> usize {
        r.iter().map(ExactSizeIterator::len).sum()
    }

    #[test]
    fn plan_sizes() {
        let cases: [(usize, usize, usize); 6] = [
            (16 * 1024, 4, 4 * 1024),
            (512 * 1024, 4, 4 * 1024),
            (1 << 20, 4, 4 * 1024),
            (16 << 20, 8, 16 * 1024),
            (32 << 20, 16, 16 * 1024),
            (128 << 20, 16, 16 * 1024),
        ];
        for (len, count, size) in cases {
            let r = sample_ranges(len);
            assert_eq!(r.len(), count, "{len} bytes");
            assert!(r.iter().all(|s| s.len() == size), "{len} bytes");
        }
        // 8 MB: budget 64 KiB -> 4 samples of 16 KiB
        let r = sample_ranges(8 << 20);
        assert_eq!((r.len(), r[0].len()), (4, 16 * 1024));
    }

    #[test]
    fn small_inputs_are_sampled_whole() {
        assert_eq!(sample_ranges(0), Vec::<Range<usize>>::new());
        assert_eq!(sample_ranges(1), vec![0..1]);
        assert_eq!(sample_ranges(16 * 1024 - 1), vec![0..16 * 1024 - 1]);
    }

    #[test]
    fn samples_span_the_input_without_overlap() {
        for len in [16 * 1024, 100_000, 1 << 20, 3_333_333, 16 << 20, 1 << 30] {
            let r = sample_ranges(len);
            assert_eq!(r[0].start, 0);
            assert_eq!(r[r.len() - 1].end, len);
            assert!(r.windows(2).all(|w| w[0].end <= w[1].start), "{len}");
        }
    }

    /// Cost bound (SC-005): from 1 megabyte up, at most 1/128 of the input
    /// or the 4 x 4-kilobyte floor (1/64 of a 1 MB input).
    #[test]
    fn sampled_bytes_stay_within_budget() {
        let floor = MIN_SAMPLES * MIN_SAMPLE;
        for len in [1 << 20, 1_500_000, 4 << 20, 16 << 20, 100 << 20, 1 << 30] {
            let budget = (len / SAMPLE_DIVISOR).max(floor);
            assert!(total(&sample_ranges(len)) <= budget, "{len}");
        }
    }
}
