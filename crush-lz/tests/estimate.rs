//! The sampled ratio estimate behind content-aware selection (FR-007,
//! FR-008): close to the real ratio where the data compresses, below the
//! selection threshold where it doesn't, and taken from the whole input.
#![allow(clippy::expect_used, clippy::cast_precision_loss)]

use crush_core::MIN_ESTIMATED_RATIO;
use crush_core::plugin::CompressionAlgorithm;
use crush_lz::{FastLzConfig, FastLzPlugin, compress};

#[path = "../../crush-parallel/benches/common/variants.rs"]
mod variants;

mod common;

use common::{base64, xorshift};

fn estimate(data: &[u8]) -> f64 {
    FastLzPlugin.estimate_ratio(data).expect("estimate")
}

fn real_ratio(data: &[u8]) -> f64 {
    let packed = compress(data, &FastLzConfig::default()).expect("compress");
    data.len() as f64 / packed.len() as f64
}

/// Samples are compressed without the history a whole block has, so the
/// estimate runs low (about 20% with 4-kilobyte samples at 1 MB, under 10%
/// with 16-kilobyte samples). That is the safe side for selection: it may decline a
/// borderline input, never pick one that would not compress. It must never
/// be noticeably above the real ratio, and not far below it.
#[test]
fn text_estimate_is_conservative_and_close() {
    for size in [1 << 20, 2 << 20, 4 << 20, 16 << 20] {
        let data = variants::generate_corpus(size, 0xDEAD_BEEF_CAFE_1234);
        let (est, real) = (estimate(&data), real_ratio(&data));
        assert!(
            est <= 1.05 * real && est >= 0.75 * real,
            "{size} bytes: estimate {est:.3}, real {real:.3}"
        );
        assert!(est >= MIN_ESTIMATED_RATIO);
    }
}

#[test]
fn base64_and_random_are_below_the_threshold() {
    let random = xorshift(4 << 20, 0x9E37_79B9_7F4A_7C15);
    let encoded = base64(&xorshift(3 << 20, 0x1234_5678_9ABC_DEF1));
    for (name, data) in [("random", &random), ("base64", &encoded)] {
        let est = estimate(data);
        assert!(
            est < MIN_ESTIMATED_RATIO,
            "{name}: estimate {est:.3} would select fast-lz"
        );
    }
}

#[test]
fn samples_cover_the_whole_input() {
    // Incompressible first half, text second half: compressible overall.
    let mut data = xorshift(8 << 20, 77);
    data.extend(variants::generate_corpus(8 << 20, 0xDEAD_BEEF_CAFE_1234));
    let (est, real) = (estimate(&data), real_ratio(&data));
    assert!(real >= MIN_ESTIMATED_RATIO, "fixture: real ratio {real:.3}");
    assert!(
        est >= MIN_ESTIMATED_RATIO,
        "estimate {est:.3}, real {real:.3}"
    );
}

#[test]
fn small_and_empty_inputs() {
    assert!(FastLzPlugin.estimate_ratio(&[]).is_none());
    // Below 16 kilobytes the whole input is compressed.
    let small = variants::generate_corpus(10_000, 1);
    let est = estimate(&small);
    assert!(est > 1.0, "{est}");
    assert!(estimate(&xorshift(10_000, 3)) < MIN_ESTIMATED_RATIO);
}

#[test]
fn estimate_does_not_disturb_compression() {
    // The estimate reuses a per-thread compressor; output must not depend on it.
    let data = variants::generate_corpus(2 << 20, 5);
    let before = compress(&data, &FastLzConfig::default()).expect("compress");
    let _ = estimate(&xorshift(1 << 20, 9));
    let _ = estimate(&data);
    let after = compress(&data, &FastLzConfig::default()).expect("compress");
    assert_eq!(before, after);
}
