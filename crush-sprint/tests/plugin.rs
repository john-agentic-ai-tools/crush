//! T035: the plugin as crush-core sees it.
#![allow(clippy::expect_used)]

use crush_core::plugin::{CompressionAlgorithm, PluginSelector, ScoringWeights};
use crush_core::{CompressionOptions, CrushError, compress_with_options, decompress, init_plugins};
use crush_sprint::{PLUGIN_MAGIC, PLUGIN_NAME, SprintPlugin};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

fn no_cancel() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}

fn sample() -> Vec<u8> {
    b"plain text that compresses well enough for every plugin to want it\n".repeat(20_000)
}

#[test]
fn name_and_magic() {
    assert_eq!(PLUGIN_NAME, "sprint");
    assert_eq!(PLUGIN_MAGIC, [0x43, 0x52, 0x01, 0x05]);
    let meta = SprintPlugin.metadata();
    assert_eq!(meta.name, "sprint");
    assert_eq!(meta.magic_number, PLUGIN_MAGIC);
    assert_eq!(SprintPlugin.name(), "sprint");
}

#[test]
fn detect_accepts_its_container_only() {
    let packed = SprintPlugin
        .compress(&sample(), no_cancel())
        .expect("compress");
    assert!(SprintPlugin.detect(&packed));
    assert!(!SprintPlugin.detect(b"not a sprint file"));
    assert!(!SprintPlugin.detect(&[]));
}

/// The container is shared with `parallel-deflate`, so byte 22 tells them apart.
#[test]
fn detect_rejects_a_parallel_deflate_container() {
    let config = crush_parallel::EngineConfiguration::builder()
        .build()
        .expect("config");
    let deflate = crush_parallel::engine::compress(&sample(), &config).expect("compress");
    assert!(!SprintPlugin.detect(&deflate));
}

#[test]
fn registered_and_selectable_by_name() {
    init_plugins().expect("init");
    let data = sample();
    let options = CompressionOptions::default().with_plugin("sprint");
    let packed = compress_with_options(&data, &options).expect("compress");
    assert_eq!(packed[..4], PLUGIN_MAGIC);
    let out = decompress(&packed).expect("decompress");
    assert_eq!(out.data, data);
}

#[test]
fn an_already_set_cancel_flag_stops_both_directions() {
    let set = Arc::new(AtomicBool::new(true));
    let data = sample();
    assert!(matches!(
        SprintPlugin.compress(&data, Arc::clone(&set)),
        Err(CrushError::Cancelled)
    ));
    let packed = SprintPlugin.compress(&data, no_cancel()).expect("compress");
    assert!(matches!(
        SprintPlugin.decompress(&packed, set),
        Err(CrushError::Cancelled)
    ));
}

#[test]
fn one_block_comes_back_by_index() {
    let line = b"block retrieval, line after line after line\n";
    let data: Vec<u8> = line.iter().copied().cycle().take(9 << 20).collect();
    let packed = SprintPlugin.compress(&data, no_cancel()).expect("compress");
    let block = SprintPlugin
        .decompress_block(&packed, 1)
        .expect("block")
        .expect("supported");
    assert_eq!(block, data[4 << 20..8 << 20]);
    assert!(SprintPlugin.decompress_block(&packed, 3).is_err());
}

#[test]
fn opted_out_of_automatic_selection() {
    assert!(!SprintPlugin.auto_selectable());
}

/// FR-013: registering sprint changes no automatic choice, even when speed or
/// ratio are all that count and its metadata is the best on offer.
#[test]
fn selection_is_unchanged_with_sprint_registered() {
    init_plugins().expect("init");
    let data = sample();
    for (speed, ratio) in [(1.0, 0.0), (0.0, 1.0), (0.7, 0.3), (0.5, 0.5)] {
        let selector = PluginSelector::new(ScoringWeights::new(speed, ratio).expect("weights"));
        assert_ne!(selector.select().expect("select").name, "sprint");
        assert_ne!(
            selector.select_for_input(&data).expect("select").name,
            "sprint"
        );
    }
    let options = CompressionOptions::default();
    let packed = compress_with_options(&data, &options).expect("compress");
    assert_ne!(packed[..4], PLUGIN_MAGIC);
}

#[test]
fn asking_for_it_by_name_still_works_when_opted_out() {
    init_plugins().expect("init");
    let selector = PluginSelector::new(ScoringWeights::default());
    assert_eq!(
        selector.select_by_name("sprint").expect("named").name,
        "sprint"
    );
}

#[test]
fn corrupt_input_is_an_error_not_a_panic() {
    let mut packed = SprintPlugin
        .compress(&sample(), no_cancel())
        .expect("compress");
    let middle = packed.len() / 2;
    packed[middle] ^= 0xFF;
    assert!(SprintPlugin.decompress(&packed, no_cancel()).is_err());
}
