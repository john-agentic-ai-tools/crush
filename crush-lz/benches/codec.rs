//! Criterion benchmarks for the `fast-lz` codec, single thread.
//!
//! Corpora: the synthetic text corpus and random data shared with
//! crush-parallel's benches (64 MB each), plus a real file from
//! `CRUSH_BENCH_CORPUS` when that variable is set (e.g. the 128 MB tar of
//! `/usr/lib` and `/usr/share` used for the lz4 comparison). The
//! `fast_lz_select` group times the selection estimate.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss
)]

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use crush_core::plugin::CompressionAlgorithm;
use crush_core::{PluginSelector, ScoringWeights, init_plugins};
use crush_lz::{FastLzConfig, FastLzPlugin, compress, decompress};
use std::hint::black_box;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

#[path = "../../crush-parallel/benches/common/variants.rs"]
mod variants;

fn corpora() -> Vec<(String, Vec<u8>)> {
    const SIZE: usize = 64 * 1024 * 1024;
    let mut v = vec![
        (
            "text-64MB".to_owned(),
            variants::generate_corpus(SIZE, 0xDEAD_BEEF_CAFE_1234),
        ),
        ("random-64MB".to_owned(), variants::random(SIZE)),
    ];
    if let Ok(path) = std::env::var("CRUSH_BENCH_CORPUS") {
        let data = std::fs::read(&path).expect("read CRUSH_BENCH_CORPUS");
        v.push(("file".to_owned(), data));
    }
    v
}

fn bench_codec(c: &mut Criterion) {
    let config = FastLzConfig::default();
    for (name, data) in corpora() {
        let packed = compress(&data, &config).expect("compress");
        eprintln!(
            "fast-lz {name}: {} -> {} bytes, ratio {:.3}",
            data.len(),
            packed.len(),
            data.len() as f64 / packed.len() as f64
        );

        let mut group = c.benchmark_group("fast_lz");
        group.throughput(Throughput::Bytes(data.len() as u64));
        group.sample_size(10);
        group.bench_with_input(BenchmarkId::new("compress", &name), &data, |b, d| {
            b.iter(|| compress(black_box(d), &config).expect("compress"));
        });
        group.bench_with_input(BenchmarkId::new("decompress", &name), &packed, |b, p| {
            b.iter(|| decompress(black_box(p), data.len()).expect("decompress"));
        });
        // The paths products use: crush_core::decompress goes through the
        // plugin's `decompress`, and the CLI through `decompress_to`.
        let cancel = Arc::new(AtomicBool::new(false));
        group.bench_with_input(
            BenchmarkId::new("decompress_plugin", &name),
            &packed,
            |b, p| {
                b.iter(|| {
                    FastLzPlugin
                        .decompress(black_box(p), Arc::clone(&cancel))
                        .expect("decompress")
                });
            },
        );
        group.bench_with_input(BenchmarkId::new("decompress_to", &name), &packed, |b, p| {
            b.iter(|| {
                FastLzPlugin
                    .decompress_to(black_box(p), Arc::clone(&cancel), &mut std::io::sink())
                    .expect("decompress")
            });
        });
        group.finish();
    }
}

/// Content-aware selection at `--level fast` weights: the sampled estimate
/// plus the selector, on text (feature 023, SC-005).
fn bench_select(c: &mut Criterion) {
    init_plugins().expect("plugins");
    let selector = PluginSelector::new(ScoringWeights::new(0.9, 0.1).expect("weights"));
    let mut group = c.benchmark_group("fast_lz_select");
    for size in [1usize << 20, 16 << 20, 128 << 20] {
        let data = variants::generate_corpus(size, 0xDEAD_BEEF_CAFE_1234);
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(
            BenchmarkId::new("estimate", format!("{}MB", size >> 20)),
            &data,
            |b, d| b.iter(|| FastLzPlugin.estimate_ratio(black_box(d))),
        );
        group.bench_with_input(
            BenchmarkId::new("select_for_input", format!("{}MB", size >> 20)),
            &data,
            |b, d| b.iter(|| selector.select_for_input(black_box(d)).expect("select")),
        );
    }
    group.finish();
}

criterion_group!(benches, bench_codec, bench_select);
criterion_main!(benches);
