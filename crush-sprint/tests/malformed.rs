//! T033: every validation rule in `specs/026-hybrid-speed-codec/data-model.md`.
//!
//! Payloads are built by hand from `contracts/sprint-format.md`. `RLE` modes
//! for all three sequence fields need no tANS table, so a sequence can be
//! written with a code byte per field. Field codes follow RFC 8878: literal
//! length codes up to 15 and match length codes up to 31 carry no extra bits,
//! and match length is the code plus 3. Offset code N reads N extra bits and
//! gives value `2^N + extra`.
//!
//! Rules covered elsewhere: code lengths in `huffman.rs`; table logs and
//! normalized counts in `tans.rs`; container and stored-block rules in
//! `crush-parallel`. A zero offset cannot be written, because the smallest
//! value selects a repeat offset and those start at 1, 4 and 8.
#![allow(clippy::expect_used, clippy::cast_possible_truncation)]

use crush_parallel::codec::BlockCodec;
use crush_sprint::SprintCodec;
use crush_sprint::bits::BitWriter;
use crush_sprint::format::SPRINT_VERSION;
use proptest::prelude::*;

const RAW: u32 = 0;
const RLE: u32 = 1;
const COMPRESSED: u32 = 2;
const MODE_RLE: u8 = 1;
const MODE_REPEAT: u8 = 3;

fn u24(v: u32) -> [u8; 3] {
    [v as u8, (v >> 8) as u8, (v >> 16) as u8]
}

fn segment(kind: u32, size: u32) -> [u8; 3] {
    u24(kind | (size << 2))
}

fn decode(payload: &[u8], len: usize) -> Result<Vec<u8>, crush_core::CrushError> {
    let codec = SprintCodec;
    let mut decoder = codec.decoder().expect("decoder");
    let mut output = vec![0u8; len];
    codec.decompress_block(&mut decoder, payload, &mut output)?;
    Ok(output)
}

fn raw_block(chunks: &[&[u8]]) -> Vec<u8> {
    let mut p = vec![SPRINT_VERSION];
    for c in chunks {
        p.extend_from_slice(&segment(RAW, c.len() as u32));
        p.extend_from_slice(c);
    }
    p
}

fn raw_literals(bytes: &[u8]) -> Vec<u8> {
    let mut s = vec![0u8];
    s.extend_from_slice(&u24(bytes.len() as u32));
    s.extend_from_slice(bytes);
    s
}

/// One compressed segment with raw literals and `count` sequences of a single
/// literal length, offset and match length code.
struct Crafted<'a> {
    regenerated: u32,
    literals: &'a [u8],
    ll: u8,
    of: u8,
    ml: u8,
    count: u8,
    modes: u8,
    /// Written after the sentinel-only stream a valid sequence needs.
    stream: Vec<u8>,
}

impl Default for Crafted<'_> {
    /// "a" then 4 copies of it: the valid control.
    fn default() -> Self {
        Self {
            regenerated: 5,
            literals: b"a",
            ll: 1,
            of: 0,
            ml: 1,
            count: 1,
            modes: MODE_RLE | (MODE_RLE << 2) | (MODE_RLE << 4),
            stream: BitWriter::new().finish(),
        }
    }
}

impl Crafted<'_> {
    fn build(&self) -> Vec<u8> {
        let mut body = u24(self.regenerated).to_vec();
        body.extend_from_slice(&raw_literals(self.literals));
        body.push(self.count);
        body.push(self.modes);
        // Tables in contract order: literal length, offset, match length.
        for (shift, code) in [(0, self.ll), (2, self.of), (4, self.ml)] {
            if (self.modes >> shift) & 3 == MODE_RLE {
                body.push(code);
            }
        }
        body.extend_from_slice(&self.stream);
        let mut p = vec![SPRINT_VERSION];
        p.extend_from_slice(&segment(COMPRESSED, body.len() as u32));
        p.extend_from_slice(&body);
        p
    }
}

