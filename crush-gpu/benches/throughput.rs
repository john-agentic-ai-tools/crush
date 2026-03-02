//! `GDeflate` compression/decompression throughput benchmarks
//!
//! Uses realistic data corpora to avoid inflated numbers from trivially
//! compressible patterns.

#![allow(clippy::expect_used, clippy::cast_possible_truncation)]

use std::sync::atomic::AtomicBool;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use crush_gpu::engine::{compress, decompress, EngineConfig};

/// Varied log text — each line differs slightly, giving realistic LZ77 matches
/// without the trivial "same 63-byte pattern repeated" problem.
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

fn bench_compress(c: &mut Criterion) {
    let cancel = AtomicBool::new(false);
    let config = EngineConfig::default();

    let mut group = c.benchmark_group("compress_throughput");

    let corpora: Vec<(&str, Vec<u8>)> = vec![
        ("log-1MB", generate_log_corpus(1_048_576)),
        ("binary-1MB", generate_binary_corpus(1_048_576)),
        ("mixed-1MB", generate_mixed_corpus(1_048_576)),
        ("mixed-10MB", generate_mixed_corpus(10_485_760)),
    ];

    for (label, data) in &corpora {
        group.throughput(Throughput::Bytes(data.len() as u64));
        group.bench_with_input(BenchmarkId::new("cpu", *label), data, |b, data| {
            b.iter(|| compress(data, &config, &cancel));
        });
    }

    group.finish();
}

fn bench_decompress(c: &mut Criterion) {
    let cancel = AtomicBool::new(false);
    let config = EngineConfig::default();
    let cpu_config = EngineConfig {
        force_cpu: true,
        ..config.clone()
    };

    let mut group = c.benchmark_group("decompress_throughput");

    let corpora: Vec<(&str, Vec<u8>)> = vec![
        ("log-1MB", generate_log_corpus(1_048_576)),
        ("binary-1MB", generate_binary_corpus(1_048_576)),
        ("mixed-1MB", generate_mixed_corpus(1_048_576)),
        ("mixed-10MB", generate_mixed_corpus(10_485_760)),
    ];

    for (label, data) in &corpora {
        let compressed = compress(data, &config, &cancel).expect("compress should succeed");

        group.throughput(Throughput::Bytes(data.len() as u64));

        // GPU-enabled (auto-detect)
        group.bench_with_input(
            BenchmarkId::new("gpu", *label),
            &compressed,
            |b, compressed| {
                b.iter(|| decompress(compressed, &config, &cancel));
            },
        );

        // CPU-only
        group.bench_with_input(
            BenchmarkId::new("cpu", *label),
            &compressed,
            |b, compressed| {
                b.iter(|| decompress(compressed, &cpu_config, &cancel));
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_compress, bench_decompress);
criterion_main!(benches);
