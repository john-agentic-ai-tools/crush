//! Input generators for incompressible and mixed workloads, shared by the
//! `throughput` bench and the ratio report test (`tests/probe_ratio.rs`).
//!
//! The probe these target samples `PROBE_SAMPLES` slices of `PROBE_SAMPLE_LEN`
//! bytes at evenly spaced offsets in each block (see `block.rs`).
#![allow(dead_code, clippy::cast_possible_truncation)]

pub const BLOCK: usize = 1024 * 1024;
pub const PROBE_SAMPLE_LEN: usize = 16 * 1024;
pub const PROBE_SAMPLES: usize = 8;

struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn fill(&mut self, out: &mut [u8]) {
        for chunk in out.chunks_mut(8) {
            let v = self.next().to_le_bytes();
            chunk.copy_from_slice(&v[..chunk.len()]);
        }
    }
}

/// Incompressible input: `XorShift64` output bytes.
#[must_use]
pub fn random(size: usize) -> Vec<u8> {
    let mut out = vec![0u8; size];
    XorShift(0x9E37_79B9_7F4A_7C15).fill(&mut out);
    out
}

/// The probe's blind spot by construction: random bytes exactly where the
/// probe samples each block, compressible `text` everywhere else.
#[must_use]
pub fn adversarial_gaps(size: usize, text: &[u8]) -> Vec<u8> {
    let mut out: Vec<u8> = text.iter().copied().cycle().take(size).collect();
    let mut rng = XorShift(0x0123_4567_89AB_CDEF);
    for block_start in (0..size).step_by(BLOCK) {
        let block_len = BLOCK.min(size - block_start);
        for k in 0..PROBE_SAMPLES {
            let off = block_start + k * block_len / PROBE_SAMPLES;
            let end = (off + PROBE_SAMPLE_LEN).min(block_start + block_len);
            rng.fill(&mut out[off..end]);
        }
    }
    out
}

/// Random data where about `percent`% of bytes are 32-byte repeats of
/// earlier data: full DEFLATE shrinks it a little, a small sample may not.
#[must_use]
pub fn near_incompressible(size: usize, percent: u64) -> Vec<u8> {
    const RUN: usize = 32;
    let mut out = random(size);
    let mut rng = XorShift(0xDEAD_BEEF_0BAD_F00D ^ percent);
    let mut pos = 4096;
    while pos + RUN < size {
        // one repeat per RUN*100/percent bytes on average
        let gap = (rng.next() % (2 * RUN as u64 * 100 / percent.max(1))) as usize;
        pos += gap.max(RUN);
        if pos + RUN >= size {
            break;
        }
        let back = 1 + (rng.next() % 4000) as usize;
        let src = pos - back;
        for i in 0..RUN {
            out[pos + i] = out[src + i];
        }
        pos += RUN;
    }
    out
}

/// A tar-like mix: segments of already-compressed data (DEFLATE output of
/// `text`) and plain `text`, 64 KB to 3 MB long, not aligned to blocks.
#[must_use]
pub fn mixed_archive(size: usize, text: &[u8]) -> Vec<u8> {
    let mut rng = XorShift(0xFEED_FACE_CAFE_BEEF);
    let mut comp = libdeflater::Compressor::new(libdeflater::CompressionLvl::default());
    let mut out = Vec::with_capacity(size);
    let mut text_pos = 0usize;
    let mut compressed_segment = rng.next().is_multiple_of(2);
    while out.len() < size {
        let len = (64 * 1024 + (rng.next() % (3 * 1024 * 1024)) as usize).min(size - out.len());
        if compressed_segment {
            // Compress ~3x as much text so the output covers `len` bytes.
            let mut seg = Vec::new();
            while seg.len() < len {
                let src: Vec<u8> = text
                    .iter()
                    .copied()
                    .cycle()
                    .skip(text_pos)
                    .take(4 * len)
                    .collect();
                text_pos = (text_pos + 4 * len) % text.len();
                let mut buf = vec![0u8; comp.deflate_compress_bound(src.len())];
                let n = comp.deflate_compress(&src, &mut buf).unwrap_or(0);
                seg.extend_from_slice(&buf[..n]);
            }
            out.extend_from_slice(&seg[..len]);
        } else {
            out.extend(text.iter().copied().cycle().skip(text_pos).take(len));
            text_pos = (text_pos + len) % text.len();
        }
        compressed_segment = !compressed_segment;
    }
    out
}

/// Generate a realistic benchmark corpus simulating source-code / log-file content.
///
/// Uses a seeded `XorShift64` PRNG — deterministic, no external dependencies.
/// Mixes code vocabulary tokens (structure/repetition) with pseudo-random ASCII
/// bytes (entropy variation), yielding a ~2–4x compression ratio representative
/// of real text workloads.
#[must_use]
pub fn generate_corpus(size: usize, seed: u64) -> Vec<u8> {
    const TOKENS: &[&[u8]] = &[
        b"fn ",
        b"let ",
        b"mut ",
        b"pub ",
        b"use ",
        b"mod ",
        b"struct ",
        b"impl ",
        b"return ",
        b"match ",
        b"if ",
        b"else ",
        b"for ",
        b"while ",
        b"Vec<u8>",
        b"Result<",
        b"    ",
        b"\n",
        b"// comment\n",
        b"Ok(",
        b"Err(",
        b"Some(",
        b"None",
        b"true",
        b"false",
        b"self",
        b"type ",
        b"trait ",
        b"where ",
        b"0x",
        b"ERROR: ",
        b"WARN: ",
        b"INFO: ",
        b"2026-02-",
        b"::new()",
    ];

    let mut data = Vec::with_capacity(size);
    let mut state = seed;

    while data.len() < size {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;

        if state.is_multiple_of(3) {
            // ~33%: pseudo-random printable ASCII byte (adds entropy)
            data.push(((state >> 8) & 0x5F) as u8 + 32);
        } else {
            // ~67%: vocabulary token (adds structure and repetition)
            let token = TOKENS[(state as usize >> 4) % TOKENS.len()];
            let remaining = size - data.len();
            data.extend_from_slice(&token[..token.len().min(remaining)]);
        }
    }

    data.truncate(size);
    data
}
