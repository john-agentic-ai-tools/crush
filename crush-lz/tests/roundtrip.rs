//! Round trips through the public codec API (FR-002).
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::cast_possible_truncation
)]

use crush_lz::format::{FRAME_HEADER_SIZE, compress_bound};
use crush_lz::{FastLzConfig, compress, decompress};
use proptest::prelude::*;

fn config(log2: u8) -> FastLzConfig {
    FastLzConfig::builder()
        .block_size_log2(log2)
        .build()
        .expect("config")
}

fn roundtrip(data: &[u8], cfg: FastLzConfig) -> Vec<u8> {
    let packed = compress(data, &cfg).expect("compress");
    assert!(
        packed.len() <= compress_bound(data.len(), cfg.block_size()),
        "output {} exceeds bound for {} bytes",
        packed.len(),
        data.len()
    );
    let unpacked = decompress(&packed, data.len()).expect("decompress");
    assert_eq!(unpacked, data, "round trip mismatch ({} bytes)", data.len());
    packed
}

fn xorshift(len: usize, mut s: u64) -> Vec<u8> {
    (0..len)
        .map(|_| {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            s as u8
        })
        .collect()
}

/// Text-like data: words from a small vocabulary chosen pseudo-randomly.
fn wordy(len: usize) -> Vec<u8> {
    const WORDS: &[&[u8]] = &[
        b"fn ",
        b"let ",
        b"crush ",
        b"block ",
        b"match ",
        b"offset ",
        b"\n",
        b"0x1f ",
        b"error ",
        b"return ",
        b"literal ",
        b"{ ",
        b"} ",
    ];
    let picks = xorshift(len, 0x0BAD_5EED);
    let mut out = Vec::with_capacity(len + 8);
    let mut i = 0;
    while out.len() < len {
        out.extend_from_slice(WORDS[picks[i % picks.len()] as usize % WORDS.len()]);
        i += 1;
    }
    out.truncate(len);
    out
}

#[test]
fn empty_input_is_header_plus_end_marker() {
    let packed = roundtrip(&[], FastLzConfig::default());
    assert_eq!(packed.len(), FRAME_HEADER_SIZE + 4);
}

#[test]
fn inputs_shorter_than_a_match() {
    for len in 1..=3 {
        roundtrip(&vec![b'a'; len], FastLzConfig::default());
        roundtrip(&xorshift(len, 9), FastLzConfig::default());
    }
}

#[test]
fn block_boundaries() {
    let cfg = config(16);
    let bs = 1 << 16;
    for len in [bs - 1, bs, bs + 1, bs * 3 + bs / 2] {
        roundtrip(&wordy(len), cfg);
    }
}

#[test]
fn highly_repetitive_inputs() {
    let zeros = roundtrip(&vec![0u8; 5 << 20], FastLzConfig::default());
    assert!(
        zeros.len() < 64 * 1024,
        "zeros compressed to {}",
        zeros.len()
    );
    let phrase: Vec<u8> = b"abc".iter().copied().cycle().take(3 << 20).collect();
    roundtrip(&phrase, FastLzConfig::default());
}

#[test]
fn random_data_is_stored_with_bounded_overhead() {
    let data = xorshift(2 << 20, 0x9E37_79B9_7F4A_7C15);
    let packed = roundtrip(&data, FastLzConfig::default());
    // two 1 MiB blocks, each stored: frame + 2 block headers + end marker
    assert_eq!(packed.len(), data.len() + 8 + 2 * 8 + 4);
}

#[test]
fn text_compresses() {
    let data = wordy(4 << 20);
    let packed = roundtrip(&data, FastLzConfig::default());
    assert!(
        packed.len() * 2 < data.len(),
        "text ratio too low: {} -> {}",
        data.len(),
        packed.len()
    );
}

#[test]
fn long_matches_and_overlapping_copies() {
    // Short-period runs exercise overlapping matches (offset < length).
    let mut data = Vec::new();
    for period in [1usize, 2, 3, 7, 16, 255, 4000] {
        let pat = xorshift(period, period as u64 + 1);
        data.extend(pat.iter().copied().cycle().take(70_000));
        data.extend(xorshift(500, period as u64));
    }
    roundtrip(&data, config(16));
    roundtrip(&data, FastLzConfig::default());
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]
    #[test]
    fn arbitrary_inputs_round_trip(
        data in proptest::collection::vec(any::<u8>(), 0..300_000),
        big_blocks in any::<bool>(),
    ) {
        let cfg = config(if big_blocks { 18 } else { 16 });
        let packed = compress(&data, &cfg).expect("compress");
        prop_assert_eq!(decompress(&packed, data.len()).expect("decompress"), data);
    }

    #[test]
    fn mixed_random_and_repeats_round_trip(
        seed in any::<u64>(),
        repeats in proptest::collection::vec((0usize..200_000, 4usize..3_000, 1usize..70_000), 0..20),
    ) {
        let mut data = xorshift(200_000, seed | 1);
        for (at, len, back) in repeats {
            let end = (at + len).min(data.len());
            for i in at..end {
                if i >= back {
                    data[i] = data[i - back];
                }
            }
        }
        let cfg = config(16);
        let packed = compress(&data, &cfg).expect("compress");
        prop_assert_eq!(decompress(&packed, data.len()).expect("decompress"), data);
    }
}
