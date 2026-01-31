//! Benchmark for plugin discovery performance
//!
//! Success Criteria SC-002: Plugin discovery completes in <500ms (100 plugins)
//! Currently only DEFLATE plugin is registered, but infrastructure must support scaling

#![allow(clippy::expect_used)]

use criterion::{criterion_group, criterion_main, Criterion};
use crush_core::{init_plugins, list_plugins};
use std::hint::black_box;

fn benchmark_plugin_discovery(c: &mut Criterion) {
    c.bench_function("plugin_discovery", |b| {
        b.iter(|| {
            // Initialize plugins (discovers from linkme distributed slice)
            let result = init_plugins();
            black_box(result).expect("Plugin initialization failed");
        });
    });
}

fn benchmark_plugin_listing(c: &mut Criterion) {
    // Initialize once for listing benchmarks
    init_plugins().expect("Plugin initialization failed");

    c.bench_function("plugin_listing", |b| {
        b.iter(|| {
            let plugins = list_plugins();
            black_box(plugins)
        });
    });
}

fn benchmark_reinit_plugins(c: &mut Criterion) {
    // Initialize once first
    init_plugins().expect("Initial plugin initialization failed");

    c.bench_function("plugin_reinit", |b| {
        b.iter(|| {
            // Re-initialization should be fast (no-op with RwLock)
            let result = init_plugins();
            black_box(result).expect("Plugin re-initialization failed");
        });
    });
}

criterion_group!(
    benches,
    benchmark_plugin_discovery,
    benchmark_plugin_listing,
    benchmark_reinit_plugins
);
criterion_main!(benches);
