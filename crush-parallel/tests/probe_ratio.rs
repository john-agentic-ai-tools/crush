//! Ratio report for the incompressibility probe: compresses the worst-case
//! and realistic variants with the probe on and off and prints the loss.
//!
//! Run with `cargo test -p crush-parallel --release --test probe_ratio -- --ignored --nocapture`.
#![allow(clippy::expect_used, clippy::cast_precision_loss)]

#[path = "../benches/common/variants.rs"]
mod variants;

use crush_parallel::{EngineConfiguration, compress, decompress};

fn size_with(data: &[u8], probe: bool) -> usize {
    let config = EngineConfiguration::builder()
        .incompressible_probe(probe)
        .build()
        .expect("config");
    let out = compress(data, &config).expect("compress");
    assert_eq!(decompress(&out, &config).expect("decompress"), data);
    out.len()
}

#[test]
#[ignore = "report, not a gate: run with --ignored --nocapture"]
fn probe_ratio_report() {
    const SIZE: usize = 64 * 1024 * 1024;
    let text = variants::generate_corpus(8 * 1024 * 1024, 0x5EED_0000_0000_0001);
    let inputs = [
        ("random", variants::random(SIZE)),
        (
            "corpus",
            variants::generate_corpus(SIZE, 0xDEAD_BEEF_CAFE_1234),
        ),
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
    println!("| input | probe off | probe on | ratio off | ratio on | size change |");
    println!("|---|---|---|---|---|---|");
    for (name, data) in &inputs {
        let off = size_with(data, false);
        let on = size_with(data, true);
        println!(
            "| {name} | {off} | {on} | {:.4} | {:.4} | {:+.3}% |",
            data.len() as f64 / off as f64,
            data.len() as f64 / on as f64,
            (on as f64 / off as f64 - 1.0) * 100.0
        );
    }
}
