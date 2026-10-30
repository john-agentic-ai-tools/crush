//! Hand-built `fast-lz` frames for decoder tests, and test-data generators.
#![allow(
    dead_code,
    clippy::cast_possible_truncation,
    clippy::missing_panics_doc
)]

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

pub fn xorshift(len: usize, mut s: u64) -> Vec<u8> {
    (0..len)
        .map(|_| {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            s.to_le_bytes()[0]
        })
        .collect()
}

/// Standard base64 of `data`, wrapped at 76 columns like MIME.
pub fn base64(data: &[u8]) -> Vec<u8> {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::with_capacity(data.len() * 4 / 3 + data.len() / 57 + 4);
    for (n, chunk) in data.chunks(3).enumerate() {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let v = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for k in 0..4 {
            out.push(A[((v >> (18 - 6 * k)) & 63) as usize]);
        }
        if n % 19 == 18 {
            out.push(b'\n');
        }
    }
    out
}

/// Log-like text, `len` bytes: fixed fields with varying timestamps and ids.
pub fn log_text(len: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(len + 128);
    let mut i = 0u64;
    while out.len() < len {
        out.extend_from_slice(
            format!(
                "2026-10-29 12:{:02}:{:02}.{:03} INFO request id={} path=/api/v1/items/{} status=200 ms={}
",
                (i / 60) % 60,
                i % 60,
                (i * 7) % 1000,
                i * 2_654_435_761 % 1_000_000,
                i % 977,
                i % 89
            )
            .as_bytes(),
        );
        i += 1;
    }
    out.truncate(len);
    out
}
