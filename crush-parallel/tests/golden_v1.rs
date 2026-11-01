//! Golden CRSH version 1 files, made before the engine became codec-agnostic.
//!
//! They prove existing archives stay readable and that DEFLATE output keeps its
//! exact bytes (`specs/026-hybrid-speed-codec/contracts/block-codec.md`).
//! Bytes 8..16 hold the engine version, so comparisons skip them.
//!
//! A libdeflate upgrade can legitimately change compressed bytes. Only then,
//! regenerate with:
//! `cargo test -p crush-parallel --test golden_v1 regenerate -- --ignored`
#![allow(clippy::expect_used, clippy::missing_panics_doc)]

use crush_parallel::{EngineConfiguration, compress, decompress};
use std::path::PathBuf;

const ENGINE_VERSION: std::ops::Range<usize> = 8..16;

fn text(len: usize) -> Vec<u8> {
    let mut state: u64 = 0x2545_F491_4F6C_DD1D;
    let words: [&[u8]; 6] = [
        b"block ", b"index ", b"crush ", b"stream ", b"table ", b"\n",
    ];
    let mut out = Vec::with_capacity(len + 8);
    while out.len() < len {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        out.extend_from_slice(words[(state % 6) as usize]);
    }
    out.truncate(len);
    out
}

fn noise(len: usize) -> Vec<u8> {
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state.to_le_bytes()[0]
        })
        .collect()
}

/// (file name, block size, input)
fn cases() -> Vec<(&'static str, u32, Vec<u8>)> {
    vec![
        ("text_64k.crsh", 65_536, text(300_000)),
        ("text_1m.crsh", 1_048_576, text(1_500_000)),
        ("stored_64k.crsh", 65_536, noise(200_000)),
    ]
}

fn path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join(name)
}

fn config(block_size: u32) -> EngineConfiguration {
    EngineConfiguration::builder()
        .block_size(block_size)
        .workers(2)
        .build()
        .expect("config")
}

fn without_engine_version(bytes: &[u8]) -> Vec<u8> {
    let mut out = bytes.to_vec();
    out[ENGINE_VERSION].fill(0);
    out
}

#[test]
fn golden_files_decompress_byte_for_byte() {
    for (name, block_size, input) in cases() {
        let golden = std::fs::read(path(name)).expect("golden file");
        let output = decompress(&golden, &config(block_size)).expect("decompress");
        assert_eq!(output, input, "{name}");
    }
}

#[test]
fn compression_reproduces_golden_files() {
    for (name, block_size, input) in cases() {
        let golden = std::fs::read(path(name)).expect("golden file");
        let fresh = compress(&input, &config(block_size)).expect("compress");
        assert_eq!(
            without_engine_version(&fresh),
            without_engine_version(&golden),
            "{name}"
        );
    }
}

#[test]
fn stored_case_really_stores_blocks() {
    let (name, _, input) = cases().remove(2);
    let golden = std::fs::read(path(name)).expect("golden file");
    assert!(golden.len() > input.len(), "{name} should not shrink");
}

#[test]
#[ignore = "writes golden files; see the module docs"]
fn regenerate() {
    std::fs::create_dir_all(path("")).expect("mkdir");
    for (name, block_size, input) in cases() {
        let bytes = compress(&input, &config(block_size)).expect("compress");
        std::fs::write(path(name), bytes).expect("write");
    }
}
