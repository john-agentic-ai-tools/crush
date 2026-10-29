//! In-memory single-thread timing of `fast-lz` on one file, comparable with
//! `lz4 -b`: prints the best and median of N runs for compression and
//! decompression, and the ratio.
//!
//! ```text
//! cargo run --release -p crush-lz --example codec_bench -- FILE [RUNS]
//! ```
#![allow(clippy::expect_used, clippy::cast_precision_loss)]

use crush_lz::{FastLzConfig, compress, decompress};
use std::time::Instant;

fn mbs(bytes: usize, secs: f64) -> f64 {
    bytes as f64 / 1e6 / secs
}

fn stats(mut t: Vec<f64>) -> (f64, f64) {
    t.sort_by(f64::total_cmp);
    (t[0], t[t.len() / 2])
}

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: codec_bench FILE [RUNS]");
    let runs: usize = args.next().map_or(5, |r| r.parse().expect("RUNS"));
    let data = std::fs::read(&path).expect("read input");
    let config = FastLzConfig::default();

    let mut packed = Vec::new();
    let mut ct = Vec::new();
    for _ in 0..runs {
        let t = Instant::now();
        packed = compress(&data, &config).expect("compress");
        ct.push(t.elapsed().as_secs_f64());
    }
    let mut dt = Vec::new();
    for _ in 0..runs {
        let t = Instant::now();
        let out = decompress(&packed, data.len()).expect("decompress");
        dt.push(t.elapsed().as_secs_f64());
        assert_eq!(out.len(), data.len());
    }
    assert_eq!(decompress(&packed, data.len()).expect("decompress"), data);

    let (cb, cm) = stats(ct);
    let (db, dm) = stats(dt);
    println!(
        "fast-lz {} bytes -> {} ({:.3}) compress best {:.1} median {:.1} MB/s decompress best {:.1} median {:.1} MB/s",
        data.len(),
        packed.len(),
        data.len() as f64 / packed.len() as f64,
        mbs(data.len(), cb),
        mbs(data.len(), cm),
        mbs(data.len(), db),
        mbs(data.len(), dm),
    );
}
