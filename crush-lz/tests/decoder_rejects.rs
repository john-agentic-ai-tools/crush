//! The decoder's obligations from contracts/fast-lz-format.md (FR-005):
//! malformed input is `CorruptedData`, never a panic, hang, or huge allocation.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::cast_possible_truncation
)]

use crush_core::error::{CrushError, ValidationError};
use crush_lz::decompress;
use crush_lz::format::{BlockHeader, END_MARKER};

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

#[test]
fn huge_declared_length_does_not_allocate_it() {
    // A tiny payload claiming a 1 TiB original must fail fast, not try to
    // reserve 1 TiB (which would abort the process).
    assert_corrupt(&one_block(&last(b"abcd"), 4), 1 << 40, "huge expected_len");
    let mut v = frame(20);
    v.extend_from_slice(&END_MARKER);
    assert_corrupt(&v, 1 << 40, "empty frame, huge expected_len");
}
