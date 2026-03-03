//! `GDeflate` compression ratio benchmarks

#![allow(
    clippy::expect_used,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation
)]

use std::sync::atomic::AtomicBool;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use crush_gpu::engine::{compress, EngineConfig};

/// Varied log text — each line differs slightly, realistic LZ77 matches.
fn generate_log_corpus(size: usize) -> Vec<u8> {
    let mut data = Vec::with_capacity(size);
    let mut counter = 0u32;
    while data.len() < size {
        let line = format!(
            "2026-02-23T10:{:02}:{:02}Z INFO svc_{} - Event id={} status=ok ip=10.0.{}.{}\n",
            (counter / 60) % 60,
            counter % 60,
            counter % 5,
            counter,
            (counter / 256) % 256,
            counter % 256,
        );
        let remaining = size - data.len();
        let bytes = line.as_bytes();
        if remaining >= bytes.len() {
            data.extend_from_slice(bytes);
        } else {
            data.extend_from_slice(&bytes[..remaining]);
        }
        counter += 1;
    }
    data
}

/// Binary counter data — moderately compressible (sequential u32 LE).
fn generate_binary_corpus(size: usize) -> Vec<u8> {
    (0u32..).flat_map(u32::to_le_bytes).take(size).collect()
}

/// Mixed corpus — 50% varied log text + 50% binary counter.
fn generate_mixed_corpus(size: usize) -> Vec<u8> {
    let half = size / 2;
    let mut data = generate_log_corpus(half);
    data.extend_from_slice(&generate_binary_corpus(size - half));
    data
}

fn bench_ratio(c: &mut Criterion) {
    let cancel = AtomicBool::new(false);
    let config = EngineConfig::default();
    let size = 1_048_576; // 1 MB

    let corpora: Vec<(&str, Vec<u8>)> = vec![
        ("log-text", generate_log_corpus(size)),
        ("binary", generate_binary_corpus(size)),
        ("mixed", generate_mixed_corpus(size)),
    ];

    let mut group = c.benchmark_group("compression_ratio");

    for (name, data) in &corpora {
        // Print the actual ratio for visibility
        let compressed = compress(data, &config, &cancel).expect("compress");
        eprintln!(
            "  {name}: {size} -> {} bytes ({:.1}% ratio)",
            compressed.len(),
            compressed.len() as f64 / size as f64 * 100.0
        );

        group.bench_with_input(BenchmarkId::new("gdeflate", *name), data, |b, data| {
            b.iter(|| {
                let compressed = compress(data, &config, &cancel).expect("compress");
                compressed.len() as f64 / data.len() as f64
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_ratio);
criterion_main!(benches);
