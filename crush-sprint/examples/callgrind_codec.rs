//! One sprint operation on synthetic text, for `bench.py callgrind`. Only
//! `measured` is counted (valgrind's `--toggle-collect`), so setup doesn't
//! appear in the instruction counts.
//!
//! ```text
//! cargo run --release -p crush-sprint --example callgrind_codec -- compress|decompress SIZE
//! ```
#![allow(clippy::expect_used)]

use crush_parallel::codec::BlockCodec;
use crush_sprint::SprintCodec;
use std::hint::black_box;

#[path = "../../crush-parallel/benches/common/variants.rs"]
mod variants;

fn compress(data: &[u8]) -> Vec<u8> {
    let codec = SprintCodec;
    let mut encoder = codec.encoder().expect("encoder");
    let mut out = Vec::new();
    codec
        .compress_block(&mut encoder, data, &mut out)
        .expect("compress");
    out
}

#[inline(never)]
fn measured(op: &str, data: &[u8], packed: &[u8], output: &mut [u8]) {
    if op == "compress" {
        black_box(compress(black_box(data)));
    } else {
        let codec = SprintCodec;
        let mut decoder = codec.decoder().expect("decoder");
        codec
            .decompress_block(&mut decoder, black_box(packed), output)
            .expect("decompress");
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let op = args.next().expect("usage: callgrind_codec OP SIZE");
    let size: usize = args.next().expect("SIZE").parse().expect("SIZE");
    let data = variants::generate_corpus(size, 0xDEAD_BEEF_CAFE_1234);
    let packed = compress(&data);
    let mut output = vec![0u8; data.len()];
    measured(&op, &data, &packed, &mut output);
}
