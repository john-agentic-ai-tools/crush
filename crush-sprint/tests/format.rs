//! T034: the byte layouts in `specs/026-hybrid-speed-codec/contracts/sprint-format.md`.
#![allow(clippy::expect_used, clippy::cast_possible_truncation)]

use crush_core::CrushError;
use crush_parallel::EngineConfiguration;
use crush_parallel::codec::BlockCodec;
use crush_parallel::engine::compress_with;
use crush_parallel::format::{FORMAT_VERSION_CODEC, FileHeader};
use crush_sprint::format::{
    BLOCK_SIZE, MAX_SEGMENT_SIZE, SPRINT_VERSION, SegmentHeader, SegmentKind,
};
use crush_sprint::{CODEC_ID, SprintCodec};

fn payload(data: &[u8]) -> Vec<u8> {
    let codec = SprintCodec;
    let mut encoder = codec.encoder().expect("encoder");
    let mut out = Vec::new();
    assert!(
        codec
            .compress_block(&mut encoder, data, &mut out)
            .expect("compress"),
        "expected a compressed block"
    );
    out
}

#[test]
fn constants_match_the_contract() {
    assert_eq!(CODEC_ID, 1);
    assert_eq!(<SprintCodec as BlockCodec>::CODEC_ID, 1);
    assert_eq!(SPRINT_VERSION, 1);
    assert_eq!(MAX_SEGMENT_SIZE, 131_072);
    assert_eq!(BLOCK_SIZE, 4 << 20);
}

#[test]
fn segment_header_is_type_then_size_in_24_bits() {
    let h = SegmentHeader {
        kind: SegmentKind::Raw,
        size: 5,
    };
    assert_eq!(h.to_bytes(), [0b0001_0100, 0, 0]);
    let h = SegmentHeader {
        kind: SegmentKind::Rle,
        size: 131_072,
    };
    assert_eq!(h.to_bytes(), [0x01, 0x00, 0x08]);
    let h = SegmentHeader {
        kind: SegmentKind::Compressed,
        size: 0x3F_FFFF,
    };
    assert_eq!(h.to_bytes(), [0xFE, 0xFF, 0xFF]);
}

#[test]
fn segment_headers_parse_back() {
    for kind in [SegmentKind::Raw, SegmentKind::Rle, SegmentKind::Compressed] {
        for size in [0, 1, 131_072, 0x3F_FFFF] {
            let h = SegmentHeader { kind, size };
            assert_eq!(SegmentHeader::parse(&h.to_bytes()).expect("parse"), h);
        }
    }
}

#[test]
fn reserved_segment_type_3_is_an_error() {
    assert!(SegmentHeader::parse(&[0b0000_0111, 0, 0]).is_err());

    let codec = SprintCodec;
    let mut decoder = codec.decoder().expect("decoder");
    let mut output = [0u8; 1];
    let payload = [SPRINT_VERSION, 0b0000_0111, 0, 0, 0];
    assert!(
        codec
            .decompress_block(&mut decoder, &payload, &mut output)
            .is_err()
    );
}

#[test]
fn a_short_segment_header_is_an_error() {
    assert!(SegmentHeader::parse(&[0, 0]).is_err());
}

#[test]
fn a_block_starts_with_the_sprint_version() {
    assert_eq!(payload(&b"abcabcabc".repeat(1000))[0], SPRINT_VERSION);
}

#[test]
fn a_block_of_one_byte_value_uses_rle_segments() {
    let data = vec![9u8; 131_072 * 2];
    let p = payload(&data);
    assert_eq!(p.len(), 1 + 2 * (3 + 1));
    let h = SegmentHeader::parse(&p[1..4]).expect("parse");
    assert_eq!(
        h,
        SegmentHeader {
            kind: SegmentKind::Rle,
            size: 131_072
        }
    );
    assert_eq!(p[4], 9);
}

#[test]
fn no_segment_regenerates_more_than_the_limit() {
    let data = vec![1u8; 3 * 131_072 + 5];
    let p = payload(&data);
    let mut at = 1;
    let mut total = 0;
    while at < p.len() {
        let h = SegmentHeader::parse(&p[at..]).expect("parse");
        assert!(h.size as usize <= MAX_SEGMENT_SIZE);
        total += h.size as usize;
        at += 3 + 1;
    }
    assert_eq!(total, data.len());
}

/// Unknown versions name the problem: `contracts/cli.md` wants an unsupported
/// version error, not a generic corruption report.
#[test]
fn a_newer_sprint_version_is_an_unsupported_version_error() {
    let codec = SprintCodec;
    let mut decoder = codec.decoder().expect("decoder");
    let mut output = [0u8; 1];
    let err = codec
        .decompress_block(&mut decoder, &[SPRINT_VERSION + 1, 0, 0, 0], &mut output)
        .expect_err("newer version");
    assert!(matches!(err, CrushError::VersionMismatch { .. }), "{err:?}");
}

#[test]
fn the_container_is_crsh_v2_with_codec_id_1_at_byte_22() {
    let data = b"container layout check ".repeat(4000);
    let config = EngineConfiguration::builder()
        .block_size(BLOCK_SIZE as u32)
        .build()
        .expect("config");
    let file = compress_with(&SprintCodec, &data, &config).expect("compress");
    assert_eq!(file[22], 1);
    let header =
        FileHeader::from_bytes(file[..FileHeader::SIZE].try_into().expect("size")).expect("header");
    assert_eq!(header.format_version, FORMAT_VERSION_CODEC);
    assert_eq!(header.codec_id, 1);
}

#[test]
fn table_literals_and_sequence_tables_change_nothing_between_runs() {
    let data = b"the same input gives the same bytes ".repeat(2000);
    assert_eq!(payload(&data), payload(&data));
}
