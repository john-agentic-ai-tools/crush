//! T041: price the sprint prototype on a real file, one core, in memory.
//!
//! Times the match finder, Huffman encoding and four-stream Huffman decoding,
//! and estimates the ratio. Sequences have no coder yet, so their size is the
//! order-0 entropy of the codes per segment plus extra bits, and each Huffman
//! table counts as 4 bits per symbol up to the last one used.
//!
//! `cargo run --release -p crush-sprint --example codec_bench -- FILE [RUNS]
//! [LONG_LOG] [SHORT_LOG] [LITERALS_OUT]`
//!
//! `LITERALS_OUT` writes the literal bytes, for `zstd -b` to price its own
//! Huffman decoder on the same data.
#![allow(clippy::expect_used, clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use crush_sprint::huffman::{HuffmanDecoder, HuffmanEncoder, build_code_lengths};
use crush_sprint::matcher::{Matcher, Sequence};
use std::time::Instant;

const BLOCK: usize = 4 << 20;
const SEGMENT: usize = 128 << 10;

/// One segment's output from the match stage.
#[derive(Default)]
struct Segment {
    literals: Vec<u8>,
    sequences: Vec<Sequence>,
}

fn match_stage(data: &[u8], matcher: &mut Matcher, segments: &mut Vec<Segment>) {
    segments.clear();
    for block in data.chunks(BLOCK) {
        matcher.reset();
        let mut repeats = [1, 4, 8];
        let mut start = 0;
        while start < block.len() {
            let end = (start + SEGMENT).min(block.len());
            let mut segment = Segment::default();
            segment.sequences.reserve(SEGMENT / 16);
            segment.literals.reserve(SEGMENT / 4);
            let tail = matcher.find(block, start, end, &mut repeats, &mut segment.sequences);
            let mut at = start;
            for s in &segment.sequences {
                let lits = s.literal_length as usize;
                segment.literals.extend_from_slice(&block[at..at + lits]);
                at += lits + s.match_length as usize;
            }
            segment.literals.extend_from_slice(&block[tail..end]);
            segments.push(segment);
            start = end;
        }
    }
}

struct Coded {
    bytes: Vec<u8>,
    lengths: [u8; 256],
    len: usize,
}

fn huffman_stage(segments: &[Segment], coded: &mut Vec<Coded>) -> usize {
    coded.clear();
    let mut raw = 0;
    for segment in segments {
        let mut histogram = [0u32; 256];
        for &b in &segment.literals {
            histogram[usize::from(b)] += 1;
        }
        if segment.literals.len() < 64 {
            raw += segment.literals.len();
            continue;
        }
        let Ok(lengths) = build_code_lengths(&histogram) else {
            continue;
        };
        let encoder = HuffmanEncoder::from_lengths(&lengths).expect("lengths");
        let mut bytes = Vec::with_capacity(segment.literals.len());
        encoder
            .encode_4_streams(&segment.literals, &mut bytes)
            .expect("encode");
        if bytes.len() < segment.literals.len() {
            coded.push(Coded {
                bytes,
                lengths,
                len: segment.literals.len(),
            });
        } else {
            raw += segment.literals.len();
        }
    }
    raw
}

fn entropy_bytes(counts: &[u32]) -> f64 {
    let total: u32 = counts.iter().sum();
    let t = f64::from(total);
    counts
        .iter()
        .filter(|&&c| c != 0)
        .map(|&c| f64::from(c) * (t / f64::from(c)).log2())
        .sum::<f64>()
        / 8.0
}

fn value_code(v: u32) -> (usize, u32) {
    // RFC 8878 literal and match length codes, near enough for an estimate:
    // exact below 16, then about two codes per doubling.
    if v < 16 {
        (v as usize, 0)
    } else {
        let log = v.ilog2();
        let half = (v >> (log - 1)) & 1;
        ((16 + (log - 4) * 2 + half) as usize, log - 1)
    }
}

/// Order-0 entropy of the codes per segment, extra bits, and a small table
/// cost per field. Offset values carry repeat codes as the format defines.
fn sequence_bytes(segments: &[Segment]) -> f64 {
    let mut total = 0.0;
    let mut repeats = [1u32, 4, 8];
    for segment in segments {
        let (mut ll, mut ml, mut of) = ([0u32; 64], [0u32; 64], [0u32; 64]);
        let mut extra = 0u64;
        for s in &segment.sequences {
            let (code, bits) = value_code(s.literal_length);
            ll[code.min(63)] += 1;
            extra += u64::from(bits);
            let (code, bits) = value_code(s.match_length.saturating_sub(3));
            ml[code.min(63)] += 1;
            extra += u64::from(bits);
            let value = if s.offset == repeats[0] {
                1
            } else if s.offset == repeats[1] {
                repeats.swap(0, 1);
                2
            } else {
                repeats = [s.offset, repeats[0], repeats[1]];
                s.offset + 3
            };
            let code = value.ilog2();
            of[code as usize] += 1;
            extra += u64::from(code);
        }
        if !segment.sequences.is_empty() {
            total += entropy_bytes(&ll) + entropy_bytes(&ml) + entropy_bytes(&of);
            total += extra as f64 / 8.0 + 30.0;
        }
    }
    total
}

