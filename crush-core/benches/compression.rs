//! Benchmark for compression throughput
//!
//! Performance target: >500 MB/s on 8-core CPU (per constitution)
//! Phase 1 target: Within 5% of gzip (single-threaded)

#![allow(clippy::expect_used)]

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use crush_core::{compress, decompress, init_plugins};
use std::hint::black_box;

fn benchmark_compress_small(c: &mut Criterion) {
    init_plugins().expect("Plugin initialization failed");

    let mut group = c.benchmark_group("compress_small");

    // 1KB of data
    let data = vec![0x42u8; 1024];
    group.throughput(Throughput::Bytes(data.len() as u64));

    group.bench_function("1KB", |b| {
        b.iter(|| {
            let compressed = compress(black_box(&data)).expect("Compression failed");
            black_box(compressed)
        });
    });

    group.finish();
}

fn benchmark_compress_medium(c: &mut Criterion) {
    init_plugins().expect("Plugin initialization failed");

    let mut group = c.benchmark_group("compress_medium");

    // 1MB of data
    let data = vec![0x42u8; 1024 * 1024];
    group.throughput(Throughput::Bytes(data.len() as u64));

    group.bench_function("1MB", |b| {
        b.iter(|| {
            let compressed = compress(black_box(&data)).expect("Compression failed");
            black_box(compressed)
        });
    });

    group.finish();
}

fn benchmark_compress_large(c: &mut Criterion) {
    init_plugins().expect("Plugin initialization failed");

    let mut group = c.benchmark_group("compress_large");
    group.sample_size(10); // Reduce sample size for large data

    // 10MB of data
    let data = vec![0x42u8; 10 * 1024 * 1024];
    group.throughput(Throughput::Bytes(data.len() as u64));

    group.bench_function("10MB", |b| {
        b.iter(|| {
            let compressed = compress(black_box(&data)).expect("Compression failed");
            black_box(compressed)
        });
    });

    group.finish();
}

fn benchmark_decompress_small(c: &mut Criterion) {
    init_plugins().expect("Plugin initialization failed");

    let data = vec![0x42u8; 1024];
    let compressed = compress(&data).expect("Compression failed");

    let mut group = c.benchmark_group("decompress_small");
    group.throughput(Throughput::Bytes(data.len() as u64));

    group.bench_function("1KB", |b| {
        b.iter(|| {
            let decompressed = decompress(black_box(&compressed)).expect("Decompression failed");
            black_box(decompressed)
        });
    });

    group.finish();
}

fn benchmark_decompress_medium(c: &mut Criterion) {
    init_plugins().expect("Plugin initialization failed");

    let data = vec![0x42u8; 1024 * 1024];
    let compressed = compress(&data).expect("Compression failed");

    let mut group = c.benchmark_group("decompress_medium");
    group.throughput(Throughput::Bytes(data.len() as u64));

    group.bench_function("1MB", |b| {
        b.iter(|| {
            let decompressed = decompress(black_box(&compressed)).expect("Decompression failed");
            black_box(decompressed)
        });
    });

    group.finish();
}

fn benchmark_roundtrip(c: &mut Criterion) {
    init_plugins().expect("Plugin initialization failed");

    let mut group = c.benchmark_group("roundtrip");

    for size in &[1024, 10 * 1024, 100 * 1024] {
        let data = vec![0x42u8; *size];
        group.throughput(Throughput::Bytes(*size as u64));

        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| {
                let compressed = compress(black_box(&data)).expect("Compression failed");
                let decompressed =
                    decompress(black_box(&compressed)).expect("Decompression failed");
                black_box(decompressed)
            });
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    benchmark_compress_small,
    benchmark_compress_medium,
    benchmark_compress_large,
    benchmark_decompress_small,
    benchmark_decompress_medium,
    benchmark_roundtrip
);
criterion_main!(benches);
