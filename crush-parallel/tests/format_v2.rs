//! Container version tests (R9).
#![allow(clippy::expect_used, clippy::missing_panics_doc)]

use crush_parallel::format::{FileFlags, FileHeader};

fn header(codec_id: u8) -> [u8; FileHeader::SIZE] {
    FileHeader::for_codec(65_536, 6, FileFlags::default(), 10, 1, codec_id).to_bytes()
}

#[test]
fn deflate_writes_version_1_with_a_zero_codec_byte() {
    let b = header(0);
    assert_eq!(b[4..8], 1u32.to_le_bytes());
    assert_eq!(b[22], 0);
}

#[test]
fn other_codecs_write_version_2_and_their_id() {
    let b = header(1);
    assert_eq!(b[4..8], 2u32.to_le_bytes());
    assert_eq!(b[22], 1);
    assert_eq!(FileHeader::from_bytes(&b).expect("parse").codec_id, 1);
}

#[test]
fn version_1_implies_deflate() {
    let mut b = header(0);
    assert_eq!(FileHeader::from_bytes(&b).expect("parse").codec_id, 0);
    b[22] = 1;
    assert!(FileHeader::from_bytes(&b).is_err());
}

#[test]
fn version_2_cannot_claim_deflate() {
    let mut b = header(1);
    b[22] = 0;
    assert!(FileHeader::from_bytes(&b).is_err());
}

#[test]
fn unknown_versions_are_rejected() {
    let mut b = header(1);
    b[4..8].copy_from_slice(&3u32.to_le_bytes());
    assert!(FileHeader::from_bytes(&b).is_err());
}
