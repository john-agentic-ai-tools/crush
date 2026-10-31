//! The decoder's obligations from contracts/fast-lz-format.md (FR-005):
//! malformed input is `CorruptedData`, never a panic, hang, or huge allocation.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::cast_possible_truncation
)]

use crush_core::error::{CrushError, ValidationError};
use crush_core::plugin::CompressionAlgorithm;
use crush_lz::format::{BlockHeader, END_MARKER};
use crush_lz::{FastLzPlugin, decompress};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

mod common;

use common::{frame, last, one_block, seq};

fn assert_corrupt(payload: &[u8], expected_len: usize, what: &str) {
    match decompress(payload, expected_len) {
        Err(CrushError::Validation(ValidationError::CorruptedData(_))) => {}
        other => panic!("{what}: expected CorruptedData, got {other:?}"),
    }
}

#[test]
fn valid_hand_built_block_decodes() {
    // sanity check of the helpers: "abcd" + match(offset 4, len 8) + "xy" = 14 bytes
    let mut p = seq(b"abcd", 4, 8);
    p.extend(last(b"xy"));
    let out = decompress(&one_block(&p, 14), 14).expect("valid block");
    assert_eq!(out, b"abcdabcdabcdxy");
}

#[test]
fn frame_header_errors() {
    assert_corrupt(&[], 0, "empty payload");
    assert_corrupt(b"LZ\x01", 0, "truncated frame header");
    let mut bad = one_block(&last(b"hi"), 2);
    bad[0] = b'X';
    assert_corrupt(&bad, 2, "bad magic");
    let mut bad = one_block(&last(b"hi"), 2);
    bad[2] = 9;
    assert_corrupt(&bad, 2, "bad version");
    let mut bad = one_block(&last(b"hi"), 2);
    bad[4] = 30;
    assert_corrupt(&bad, 2, "bad block size");
}

#[test]
fn block_header_errors() {
    let mut v = frame(16);
    v.extend_from_slice(&[5, 0, 0, 0]);
    assert_corrupt(&v, 5, "truncated block header");
    let mut v = frame(16);
    v.extend_from_slice(
        &BlockHeader {
            compressed_len: 3,
            stored: false,
            uncompressed_len: 70_000,
        }
        .to_bytes(),
    );
    assert_corrupt(&v, 70_000, "uncompressed_len > block size");
    let mut v = frame(16);
    v.extend_from_slice(
        &BlockHeader {
            compressed_len: 2,
            stored: true,
            uncompressed_len: 3,
        }
        .to_bytes(),
    );
    v.extend_from_slice(b"ab");
    v.extend_from_slice(&END_MARKER);
    assert_corrupt(&v, 3, "stored length mismatch");
}

#[test]
fn sequence_errors() {
    // literal length extension runs past the payload
    assert_corrupt(
        &one_block(&[0xF0, 255, 255], 600),
        600,
        "lit ext past payload",
    );
    // literals run past the payload
    assert_corrupt(
        &one_block(&[0x50, b'a', b'b'], 5),
        5,
        "literals past payload",
    );
    // offset 0
    let mut p = seq(b"abcd", 0, 4);
    p.extend(last(b""));
    assert_corrupt(&one_block(&p, 8), 8, "offset 0");
    // offset beyond produced bytes
    let mut p = seq(b"abcd", 5, 4);
    p.extend(last(b""));
    assert_corrupt(&one_block(&p, 8), 8, "offset > produced");
    // match runs past the declared block length
    let mut p = seq(b"abcd", 4, 20);
    p.extend(last(b""));
    assert_corrupt(&one_block(&p, 10), 10, "match > uncompressed_len");
    // offset truncated
    assert_corrupt(
        &one_block(&[0x40, b'a', b'b', b'c', b'd', 4], 8),
        8,
        "truncated offset",
    );
    // match length extension past the payload
    assert_corrupt(
        &one_block(&[0x4F, b'a', b'b', b'c', b'd', 4, 0, 255], 300),
        300,
        "match ext past payload",
    );
}

#[test]
fn block_length_and_total_errors() {
    // block decodes to fewer bytes than declared
    assert_corrupt(&one_block(&last(b"abc"), 4), 4, "short block");
    // more blocks than the expected total
    assert_corrupt(
        &one_block(&last(b"abcd"), 4),
        3,
        "total exceeds expected_len",
    );
    // fewer bytes than expected in total
    assert_corrupt(&one_block(&last(b"abcd"), 4), 5, "total below expected_len");
    // missing end marker
    let mut v = one_block(&last(b"abcd"), 4);
    v.truncate(v.len() - 4);
    assert_corrupt(&v, 4, "missing end marker");
    // trailing bytes after the end marker
    let mut v = one_block(&last(b"abcd"), 4);
    v.push(0);
    assert_corrupt(&v, 4, "trailing bytes");
}

