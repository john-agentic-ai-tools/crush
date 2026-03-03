#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::cast_possible_truncation
)]

use criterion::{criterion_group, criterion_main, Criterion};
use crush_parallel::{compress, decompress_block, load_index, EngineConfiguration};
use std::io::Cursor;

/// Generate a realistic benchmark corpus simulating source-code / log-file content.
///
/// Uses a seeded `XorShift64` PRNG — deterministic, no external dependencies.
/// Yields ~2–4x compression ratio representative of real text workloads.
fn generate_corpus(size: usize, seed: u64) -> Vec<u8> {
    const TOKENS: &[&[u8]] = &[
        b"fn ",
        b"let ",
        b"mut ",
        b"pub ",
        b"use ",
        b"mod ",
        b"struct ",
        b"impl ",
        b"return ",
        b"match ",
        b"if ",
        b"else ",
        b"for ",
        b"while ",
        b"Vec<u8>",
        b"Result<",
        b"    ",
        b"\n",
        b"// comment\n",
        b"Ok(",
        b"Err(",
        b"Some(",
        b"None",
        b"true",
        b"false",
        b"self",
        b"type ",
        b"trait ",
        b"where ",
        b"0x",
        b"ERROR: ",
        b"WARN: ",
        b"INFO: ",
        b"2026-02-",
        b"::new()",
    ];

    let mut data = Vec::with_capacity(size);
    let mut state = seed;

    while data.len() < size {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;

        if state.is_multiple_of(3) {
            data.push(((state >> 8) & 0x5F) as u8 + 32);
        } else {
            let token = TOKENS[(state as usize >> 4) % TOKENS.len()];
            let remaining = size - data.len();
            data.extend_from_slice(&token[..token.len().min(remaining)]);
        }
    }

    data.truncate(size);
    data
}

fn bench_random_access(c: &mut Criterion) {
    // 64 MB realistic corpus → 64 blocks at 1 MB each
    let data = generate_corpus(64 * 1024 * 1024, 0xABCD_EF01_2345_6789);

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