/// If this fails, the other crafted payloads are not testing their own rule.
#[test]
fn control_a_hand_built_sequence_decodes() {
    assert_eq!(
        decode(&Crafted::default().build(), 5).expect("valid"),
        b"aaaaa"
    );
}

#[test]
fn control_raw_and_rle_segments_decode() {
    assert_eq!(
        decode(&raw_block(&[b"abc", b"de"]), 5).expect("valid"),
        b"abcde"
    );
    let mut p = vec![SPRINT_VERSION];
    p.extend_from_slice(&segment(RLE, 4));
    p.push(b'z');
    assert_eq!(decode(&p, 4).expect("valid"), b"zzzz");
}

#[test]
fn empty_payload_has_no_version() {
    assert!(decode(&[], 0).is_err());
}

#[test]
fn segment_sizes_must_sum_exactly_to_the_block_size() {
    assert!(decode(&raw_block(&[b"abc"]), 4).is_err(), "short");
    assert!(decode(&raw_block(&[b"abc", b"de"]), 4).is_err(), "long");
    assert!(decode(&[SPRINT_VERSION], 1).is_err(), "no segments");
}

#[test]
fn regenerated_size_must_be_at_most_128_kib() {
    let big = vec![0u8; 131_073];
    assert!(decode(&raw_block(&[&big]), big.len()).is_err(), "raw");

    let mut p = vec![SPRINT_VERSION];
    p.extend_from_slice(&segment(RLE, 131_073));
    p.push(0);
    assert!(decode(&p, 131_073).is_err(), "RLE");

    let c = Crafted {
        regenerated: 131_073,
        ..Crafted::default()
    };
    assert!(decode(&c.build(), 131_073).is_err(), "compressed");

    let ok = vec![0u8; 131_072];
    assert!(
        decode(&raw_block(&[&ok]), ok.len()).is_ok(),
        "the limit itself"
    );
}

#[test]
fn section_lengths_must_fit_the_remaining_payload() {
    let mut p = vec![SPRINT_VERSION];
    p.extend_from_slice(&segment(RAW, 10));
    p.extend_from_slice(b"abc");
    assert!(decode(&p, 10).is_err(), "raw body");

    let mut p = Crafted::default().build();
    p.truncate(p.len() - 2);
    assert!(decode(&p, 5).is_err(), "compressed body");

    let mut p = vec![SPRINT_VERSION];
    p.extend_from_slice(&segment(RLE, 4));
    assert!(decode(&p, 4).is_err(), "RLE byte");

    assert!(decode(&[SPRINT_VERSION, 0x00], 1).is_err(), "header");
}

#[test]
fn trailing_bytes_after_the_last_segment_are_an_error() {
    let mut p = raw_block(&[b"abc"]);
    p.push(0);
    assert!(decode(&p, 3).is_err());
}

#[test]
fn literals_section_must_fit_its_segment() {
    let mut body = u24(3).to_vec();
    let mut lits = raw_literals(b"abc");
    lits.truncate(lits.len() - 1);
    body.extend_from_slice(&lits);
    let mut p = vec![SPRINT_VERSION];
    p.extend_from_slice(&segment(COMPRESSED, body.len() as u32));
    p.extend_from_slice(&body);
    assert!(decode(&p, 3).is_err());
}

/// Regenerated literals above the limit are rejected before any allocation.
#[test]
fn literals_above_the_limit_are_rejected() {
    let mut body = u24(16).to_vec();
    body.push(1);
    body.extend_from_slice(&u24(131_073));
    body.push(b'a');
    let mut p = vec![SPRINT_VERSION];
    p.extend_from_slice(&segment(COMPRESSED, body.len() as u32));
    p.extend_from_slice(&body);
    assert!(decode(&p, 16).is_err());
}

