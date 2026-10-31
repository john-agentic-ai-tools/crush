//! One codec operation on the data of the `compress_*`/`decompress_*` criterion
//! benches, for `bench.py callgrind`. Only `measured` is counted (valgrind's
//! `--toggle-collect`), so setup doesn't appear in the instruction counts.
//!
//! ```text
//! cargo run --release -p crush-core --example callgrind_codec -- compress|decompress SIZE
//! ```
#![allow(clippy::expect_used)]

use crush_core::{compress, decompress, init_plugins};
use std::hint::black_box;

#[inline(never)]
fn measured(op: &str, data: &[u8], packed: &[u8]) {
    if op == "compress" {
        black_box(compress(black_box(data)).expect("compress"));
    } else {
        black_box(decompress(black_box(packed)).expect("decompress"));
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let op = args.next().expect("usage: callgrind_codec OP SIZE");
    let size: usize = args.next().expect("SIZE").parse().expect("SIZE");
    init_plugins().expect("init_plugins");
    let data = vec![0x42u8; size];
    let packed = compress(&data).expect("compress");
    measured(&op, &data, &packed);
}
