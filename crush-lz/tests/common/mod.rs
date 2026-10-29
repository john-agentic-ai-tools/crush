//! Hand-built `fast-lz` frames for decoder tests.
#![allow(dead_code, clippy::cast_possible_truncation)]

use crush_lz::format::{BlockHeader, END_MARKER, FrameHeader};

pub fn frame(log2: u8) -> Vec<u8> {
    FrameHeader::new(log2).expect("header").to_bytes().to_vec()
}

/// Frame (64-kilobyte blocks) + one compressed block with `payload` decoding to `u` bytes + end marker.
pub fn one_block(payload: &[u8], u: u32) -> Vec<u8> {
    let mut v = frame(16);
    v.extend_from_slice(
        &BlockHeader {
            compressed_len: payload.len() as u32,
            stored: false,
            uncompressed_len: u,
        }
        .to_bytes(),
    );
    v.extend_from_slice(payload);
    v.extend_from_slice(&END_MARKER);
    v
}

/// `lit` literals then a match of `mlen` at `offset`.
pub fn seq(lit: &[u8], offset: u16, mlen: usize) -> Vec<u8> {
    let ml = mlen - 4;
    let mut v = vec![((lit.len().min(15) as u8) << 4) | ml.min(15) as u8];
    if lit.len() >= 15 {
        ext(&mut v, lit.len() - 15);
    }
    v.extend_from_slice(lit);
    v.extend_from_slice(&offset.to_le_bytes());
    if ml >= 15 {
        ext(&mut v, ml - 15);
    }
    v
}

/// Final literals-only sequence.
pub fn last(lit: &[u8]) -> Vec<u8> {
    let mut v = vec![(lit.len().min(15) as u8) << 4];
    if lit.len() >= 15 {
        ext(&mut v, lit.len() - 15);
    }
    v.extend_from_slice(lit);
    v
}

pub fn ext(v: &mut Vec<u8>, mut n: usize) {
    while n >= 255 {
        v.push(255);
        n -= 255;
    }
    v.push(n as u8);
}
