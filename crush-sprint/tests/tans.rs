//! T031: tANS tables and coding.
#![allow(clippy::expect_used)]

use crush_sprint::bits::{BitReader, BitWriter};
use crush_sprint::tans::{
    DecodeTable, EncodeTable, MAX_LOG_LITERAL_LENGTH, MAX_LOG_MATCH_LENGTH, MAX_LOG_OFFSET,
    normalize_counts,
};
use proptest::prelude::*;

fn histogram(symbols: &[u8]) -> Vec<u32> {
    let mut h = vec![0u32; 64];
    for &s in symbols {
        h[usize::from(s)] += 1;
    }
    h
}

fn round_trip(symbols: &[u8], log: u8) -> Vec<u8> {
    let normalized = normalize_counts(&histogram(symbols), log).expect("normalize");
    let mut w = BitWriter::new();
    EncodeTable::new(&normalized, log, log)
        .expect("encode table")
        .encode(symbols, &mut w);
    let bytes = w.finish();
    let mut r = BitReader::new(&bytes).expect("reader");
    let decoded = DecodeTable::new(&normalized, log, log)
        .expect("decode table")
        .decode(&mut r, symbols.len())
        .expect("decode");
    assert!(
        r.is_exhausted(),
        "bitstream must end at its initial position"
    );
    decoded
}

#[test]
fn maximum_table_logs_are_9_8_and_9() {
    assert_eq!(MAX_LOG_LITERAL_LENGTH, 9);
    assert_eq!(MAX_LOG_OFFSET, 8);
    assert_eq!(MAX_LOG_MATCH_LENGTH, 9);
}

#[test]
fn normalized_counts_sum_to_the_table_size() {
    let normalized = normalize_counts(&[1000, 300, 20, 1, 1, 0, 7], 8).expect("normalize");
    let sum: i32 = normalized.iter().map(|&c| i32::from(c.max(-1).abs())).sum();
    assert_eq!(sum, 1 << 8);
}

#[test]
fn every_seen_symbol_keeps_a_probability() {
    let normalized = normalize_counts(&[1_000_000, 1, 1, 1], 6).expect("normalize");
    assert!(normalized.iter().all(|&c| c != 0));
}

#[test]
fn normalized_counts_that_do_not_sum_are_rejected() {
    assert!(DecodeTable::new(&[100, 100], 8, 8).is_err(), "short");
    assert!(DecodeTable::new(&[200, 100], 8, 8).is_err(), "long");
    assert!(EncodeTable::new(&[100, 100], 8, 8).is_err());
}

#[test]
fn a_table_log_above_its_maximum_is_rejected() {
    let counts = [512i16];
    assert!(DecodeTable::new(&counts, 9, MAX_LOG_LITERAL_LENGTH).is_ok());
    assert!(DecodeTable::new(&counts, 9, MAX_LOG_OFFSET).is_err());
    let counts = [1024i16];
    assert!(DecodeTable::new(&counts, 10, MAX_LOG_MATCH_LENGTH).is_err());
}

#[test]
fn a_table_log_of_zero_is_rejected() {
    assert!(DecodeTable::new(&[1], 0, MAX_LOG_OFFSET).is_err());
}

#[test]
fn a_count_below_minus_one_is_rejected() {
    assert!(DecodeTable::new(&[-2, 258], 8, 8).is_err());
}

#[test]
fn reading_past_the_stream_is_an_error() {
    let symbols = [0u8, 1, 2, 1, 0, 1, 1];
    let normalized = normalize_counts(&histogram(&symbols), 6).expect("normalize");
    let mut w = BitWriter::new();
    w.write(1, 1);
    let bytes = w.finish();
    let mut r = BitReader::new(&bytes).expect("reader");
    assert!(
        DecodeTable::new(&normalized, 6, 6)
            .expect("table")
            .decode(&mut r, 10_000)
            .is_err()
    );
}

#[test]
fn skewed_symbols_round_trip_small() {
    let mut symbols = vec![3u8; 5000];
    symbols.extend_from_slice(&[0, 1, 2, 4, 5, 6]);
    assert_eq!(round_trip(&symbols, 9), symbols);
}

proptest! {
    #[test]
    fn symbols_round_trip(
        symbols in prop::collection::vec(0u8..36, 2..2000),
        log in 6u8..=9,
    ) {
        prop_assume!(symbols.iter().any(|&s| s != symbols[0]));
        prop_assert_eq!(round_trip(&symbols, log), symbols);
    }
}