fn push_block(v: &mut Vec<u8>, payload: &[u8], stored: bool, u: u32) {
    v.extend_from_slice(
        &BlockHeader {
            compressed_len: payload.len() as u32,
            stored,
            uncompressed_len: u,
        }
        .to_bytes(),
    );
    v.extend_from_slice(payload);
}

/// A block that declares 100 bytes but encodes far more, in the sequence
/// shape the decoder's fast loop accepts.
fn overrunning_block() -> Vec<u8> {
    let mut p = seq(&[b'a'; 20], 20, 4);
    for _ in 0..20 {
        p.extend(seq(b"bcde", 16, 18));
    }
    p.extend(last(b""));
    p
}

#[test]
fn block_overrun_is_rejected_with_room_after_it() {
    // The output buffer has space after this block for the next one, so the
    // decoder must bound the block by its own declared size.
    let mut v = frame(16);
    push_block(&mut v, &overrunning_block(), false, 100);
    push_block(&mut v, &[7; 4096], true, 4096);
    v.extend_from_slice(&END_MARKER);
    assert_corrupt(&v, 100 + 4096, "block overruns its declared size");
}

#[test]
fn forged_block_len_grows_one_block_only() {
    // A few payload bytes declare a 1 MiB block and a 1 TiB total.
    let mut v = frame(20);
    push_block(&mut v, &last(b"ab"), false, 1 << 20);
    v.extend_from_slice(&END_MARKER);
    assert_corrupt(&v, 1 << 40, "forged block length and total");
}

fn stream(payload: &[u8]) -> (crush_core::error::Result<u64>, Vec<u8>) {
    let mut sink = Vec::new();
    let cancel = Arc::new(AtomicBool::new(false));
    let result = FastLzPlugin.decompress_to(payload, cancel, &mut sink);
    (result, sink)
}

/// A 65,536-byte run of a 16-byte pattern, so the streaming decoder's buffer is full of
/// these bytes when the next block arrives.
fn marker_block() -> (Vec<u8>, Vec<u8>) {
    let pattern = *b"MARKERmarkerMARK";
    let mut p = seq(&pattern, 16, 65_536 - 16);
    p.extend(last(b""));
    (p, pattern.repeat(4096))
}

#[test]
fn corrupt_block_after_valid_block_surfaces_no_stale_bytes() {
    let (valid, expected) = marker_block();
    let mut beyond = seq(b"abcd", 5, 4);
    beyond.extend(last(b""));
    let mut past = seq(b"abcd", 4, 200);
    past.extend(last(b""));
    for (what, block, ulen) in [
        ("decodes short", last(b"abc"), 1000),
        ("offset beyond the block", beyond, 8),
        ("match past the block", past, 10),
    ] {
        let mut v = frame(16);
        push_block(&mut v, &valid, false, 65_536);
        push_block(&mut v, &block, false, ulen);
        v.extend_from_slice(&END_MARKER);
        let (result, sink) = stream(&v);
        assert!(result.is_err(), "{what}: expected an error");
        assert_eq!(sink, expected, "{what}: wrote more than the valid block");
    }
}

#[test]
fn smaller_block_after_larger_writes_exact_length() {
    let (valid, mut expected) = marker_block();
    let mut v = frame(16);
    push_block(&mut v, &valid, false, 65_536);
    push_block(&mut v, &last(b"hello"), false, 5);
    push_block(&mut v, &last(&[b'z'; 40]), false, 40);
    v.extend_from_slice(&END_MARKER);
    expected.extend_from_slice(b"hello");
    expected.extend_from_slice(&[b'z'; 40]);
    let (result, sink) = stream(&v);
    assert_eq!(result.expect("valid frame"), expected.len() as u64);
    assert_eq!(sink, expected);
}

#[test]
fn huge_declared_length_does_not_allocate_it() {
    // A tiny payload claiming a 1 TiB original must fail fast, not try to
    // reserve 1 TiB (which would abort the process).
    assert_corrupt(&one_block(&last(b"abcd"), 4), 1 << 40, "huge expected_len");
    let mut v = frame(20);
    v.extend_from_slice(&END_MARKER);
    assert_corrupt(&v, 1 << 40, "empty frame, huge expected_len");
}
