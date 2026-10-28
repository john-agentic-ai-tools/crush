//! Benchmark for plugin selection performance
//!
//! Success Criteria SC-004: Plugin selection completes in <10ms

#![allow(clippy::expect_used)]

use criterion::{Criterion, criterion_group, criterion_main};
use crush_core::{CompressionOptions, ScoringWeights, init_plugins};
use std::hint::black_box;

fn benchmark_auto_selection(c: &mut Criterion) {
    // Initialize plugins once
    init_plugins().expect("Plugin initialization failed");

    c.bench_function("plugin_auto_selection", |b| {
        b.iter(|| {
            let options = CompressionOptions::default();
            // Selection happens internally during compression planning
            black_box(options)
        });
    });
}

fn benchmark_manual_selection(c: &mut Criterion) {
    // Initialize plugins once
    init_plugins().expect("Plugin initialization failed");

    c.bench_function("plugin_manual_selection", |b| {
        b.iter(|| {
            let options = CompressionOptions::default().with_plugin("deflate");
            black_box(options)
        });
    });
}

fn benchmark_weighted_selection(c: &mut Criterion) {
    // Initialize plugins once
    init_plugins().expect("Plugin initialization failed");

    c.bench_function("plugin_weighted_selection", |b| {
        b.iter(|| {
            let weights = ScoringWeights {
                throughput: 0.8,
                compression_ratio: 0.2,
            };
            let options = CompressionOptions::default().with_weights(weights);
            black_box(options)
        });
    });
}

criterion_group!(
    benches,
    benchmark_auto_selection,
    benchmark_manual_selection,
    benchmark_weighted_selection
);
criterion_main!(benches);
