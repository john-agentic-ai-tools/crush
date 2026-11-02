//! Byte statistics for the literal stage.

/// Literals below this are not sampled: the full count is cheap.
const MIN_SAMPLED: usize = 2048;
/// One literal in this many goes into the sample.
const SAMPLE_STEP: usize = 8;
/// Above this many bits per byte, Huffman cannot pay for its table and time.
const FLAT_BITS: f64 = 7.9;

/// Count each byte value. Four tables keep repeated bytes from stalling on
/// one counter.
#[must_use]
pub fn byte_histogram(data: &[u8]) -> [u32; 256] {
    let mut counts = [[0u32; 256]; 4];
    let (groups, tail) = data.as_chunks::<4>();
    for &[a, b, c, d] in groups {
        counts[0][usize::from(a)] += 1;
        counts[1][usize::from(b)] += 1;
        counts[2][usize::from(c)] += 1;
        counts[3][usize::from(d)] += 1;
    }
    for &byte in tail {
        counts[0][usize::from(byte)] += 1;
    }
    let mut out = [0u32; 256];
    for (i, slot) in out.iter_mut().enumerate() {
        *slot = counts[0][i] + counts[1][i] + counts[2][i] + counts[3][i];
    }
    out
}

/// Order-0 entropy in bits per symbol.
#[allow(clippy::cast_precision_loss)]
#[must_use]
pub fn entropy_bits(histogram: &[u32]) -> f64 {
    let total: u64 = histogram.iter().map(|&c| u64::from(c)).sum();
    if total == 0 {
        return 0.0;
    }
    let total = total as f64;
    histogram
        .iter()
        .filter(|&&c| c != 0)
        .map(|&c| {
            let share = f64::from(c) / total;
            -share * share.log2()
        })
        .sum()
}

/// True if a sample of `literals` looks too flat for Huffman to help.
#[must_use]
pub fn looks_incompressible(literals: &[u8]) -> bool {
    if literals.len() < MIN_SAMPLED {
        return false;
    }
    let mut counts = [0u32; 256];
    for &byte in literals.iter().step_by(SAMPLE_STEP) {
        counts[usize::from(byte)] += 1;
    }
    entropy_bits(&counts) >= FLAT_BITS.min(f64::from((literals.len() / SAMPLE_STEP).ilog2()) - 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn histogram_counts_every_byte() {
        let data: Vec<u8> = (0..1003u32).map(|i| (i % 7) as u8).collect();
        let h = byte_histogram(&data);
        assert_eq!(h.iter().sum::<u32>(), 1003);
        assert_eq!(h[6], 143);
    }

    #[test]
    fn flat_data_is_flagged_and_skewed_data_is_not() {
        let mut x = 0x9E37_79B9_u32;
        let flat: Vec<u8> = (0..65_536)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                (x >> 8) as u8
            })
            .collect();
        assert!(looks_incompressible(&flat));
        let skewed: Vec<u8> = flat.iter().map(|b| b & 0x0F).collect();
        assert!(!looks_incompressible(&skewed));
        assert!(!looks_incompressible(&flat[..100]));
    }
}
