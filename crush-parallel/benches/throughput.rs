#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::cast_possible_truncation
)]

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use crush_parallel::{compress, decompress, decompress_block, load_index, EngineConfiguration};
use std::hint::black_box;
use std::io::Cursor;

/// Generate a realistic benchmark corpus simulating source-code / log-file content.
///
/// Uses a seeded `XorShift64` PRNG — deterministic, no external dependencies.
/// Mixes code vocabulary tokens (structure/repetition) with pseudo-random ASCII
/// bytes (entropy variation), yielding a ~2–4x compression ratio representative
/// of real text workloads.
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
            // ~33%: pseudo-random printable ASCII byte (adds entropy)
            data.push(((state >> 8) & 0x5F) as u8 + 32);
        } else {
            // ~67%: vocabulary token (adds structure and repetition)
            let token = TOKENS[(state as usize >> 4) % TOKENS.len()];
            let remaining = size - data.len();
            data.extend_from_slice(&token[..token.len().min(remaining)]);
        }
    }

    data.truncate(size);
    data
}

fn bench_compression(c: &mut Criterion) {
    // 128 MB realistic corpus (source-code / log-file character distribution)
    let data = generate_corpus(128 * 1024 * 1024, 0xDEAD_BEEF_CAFE_1234);

    let mut group = c.benchmark_group("compress_throughput");
    group.throughput(Throughput::Bytes(data.len() as u64));
    group.sample_size(10);

    // workers=0 uses the global rayon pool (all logical CPUs) — the default
    for (label, workers) in [("default", 0usize), ("1", 1), ("2", 2), ("4", 4), ("8", 8)] {
        for block_kb in [64usize, 512, 1024] {
            let block_size = u32::try_from(block_kb * 1024).expect("block_size fits u32");
            let config = EngineConfiguration::builder()
                .workers(workers)
                .block_size(block_size)
                .build()
                .expect("config");

            group.bench_with_input(
                BenchmarkId::new(format!("threads={label}"), format!("block={block_kb}KB")),
                &data,
                |b, data| {
                    b.iter(|| compress(data, &config).expect("compress"));
                },
            );
        }
    }
    group.finish();
}

fn bench_decompression(c: &mut Criterion) {
    // 128 MB realistic corpus — same seed family as compression bench for consistency
    let data = generate_corpus(128 * 1024 * 1024, 0xCAFE_BABE_0000_0001);

    // Use default (global pool) for pre-compression to avoid capping available threads
    let config_compress = EngineConfiguration::builder()
        .block_size(1_048_576)
        .build()
        .expect("config");
    let compressed = compress(&data, &config_compress).expect("compress");

    let mut group = c.benchmark_group("decompress_throughput");
    group.throughput(Throughput::Bytes(data.len() as u64));
    group.sample_size(10);

    // workers=0 uses the global rayon pool (all logical CPUs) — the default
    for (label, workers) in [("default", 0usize), ("1", 1), ("2", 2), ("4", 4), ("8", 8)] {
        let config = EngineConfiguration::builder()
            .workers(workers)
            .block_size(1_048_576)
            .build()
            .expect("config");

        group.bench_with_input(
            BenchmarkId::new("threads", label),
            &compressed,
            |b, compressed| {
                b.iter(|| decompress(compressed, &config).expect("decompress"));
            },
        );
    }
    group.finish();
}

