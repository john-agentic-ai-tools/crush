//! Selection report (T033, SC-004, SC-005): for each kind of input at
//! 1 MB, 16 MB and 128 MB, the estimate, the plugin chosen at `--level fast`
//! weights, and the estimate's cost as a share of compressing with
//! `fast-lz`. Asserts the routing and the 2% cost bound.
//!
//! ```text
//! cargo test --release -p crush-lz --test selection_report -- --ignored --nocapture
//! ```
//!
//! Set `CRUSH_BENCH_CORPUS` to a real file (e.g. the 128 MB tar used for the
//! lz4 comparison) to include it.
#![allow(
    clippy::expect_used,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation
)]

use crush_core::plugin::CompressionAlgorithm;
use crush_core::{PluginSelector, ScoringWeights, init_plugins};
use crush_lz::{FastLzConfig, FastLzPlugin, PLUGIN_NAME, compress};
use std::time::Instant;

#[path = "../../crush-parallel/benches/common/variants.rs"]
mod variants;

mod common;

use common::{base64, log_text, xorshift};

const SIZES: [usize; 3] = [1 << 20, 16 << 20, 128 << 20];
/// SC-005, with headroom for timer noise on the 1 MB inputs.
const MAX_COST: f64 = 0.02;

/// Fastest of `runs` timings of `f`, in seconds.
fn best_of<T>(runs: usize, mut f: impl FnMut() -> T) -> f64 {
    (0..runs)
        .map(|_| {
            let t = Instant::now();
            std::hint::black_box(f());
            t.elapsed().as_secs_f64()
        })
        .fold(f64::INFINITY, f64::min)
}

/// DEFLATE output all the way through: text deflated in 4 MB pieces with
/// different seeds until `len` bytes.
fn deflated(len: usize) -> Vec<u8> {
    let mut c = libdeflater::Compressor::new(libdeflater::CompressionLvl::default());
    let mut out = Vec::with_capacity(len + (4 << 20));
    let mut seed = 21;
    while out.len() < len {
        let text = variants::generate_corpus(4 << 20, seed);
        let at = out.len();
        out.resize(at + c.deflate_compress_bound(text.len()), 0);
        let n = c.deflate_compress(&text, &mut out[at..]).expect("deflate");
        out.truncate(at + n);
        seed += 1;
    }
    out.truncate(len);
    out
}

/// Input kinds and whether fast-lz must be chosen for them (SC-004).
fn inputs(len: usize) -> Vec<(&'static str, Vec<u8>, bool)> {
    let mut v = vec![
        (
            "source text",
            variants::generate_corpus(len, 0xDEAD_BEEF_CAFE_1234),
            true,
        ),
        ("log text", log_text(len), true),
        ("base64 text", base64(&xorshift(len / 4 * 3, 31)), false),
        ("random", xorshift(len, 32), false),
        ("deflate output", deflated(len), false),
    ];
    if let Ok(path) = std::env::var("CRUSH_BENCH_CORPUS") {
        let mut data = std::fs::read(path).expect("read CRUSH_BENCH_CORPUS");
        if data.len() >= len {
            data.truncate(len);
            v.push(("real file", data, true));
        }
    }
    v
}

#[test]
#[ignore = "report: run in release with --ignored --nocapture"]
fn selection_report() {
    init_plugins().expect("plugins");
    let fast = ScoringWeights::new(0.9, 0.1).expect("weights");
    let selector = PluginSelector::new(fast);
    let config = FastLzConfig::default();
    let mut failures = Vec::new();

    println!("| input | size | estimate | chosen at fast | estimate cost |");
    println!("|---|---|---|---|---|");
    for len in SIZES {
        for (name, data, want_fast) in inputs(len) {
            let estimate = FastLzPlugin.estimate_ratio(&data).unwrap_or(f64::NAN);
            let chosen = selector.select_for_input(&data).expect("select").name;
            let t_est = best_of(5, || FastLzPlugin.estimate_ratio(&data));
            let t_comp = best_of(3, || compress(&data, &config).expect("compress"));
            let cost = t_est / t_comp;
            println!(
                "| {name} | {} MB | {estimate:.3} | {chosen} | {:.2}% |",
                len >> 20,
                cost * 100.0
            );
            if (chosen == PLUGIN_NAME) != want_fast {
                failures.push(format!("{name} {len}: chose {chosen}"));
            }
            if cost > MAX_COST {
                failures.push(format!("{name} {len}: estimate cost {:.2}%", cost * 100.0));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
