#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::cast_possible_truncation
)]

use criterion::{criterion_group, criterion_main, Criterion};
use crush_parallel::{compress, decompress_block, load_index, EngineConfiguration};
use std::io::Cursor;

fn bench_random_access(c: &mut Criterion) {
    // Create a multi-block compressed file
    let data: Vec<u8> = b"random access benchmark"
        .iter()
        .cycle()
        .take(64 * 1024 * 1024) // 64 MB → 64 blocks at 1 MB each
        .copied()
        .collect();

    let config = EngineConfiguration::builder()
        .block_size(1_048_576)
        .build()
        .expect("config");
    let compressed = compress(&data, &config).expect("compress");

    let mut cursor = Cursor::new(&compressed);
    let index = load_index(&mut cursor).expect("load_index");
    let last_block = index.len() - 1;

    let mut group = c.benchmark_group("random_access");
    group.sample_size(50);

    group.bench_function("decompress_last_block", |b| {
        b.iter(|| {
            let mut c = Cursor::new(&compressed);
            decompress_block(&mut c, &index, last_block, &config).expect("decompress_block")
        });
    });

    group.bench_function("decompress_first_block", |b| {
        b.iter(|| {
            let mut c = Cursor::new(&compressed);
            decompress_block(&mut c, &index, 0, &config).expect("decompress_block")
        });
    });

    group.finish();
}

criterion_group!(benches, bench_random_access);
criterion_main!(benches);
