//! One `fast-lz` operation on synthetic text, for `bench.py callgrind`. Only
//! `measured` is counted (valgrind's `--toggle-collect`), so setup doesn't
//! appear in the instruction counts.
//!
//! ```text
//! cargo run --release -p crush-lz --example callgrind_codec -- compress|decompress SIZE
//! ```
#![allow(clippy::expect_used)]

use crush_lz::{FastLzConfig, compress, decompress};
use std::hint::black_box;

#[path = "../../crush-parallel/benches/common/variants.rs"]
mod variants;

#[inline(never)]
fn measured(op: &str, data: &[u8], packed: &[u8], config: FastLzConfig) {
    if op == "compress" {
        black_box(compress(black_box(data), &config).expect("compress"));
    } else {
        black_box(decompress(black_box(packed), data.len()).expect("decompress"));
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let op = args.next().expect("usage: callgrind_codec OP SIZE");
    let size: usize = args.next().expect("SIZE").parse().expect("SIZE");
    let data = variants::generate_corpus(size, 0xDEAD_BEEF_CAFE_1234);
    let config = FastLzConfig::default();
    let packed = compress(&data, &config).expect("compress");
    measured(&op, &data, &packed, config);
}
