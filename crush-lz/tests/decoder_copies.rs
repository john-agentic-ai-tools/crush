//! Valid hand-built blocks decode to exactly what a byte-by-byte reference
//! produces, for every short offset and match length the decoder's copy
//! fast paths distinguish, and for literal runs that end at the very end of
//! the payload (no slack after them to over-read).
#![allow(clippy::expect_used, clippy::cast_possible_truncation)]

use crush_lz::decompress;

mod common;

use common::{last, one_block, seq};

/// Byte-at-a-time LZ77 copy: the reference the fast paths must match.
fn reference_match(out: &mut Vec<u8>, offset: usize, len: usize) {
    for _ in 0..len {
        out.push(out[out.len() - offset]);
    }
}

fn distinct(len: usize, salt: u8) -> Vec<u8> {
    (0..len)
        .map(|i| (i as u8).wrapping_mul(31) ^ salt)
        .collect()
}

#[test]
fn every_short_offset_and_length() {
    let lengths: Vec<usize> = (4..=40)
        .chain([47, 48, 49, 63, 64, 65, 200, 1_000])
        .collect();
    for offset in 1..=40usize {
        for &mlen in &lengths {
            for tail in [0usize, 1, 7] {
                // literals before the match cover the offset
                let lit = distinct(offset.max(5), offset as u8);
                let mut expect = lit.clone();
                reference_match(&mut expect, offset, mlen);
                let tail_lits = distinct(tail, 0xA5);
                expect.extend_from_slice(&tail_lits);

                let mut p = seq(&lit, offset as u16, mlen);
                p.extend(last(&tail_lits));
                let out = decompress(&one_block(&p, expect.len() as u32), expect.len())
                    .expect("valid block");
                assert_eq!(out, expect, "offset {offset} mlen {mlen} tail {tail}");
            }
        }
    }
}

#[test]
fn back_to_back_matches_overwrite_overshoot() {
    // Several short matches in a row: anything a fast path writes past a
    // match's end must be overwritten by the next sequence.
    let mut expect = distinct(64, 3);
    let mut p = Vec::new();
    let mut first = true;
    for (i, (offset, mlen)) in [
        (1usize, 5usize),
        (3, 4),
        (17, 9),
        (64, 33),
        (2, 19),
        (40, 4),
    ]
    .into_iter()
    .enumerate()
    {
        let lit = if first {
            distinct(64, 3)
        } else {
            distinct(i, i as u8)
        };
        if !first {
            expect.extend_from_slice(&lit);
        }
        first = false;
        p.extend(seq(&lit, offset as u16, mlen));
        reference_match(&mut expect, offset, mlen);
    }
    p.extend(last(&[]));
    let out = decompress(&one_block(&p, expect.len() as u32), expect.len()).expect("valid");
    assert_eq!(out, expect);
}

#[test]
fn literal_runs_ending_at_payload_end() {
    for n in 1..=40usize {
        let lit = distinct(n, n as u8);
        let out = decompress(&one_block(&last(&lit), n as u32), n).expect("valid");
        assert_eq!(out, lit, "final literal run of {n}");

        // short literal + match, then the final literal run
        let mut expect = distinct(n.max(4), 9);
        let mut p = seq(&expect.clone(), 4, 4);
        reference_match(&mut expect, 4, 4);
        p.extend(last(&lit));
        expect.extend_from_slice(&lit);
        let out = decompress(&one_block(&p, expect.len() as u32), expect.len()).expect("valid");
        assert_eq!(out, expect, "match then final literal run of {n}");
    }
}

#[test]
fn match_ends_exactly_at_block_end() {
    // 64-kilobyte block filled to the last byte by one long overlapping match.
    let lit = distinct(3, 1);
    let mut expect = lit.clone();
    reference_match(&mut expect, 3, 65_536 - 3);
    let mut p = seq(&lit, 3, 65_536 - 3);
    p.extend(last(&[]));
    let out = decompress(&one_block(&p, 65_536), 65_536).expect("valid");
    assert_eq!(out, expect);
}
