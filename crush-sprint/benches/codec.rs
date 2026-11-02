//! Criterion benchmarks for the sprint codec, one block at a time on one
//! thread: the synthetic text corpus and random data shared with
//! crush-parallel's benches (64 MB each), plus a real file from
//! `CRUSH_BENCH_CORPUS` when that variable is set.
#![allow(clippy::expect_used, clippy::cast_precision_loss)]

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use crush_parallel::codec::BlockCodec;
use crush_sprint::SprintCodec;
use crush_sprint::format::BLOCK_SIZE;
use std::hint::black_box;

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
        v.push((
            "file".to_owned(),
            std::fs::read(&path).expect("read CRUSH_BENCH_CORPUS"),
        ));
    }
    v
}

fn compress(data: &[u8]) -> Vec<Option<Vec<u8>>> {
    let codec = SprintCodec;
    let mut encoder = codec.encoder().expect("encoder");
    data.chunks(BLOCK_SIZE)
        .map(|block| {
            let mut out = Vec::new();
            codec
                .compress_block(&mut encoder, block, &mut out)
                .expect("compress")
                .then_some(out)
        })
        .collect()
}

fn bench_codec(c: &mut Criterion) {
    let codec = SprintCodec;
    for (name, data) in corpora() {
        let packed = compress(&data);
        let size: usize = packed
            .iter()
            .zip(data.chunks(BLOCK_SIZE))
            .map(|(p, block)| p.as_ref().map_or(block.len(), Vec::len))
            .sum();
        eprintln!(
            "sprint {name}: {} -> {size} bytes, ratio {:.3}",
            data.len(),
            data.len() as f64 / size as f64
        );

        let mut group = c.benchmark_group("sprint");
        group.sample_size(10);
        group.throughput(Throughput::Bytes(data.len() as u64));
        group.bench_with_input(BenchmarkId::new("compress", &name), &data, |b, data| {
            b.iter(|| compress(black_box(data)));
        });
        group.bench_with_input(BenchmarkId::new("decompress", &name), &data, |b, data| {
            let mut decoder = codec.decoder().expect("decoder");
            let mut output = vec![0u8; BLOCK_SIZE];
            b.iter(|| {
                for (payload, block) in packed.iter().zip(data.chunks(BLOCK_SIZE)) {
                    let output = &mut output[..block.len()];
                    match payload {
                        Some(payload) => codec
                            .decompress_block(&mut decoder, payload, output)
                            .expect("decompress"),
                        // The engine copies a stored block.
                        None => output.copy_from_slice(block),
                    }
                }
                black_box(output[0])
            });
        });
        group.finish();
    }
}

criterion_group!(benches, bench_codec);
criterion_main!(benches);