fn median(values: &mut [f64]) -> f64 {
    values.sort_by(f64::total_cmp);
    values[values.len() / 2]
}

/// The whole codec, block by block, on one thread.
fn full_codec(data: &[u8], runs: usize) {
    use crush_parallel::codec::BlockCodec;
    use crush_sprint::SprintCodec;

    let codec = SprintCodec;
    let mut encoder = codec.encoder().expect("encoder");
    let mut decoder = codec.decoder().expect("decoder");
    let (mut tc, mut td) = (Vec::new(), Vec::new());
    let mut size = 0;
    let mut output = vec![0u8; BLOCK];
    for _ in 0..runs {
        let mut payloads = Vec::new();
        let t = Instant::now();
        for block in data.chunks(BLOCK) {
            let mut out = Vec::with_capacity(BLOCK / 2);
            codec
                .compress_block(&mut encoder, block, &mut out)
                .expect("compress");
            payloads.push(out);
        }
        tc.push(t.elapsed().as_secs_f64());
        size = payloads.iter().map(Vec::len).sum();
        let t = Instant::now();
        for (payload, block) in payloads.iter().zip(data.chunks(BLOCK)) {
            codec
                .decompress_block(&mut decoder, payload, &mut output[..block.len()])
                .expect("decompress");
        }
        td.push(t.elapsed().as_secs_f64());
    }
    let mb = data.len() as f64 / 1e6;
    println!(
        "full codec: ratio {:.4}, compress {:.1} MB/s, decompress {:.1} MB/s",
        data.len() as f64 / size as f64,
        mb / median(&mut tc),
        mb / median(&mut td)
    );
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).expect("FILE");
    let number = |i: usize, default: u32| {
        args.get(i)
            .map_or(default, |a| a.parse().expect("a number"))
    };
    let runs = number(2, 5) as usize;
    let long_log = number(3, 18);
    let short_log = number(4, 16);
    let data = std::fs::read(path).expect("read");
    let mb = data.len() as f64 / 1e6;
    full_codec(&data, runs);

    let mut matcher = Matcher::new(long_log, short_log).expect("matcher");
    let mut segments = Vec::new();
    let mut coded = Vec::new();
    let (mut tm, mut te, mut td) = (Vec::new(), Vec::new(), Vec::new());
    let mut raw = 0;
    for _ in 0..runs {
        let t = Instant::now();
        match_stage(&data, &mut matcher, &mut segments);
        tm.push(t.elapsed().as_secs_f64());

        let t = Instant::now();
        raw = huffman_stage(&segments, &mut coded);
        te.push(t.elapsed().as_secs_f64());

        let decoders: Vec<HuffmanDecoder> = coded
            .iter()
            .map(|c| HuffmanDecoder::from_lengths(&c.lengths).expect("lengths"))
            .collect();
        let mut out = vec![0u8; SEGMENT];
        let t = Instant::now();
        for (c, d) in coded.iter().zip(&decoders) {
            d.decode_4_streams(&c.bytes, &mut out[..c.len])
                .expect("decode");
        }
        td.push(t.elapsed().as_secs_f64());
    }

    let literal_bytes: usize = segments.iter().map(|s| s.literals.len()).sum();
    let sequences: usize = segments.iter().map(|s| s.sequences.len()).sum();
    let huffman_bytes: usize = coded.iter().map(|c| c.bytes.len()).sum();
    let tables: usize = coded
        .iter()
        .map(|c| c.lengths.iter().rposition(|&l| l != 0).map_or(0, |p| p + 2) / 2)
        .sum();
    let decoded: usize = coded.iter().map(|c| c.len).sum();
    let seq_bytes = sequence_bytes(&segments);
    let total = (huffman_bytes + tables + raw) as f64 + seq_bytes + segments.len() as f64 * 8.0;

    let (tm, te, td) = (median(&mut tm), median(&mut te), median(&mut td));
    println!("tables 2^{long_log} 8-byte, 2^{short_log} 5-byte, {runs} runs, {mb:.0} MB");
    println!("match stage     {:8.1} MB/s of input", mb / tm);
    println!("huffman encode  {:8.1} MB/s of input", mb / te);
    println!(
        "huffman decode  {:8.1} MB/s of literals",
        decoded as f64 / 1e6 / td
    );
    println!("match + encode  {:8.1} MB/s of input", mb / (tm + te));
    println!(
        "literals {:.1}% of input, {sequences} sequences, {:.2} bytes per sequence",
        literal_bytes as f64 * 100.0 / data.len() as f64,
        seq_bytes / sequences as f64
    );
    println!(
        "size: huffman {huffman_bytes} + tables {tables} + raw {raw} + sequences {seq_bytes:.0}"
    );
    println!("estimated ratio {:.4}", data.len() as f64 / total);

    if let Some(out) = args.get(5) {
        let all: Vec<u8> = segments
            .iter()
            .flat_map(|s| s.literals.iter().copied())
            .collect();
        std::fs::write(out, all).expect("write literals");
    }
}
