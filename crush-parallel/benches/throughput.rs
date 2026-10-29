#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::cast_possible_truncation
)]

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use crush_parallel::{EngineConfiguration, compress, decompress, decompress_block, load_index};
use std::hint::black_box;
use std::io::Cursor;

#[path = "common/variants.rs"]
mod variants;

fn bench_compression(c: &mut Criterion) {
    // 128 MB realistic corpus (source-code / log-file character distribution)
    let data = variants::generate_corpus(128 * 1024 * 1024, 0xDEAD_BEEF_CAFE_1234);

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
    let data = variants::generate_corpus(128 * 1024 * 1024, 0xCAFE_BABE_0000_0001);

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
    let data = variants::generate_corpus(64 * 1024 * 1024, 0xFADE_C0DE_BEEF_0007);

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

/// Incompressible input: every block ends up stored raw. pigz is faster than
/// crush on this at 24 cores (perf/comparisons), so it gets its own benchmark.
fn bench_compress_incompressible(c: &mut Criterion) {
    let data = variants::random(128 * 1024 * 1024);

    let mut group = c.benchmark_group("compress_incompressible");
    group.throughput(Throughput::Bytes(data.len() as u64));
    group.sample_size(10);

    // Default configuration (1 MB blocks), as used by `crush compress`.
    for (label, workers) in [("default", 0usize), ("1", 1)] {
        let config = EngineConfiguration::builder()
            .workers(workers)
            .build()
            .expect("config");
        group.bench_with_input(
            BenchmarkId::new(format!("threads={label}"), "random-128MB"),
            &data,
            |b, data| {
                b.iter(|| compress(data, &config).expect("compress"));
            },
        );
    }
    group.finish();
}

/// Same random input through `crush_core::compress_with_options`, the path
/// `crush compress` takes: the difference from `compress_incompressible` is the
/// cost of the crush-core outer format (header, metadata, CRC32, copies).
fn bench_compress_with_options_incompressible(c: &mut Criterion) {
    crush_core::init_plugins().expect("init plugins");
    let data = variants::random(128 * 1024 * 1024);
    let options = crush_core::CompressionOptions::default()
        .with_plugin("parallel-deflate")
        .with_file_metadata(crush_core::plugin::FileMetadata {
            mtime: Some(1_700_000_000),
            #[cfg(unix)]
            permissions: Some(0o644),
        });

    let mut group = c.benchmark_group("compress_with_options_incompressible");
    group.throughput(Throughput::Bytes(data.len() as u64));
    group.sample_size(10);
    group.bench_with_input(
        BenchmarkId::new("threads=default", "random-128MB"),
        &data,
        |b, data| {
            b.iter(|| crush_core::compress_with_options(data, &options).expect("compress"));
        },
    );
    group.finish();
}

/// Worst cases for an incompressibility shortcut, plus a realistic mix: how
/// fast each compresses. `tests/probe_ratio.rs` reports their ratios.
fn bench_compress_mixed(c: &mut Criterion) {
    const SIZE: usize = 64 * 1024 * 1024;
    let text = variants::generate_corpus(8 * 1024 * 1024, 0x5EED_0000_0000_0001);
    let inputs = [
        ("adversarial-gaps", variants::adversarial_gaps(SIZE, &text)),
        (
            "near-incompressible-2pct",
            variants::near_incompressible(SIZE, 2),
        ),
        (
            "near-incompressible-5pct",
            variants::near_incompressible(SIZE, 5),
        ),
        (
            "near-incompressible-10pct",
            variants::near_incompressible(SIZE, 10),
        ),
        ("mixed-archive", variants::mixed_archive(SIZE, &text)),
    ];
    let config = EngineConfiguration::builder().build().expect("config");

    let mut group = c.benchmark_group("compress_mixed");
    group.throughput(Throughput::Bytes(SIZE as u64));
    group.sample_size(10);
    for (name, data) in &inputs {
        group.bench_with_input(
            BenchmarkId::new("threads=default", name),
            data,
            |b, data| {
                b.iter(|| compress(data, &config).expect("compress"));
            },
        );
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_compression,
    bench_compress_incompressible,
    bench_compress_with_options_incompressible,
    bench_compress_mixed,
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
//   All throughput benchmarks use variants::generate_corpus() — a seeded XorShift64 generator that
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