/// Literals with no sequences must fill the segment on their own.
#[test]
fn literals_only_segment_must_match_its_size() {
    let c = Crafted {
        regenerated: 3,
        literals: b"ab",
        count: 0,
        ..Crafted::default()
    };
    let mut body = u24(c.regenerated).to_vec();
    body.extend_from_slice(&raw_literals(c.literals));
    body.push(0);
    let mut p = vec![SPRINT_VERSION];
    p.extend_from_slice(&segment(COMPRESSED, body.len() as u32));
    p.extend_from_slice(&body);
    assert!(decode(&p, 3).is_err());
}

fn huffman_header(mode: u8, regenerated: u32, total: u32) -> Vec<u8> {
    let mut s = vec![mode];
    s.extend_from_slice(&u24(regenerated));
    s.extend_from_slice(&u24(total));
    s
}

fn segment_of(regenerated: u32, tail: &[u8]) -> Vec<u8> {
    let mut body = u24(regenerated).to_vec();
    body.extend_from_slice(tail);
    let mut p = vec![SPRINT_VERSION];
    p.extend_from_slice(&segment(COMPRESSED, body.len() as u32));
    p.extend_from_slice(&body);
    p
}

#[test]
fn huffman_repeat_mode_with_no_earlier_table_is_an_error() {
    let mut section = huffman_header(3, 8, 10);
    section.extend_from_slice(&[0x02, 0, 0x02, 0, 0x02, 0]);
    section.extend_from_slice(&[0xFF; 8]);
    assert!(decode(&segment_of(8, &section), 8).is_err());
}

/// A real payload with Huffman literals, its total stream size forged.
#[test]
fn stream_sizes_must_fit_the_section() {
    let mut x = 0x2545_F491_4F6C_DD1D_u64;
    let data: Vec<u8> = (0..65_536)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x & 15) as u8
        })
        .collect();
    let codec = SprintCodec;
    let mut encoder = codec.encoder().expect("encoder");
    let mut payload = Vec::new();
    assert!(
        codec
            .compress_block(&mut encoder, &data, &mut payload)
            .expect("compress")
    );
    // version, segment header, regenerated size, then the literals section
    let section = 1 + 3 + 3;
    assert_eq!(payload[section] & 3, 2, "expected Huffman literals");
    let total = usize::from(payload[section + 4])
        | usize::from(payload[section + 5]) << 8
        | usize::from(payload[section + 6]) << 16;
    assert!(section + 7 + total <= payload.len());

    let mut mutated = payload.clone();
    mutated[section + 4] = 0xFF;
    mutated[section + 5] = 0xFF;
    mutated[section + 6] = 0xFF;
    assert!(
        decode(&mutated, data.len()).is_err(),
        "total past the payload"
    );

    let mut mutated = payload;
    mutated[section + 4] = 0;
    mutated[section + 5] = 0;
    mutated[section + 6] = 0;
    assert!(decode(&mutated, data.len()).is_err(), "empty streams");
}

#[test]
fn sequence_repeat_mode_with_no_earlier_table_is_an_error() {
    for shift in [0, 2, 4] {
        let modes =
            (MODE_RLE | (MODE_RLE << 2) | (MODE_RLE << 4)) & !(3 << shift) | (MODE_REPEAT << shift);
        let c = Crafted {
            modes,
            ..Crafted::default()
        };
        assert!(decode(&c.build(), 5).is_err(), "field at bit {shift}");
    }
}

#[test]
fn bitstreams_must_end_exactly_at_their_initial_position() {
    let mut w = BitWriter::new();
    w.write(1, 1);
    let c = Crafted {
        stream: w.finish(),
        ..Crafted::default()
    };
    assert!(decode(&c.build(), 5).is_err(), "bits left over");

    // Offset code 3 wants 3 extra bits, and the stream has none.
    let c = Crafted {
        of: 3,
        ..Crafted::default()
    };
    assert!(decode(&c.build(), 5).is_err(), "bits run out");
}

#[test]
fn literal_lengths_must_not_exceed_the_remaining_literals() {
    let c = Crafted {
        ll: 2,
        regenerated: 6,
        ..Crafted::default()
    };
    assert!(decode(&c.build(), 6).is_err());
}