/// T007 — phase-isolated micro-benches.
///
/// These benchmarks deliberately vary block count at fixed total size so the cost
/// attributable to each internal phase (parallel compression vs assembly) is
/// visible when reading criterion output, without exposing private helpers as
/// `pub` (which would break SC-007).
///
/// - `compress_parallel_dominated` — few, large blocks → parallel DEFLATE cost dominates.
/// - `compress_assembly_dominated` — many, small blocks → assembly/index cost dominates.
/// - `decompress_read_phase_only`   — sequential `decompress_block` walk (measures read + single-block decompress).
/// - `decompress_parallel_dominated` — full `decompress` path; compare against `decompress_read_phase_only` to isolate the par-decompress win.
fn bench_phase_isolated(c: &mut Criterion) {
    // 64 MB fixture — small enough to iterate fast, large enough for stable numbers.
    let data = generate_corpus(64 * 1024 * 1024, 0xFADE_C0DE_BEEF_0007);

    let mut group = c.benchmark_group("phase_isolated");
    group.throughput(Throughput::Bytes(data.len() as u64));
    group.sample_size(10);

    let large_block = EngineConfiguration::builder()
        .block_size(4 * 1_048_576)
        .build()
        .expect("config");
    group.bench_with_input(
        BenchmarkId::new("compress_parallel_dominated", "block=4MB"),
        &data,
        |b, data| {
            b.iter(|| compress(data, &large_block).expect("compress"));
        },
    );

    // block_size minimum is 64 KB; this is as small as the config allows, which
    // still produces ~1000 blocks for a 64 MB fixture — assembly/index overhead dominates.
    let small_block = EngineConfiguration::builder()
        .block_size(64 * 1024)
        .build()
        .expect("config");
    group.bench_with_input(
        BenchmarkId::new("compress_assembly_dominated", "block=64KB"),
        &data,
        |b, data| {
            b.iter(|| compress(data, &small_block).expect("compress"));
        },
    );

    // Pre-compress once for the decompress benches.
    let default_cfg = EngineConfiguration::builder()
        .block_size(1_048_576)
        .build()
        .expect("config");
    let compressed = compress(&data, &default_cfg).expect("compress");

    group.bench_with_input(
        BenchmarkId::new("decompress_read_phase_only", "block=1MB"),
        &compressed,
        |b, compressed| {
            b.iter(|| {
                let mut cursor = Cursor::new(compressed);
                let index = load_index(&mut cursor).expect("load_index");
                let mut acc: usize = 0;
                for i in 0..index.len() {
                    let blk =
                        decompress_block(&mut cursor, &index, i, &default_cfg).expect("block");
                    acc = acc.wrapping_add(blk.len());
                }
                black_box(acc)
            });
        },
    );

    group.bench_with_input(
        BenchmarkId::new("decompress_parallel_dominated", "block=1MB"),
        &compressed,
        |b, compressed| {
            b.iter(|| decompress(compressed, &default_cfg).expect("decompress"));
        },
    );

    group.finish();
}

criterion_group!(
    benches,
    bench_compression,
    bench_decompression,
    bench_phase_isolated
);
criterion_main!(benches);

// SC-006 Size comparison results (T074, measured 2026-02-22):
//   Test corpus: 100 MB of real project source files (.rs, .toml, .md)
//   gzip -6 output:            80,248,551 bytes
//   crush-parallel (level 6):  80,330,673 bytes
//   Ratio (crush / gzip):      1.00102 — within 0.1% of gzip ✓ (target: ≤ 1.05)
//
//   Note: Parallel block compression restarts the LZ77 dictionary at each block boundary,
//   losing cross-block back-references. For workloads with mostly local repetition (source
//   code, logs) this is negligible; for files with long-range patterns a larger block size
//   recovers the gap.
//
// Data generation note:
//   All throughput benchmarks use generate_corpus() — a seeded XorShift64 generator that
//   mixes code-vocabulary tokens with pseudo-random ASCII bytes to simulate ~2–4x
//   compressible real-world data. The old cycle() approach produced near-perfectly
//   repetitive input and measured DEFLATE at close to memcpy speed, which was not
//   representative of actual workloads.
//
// Measured results (2026-02-23, realistic corpus, release build, libdeflater + workers wired to rayon):
//   Compression scaling (64 KB blocks):
//     1 thread: 92 MiB/s | 2: 181 | 4: 347 | 8: 576 | default (all cores): 983 MiB/s
//   Compression by block size (default workers):
//     64 KB: 983 MiB/s | 512 KB: 957 MiB/s | 1024 KB: 941 MiB/s
//   Decompression (1024 KB blocks):
//     1 thread: 312 MiB/s | 2: 375 | 4: 417 | 8: 439 | default: 435 MiB/s
//   Random access (64 MB, 1 MB blocks): ~1.12 ms per block
