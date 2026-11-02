//! T030: the bit stream and Huffman coding.
#![allow(clippy::expect_used)]

use crush_sprint::bits::{BitReader, BitWriter};
use crush_sprint::huffman::{
    HuffmanDecoder, HuffmanEncoder, MAX_CODE_LENGTH, build_code_lengths, validate_code_lengths,
};
use proptest::prelude::*;

fn histogram(data: &[u8]) -> [u32; 256] {
    let mut h = [0u32; 256];
    for &b in data {
        h[usize::from(b)] += 1;
    }
    h
}

/// Kraft sum scaled by `2^MAX_CODE_LENGTH`. A complete code sums to exactly that.
fn kraft(lengths: &[u8]) -> u32 {
    lengths
        .iter()
        .filter(|&&l| l != 0)
        .map(|&l| 1u32 << (MAX_CODE_LENGTH - l))
        .sum()
}

#[test]
fn bits_come_back_last_written_first() {
    let mut w = BitWriter::new();
    w.write(0b101, 3);
    w.write(0xABCD, 16);
    w.write(1, 1);
    let bytes = w.finish();
    let mut r = BitReader::new(&bytes).expect("reader");
    assert_eq!(r.read(1), 1);
    assert_eq!(r.read(16), 0xABCD);
    assert_eq!(r.read(3), 0b101);
    assert!(r.is_exhausted());
}

#[test]
fn zero_bit_reads_consume_nothing() {
    let mut w = BitWriter::new();
    w.write(0b11, 2);
    let bytes = w.finish();
    let mut r = BitReader::new(&bytes).expect("reader");
    assert_eq!(r.read(0), 0);
    assert!(!r.is_exhausted());
}

/// The final byte holds the sentinel: a stream without one has no start.
#[test]
fn stream_needs_a_sentinel() {
    assert!(BitReader::new(&[]).is_err());
    assert!(BitReader::new(&[0x12, 0x00]).is_err());
}

#[test]
fn reading_past_the_start_is_reported_and_does_not_panic() {
    let mut w = BitWriter::new();
    w.write(1, 1);
    let bytes = w.finish();
    let mut r = BitReader::new(&bytes).expect("reader");
    let _ = r.read(32);
    assert!(r.overflowed());
    assert!(!r.is_exhausted());
}

#[test]
fn code_lengths_form_a_complete_code_with_a_maximum_of_11_bits() {
    // Fibonacci counts force the deepest tree an unlimited builder could make.
    let mut h = [0u32; 256];
    let (mut a, mut b) = (1u32, 1u32);
    for slot in h.iter_mut().take(40) {
        *slot = a;
        (a, b) = (b, a.saturating_add(b));
    }
    let lengths = build_code_lengths(&h).expect("lengths");
    assert!(lengths.iter().all(|&l| l <= MAX_CODE_LENGTH));
    assert_eq!(kraft(&lengths), 1 << MAX_CODE_LENGTH);
    validate_code_lengths(&lengths).expect("complete");
}

#[test]
fn rejects_code_lengths_that_are_too_long() {
    let mut lengths = [0u8; 256];
    lengths[0] = 1;
    lengths[1] = MAX_CODE_LENGTH + 1;
    lengths[2] = MAX_CODE_LENGTH + 1;
    assert!(validate_code_lengths(&lengths).is_err());
    assert!(HuffmanDecoder::from_lengths(&lengths).is_err());
}

#[test]
fn rejects_incomplete_and_oversubscribed_codes() {
    let mut incomplete = [0u8; 256];
    incomplete[0] = 2;
    incomplete[1] = 2;
    assert!(validate_code_lengths(&incomplete).is_err());

    let mut oversubscribed = [0u8; 256];
    oversubscribed[..3].fill(1);
    assert!(validate_code_lengths(&oversubscribed).is_err());
}

#[test]
fn rejects_a_table_with_no_symbols() {
    assert!(validate_code_lengths(&[0u8; 256]).is_err());
}

/// One symbol is the RLE mode's job, not Huffman's.
#[test]
fn single_symbol_has_no_huffman_code() {
    let mut h = [0u32; 256];
    h[7] = 100;
    assert!(build_code_lengths(&h).is_err());
}

#[test]
fn four_streams_of_tiny_inputs_round_trip() {
    for len in 2..=16 {
        let data: Vec<u8> = (0..len).map(|i| u8::from(i % 3 == 0)).collect();
        let lengths = build_code_lengths(&histogram(&data)).expect("lengths");
        let mut out = Vec::new();
        HuffmanEncoder::from_lengths(&lengths)
            .expect("encoder")
            .encode_4_streams(&data, &mut out)
            .expect("encode");
        let mut back = vec![0u8; data.len()];
        HuffmanDecoder::from_lengths(&lengths)
            .expect("decoder")
            .decode_4_streams(&out, &mut back)
            .expect("decode");
        assert_eq!(back, data, "length {len}");
    }
}

#[test]
fn decoding_a_truncated_stream_is_an_error() {
    let data = b"abracadabra, abracadabra, abracadabra".repeat(20);
    let lengths = build_code_lengths(&histogram(&data)).expect("lengths");
    let mut out = Vec::new();
    HuffmanEncoder::from_lengths(&lengths)
        .expect("encoder")
        .encode_4_streams(&data, &mut out)
        .expect("encode");
    out.truncate(out.len() / 2);
    let mut back = vec![0u8; data.len()];
    assert!(
        HuffmanDecoder::from_lengths(&lengths)
            .expect("decoder")
            .decode_4_streams(&out, &mut back)
            .is_err()
    );
}

proptest! {
    #[test]
    fn bit_fields_round_trip(fields in prop::collection::vec((any::<u64>(), 0u32..=57), 0..200)) {
        let mut w = BitWriter::new();
        for &(v, n) in &fields {
            w.write(v & ((1u64 << n) - 1), n);
        }
        let bytes = w.finish();
        let mut r = BitReader::new(&bytes).expect("reader");
        for &(v, n) in fields.iter().rev() {
            prop_assert_eq!(r.read(n), v & ((1u64 << n) - 1));
        }
        prop_assert!(r.is_exhausted());
    }

    #[test]
    fn literals_round_trip(data in prop::collection::vec(any::<u8>(), 2..4096)) {
        let h = histogram(&data);
        prop_assume!(h.iter().filter(|&&c| c != 0).count() >= 2);
        let lengths = build_code_lengths(&h).expect("lengths");
        prop_assert!(lengths.iter().all(|&l| l <= MAX_CODE_LENGTH));
        let mut out = Vec::new();
        HuffmanEncoder::from_lengths(&lengths)
            .expect("encoder")
            .encode_4_streams(&data, &mut out)
            .expect("encode");
        let mut back = vec![0u8; data.len()];
        HuffmanDecoder::from_lengths(&lengths)
            .expect("decoder")
            .decode_4_streams(&out, &mut back)
            .expect("decode");
        prop_assert_eq!(back, data);
    }
}