#[test]
fn match_offsets_must_not_exceed_bytes_already_produced() {
    let c = Crafted {
        ll: 0,
        literals: b"",
        regenerated: 4,
        ..Crafted::default()
    };
    assert!(decode(&c.build(), 4).is_err(), "first byte of the block");

    // Offset code 3, extra 7: value 15, offset 12, with one byte produced.
    let mut w = BitWriter::new();
    w.write(7, 3);
    let c = Crafted {
        of: 3,
        stream: w.finish(),
        ..Crafted::default()
    };
    assert!(decode(&c.build(), 5).is_err(), "past the block start");
}

/// Offsets may reach into earlier segments of the block, and no further.
#[test]
fn a_match_may_reach_an_earlier_segment() {
    // With no literals, offset code 0 is the second repeat offset, 4.
    let mut p = raw_block(&[b"wxyz"]);
    let c = Crafted {
        ll: 0,
        literals: b"",
        regenerated: 4,
        ..Crafted::default()
    };
    p.extend_from_slice(&c.build()[1..]);
    assert_eq!(decode(&p, 8).expect("valid"), b"wxyzwxyz");
}

/// With no literals, value 3 is the first repeat offset less one: zero here.
#[test]
fn a_zero_offset_is_an_error() {
    let mut w = BitWriter::new();
    w.write(1, 1);
    let c = Crafted {
        ll: 0,
        of: 1,
        literals: b"",
        regenerated: 4,
        stream: w.finish(),
        ..Crafted::default()
    };
    let mut p = raw_block(&[b"wxyz"]);
    p.extend_from_slice(&c.build()[1..]);
    assert!(decode(&p, 8).is_err());
}

#[test]
fn output_must_never_exceed_the_regenerated_size() {
    let c = Crafted {
        regenerated: 3,
        ..Crafted::default()
    };
    assert!(decode(&c.build(), 3).is_err(), "match past the segment");

    let c = Crafted {
        regenerated: 6,
        ..Crafted::default()
    };
    assert!(decode(&c.build(), 6).is_err(), "segment left short");

    let c = Crafted {
        count: 2,
        ..Crafted::default()
    };
    assert!(
        decode(&c.build(), 5).is_err(),
        "second sequence past the end"
    );
}

/// The forged count must fail on the bitstream, not on a huge allocation.
#[test]
fn a_forged_sequence_count_is_rejected_cheaply() {
    let mut p = Crafted::default().build();
    let at = p.len() - 6;
    assert_eq!(p[at], 1, "sequence count byte");
    p[at] = 0x7F;
    assert!(decode(&p, 5).is_err());
}

fn valid_payload() -> (Vec<u8>, Vec<u8>) {
    let mut data = b"sprint decodes every byte it is handed, safely. ".repeat(600);
    data.extend((0..4096u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 24) as u8));
    let codec = SprintCodec;
    let mut encoder = codec.encoder().expect("encoder");
    let mut payload = Vec::new();
    assert!(
        codec
            .compress_block(&mut encoder, &data, &mut payload)
            .expect("compress")
    );
    (data, payload)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn any_bytes_never_panic(mut payload in prop::collection::vec(any::<u8>(), 0..2048), len in 0usize..4096) {
        if let Some(first) = payload.first_mut() {
            *first = SPRINT_VERSION;
        }
        let _ = decode(&payload, len);
    }

    #[test]
    fn flipped_bytes_never_panic(flips in prop::collection::vec((any::<prop::sample::Index>(), any::<u8>()), 1..8)) {
        let (data, mut payload) = valid_payload();
        for (at, xor) in flips {
            let i = at.index(payload.len());
            payload[i] ^= xor;
        }
        let _ = decode(&payload, data.len());
    }

    #[test]
    fn truncated_payloads_are_errors(cut in any::<prop::sample::Index>()) {
        let (data, payload) = valid_payload();
        let keep = cut.index(payload.len());
        prop_assert!(decode(&payload[..keep], data.len()).is_err());
    }
}
